//! Giao diện Muse bằng egui: đăng nhập, trang chủ, tìm kiếm, yêu thích, lịch sử, hàng đợi, playlist, thanh phát nhạc.
//! Mọi lời gọi mạng chạy trong thread riêng và trả kết quả về UI qua kênh `Msg`.

use crate::api::{Api, Playlist, Track, User};
use crate::player::Player;
use eframe::egui::{self, vec2, Align, Color32, Layout, Margin, RichText, Sense};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

// Bảng màu bám theo bản web (nền tối tím, accent lấy từ hồ sơ người dùng).
const BG: Color32 = Color32::from_rgb(0x14, 0x11, 0x1c);
const PANEL: Color32 = Color32::from_rgb(0x1c, 0x19, 0x25);
const CARD: Color32 = Color32::from_rgb(0x25, 0x21, 0x2f);
const TEXT: Color32 = Color32::from_rgb(0xe8, 0xe6, 0xee);
const MUTED: Color32 = Color32::from_rgb(0x9a, 0x96, 0xa8);
const DEFAULT_ACCENT: Color32 = Color32::from_rgb(0x31, 0xc2, 0x7c);

// ---------- Cấu hình lưu trên máy ----------
#[derive(Default, Serialize, Deserialize)]
struct Config {
    server: String,
    token: String,
}

fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("Muse").join("native.json"))
}

fn load_config() -> Config {
    config_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_config(c: &Config) {
    if let Some(p) = config_path() {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(s) = serde_json::to_string_pretty(c) {
            let _ = std::fs::write(p, s);
        }
    }
}

// ---------- Kiểu dữ liệu nội bộ ----------
#[derive(Clone, PartialEq)]
enum Page {
    Home,
    Search,
    Favorites,
    History,
    Queue,
    Playlist(i64),
}

enum Msg {
    LoggedIn(Result<(String, User), String>),
    Search(Result<Vec<Track>, String>),
    Favs(Vec<Track>),
    History(Vec<Track>),
    Recs(Vec<Track>),
    Playlists(Vec<Playlist>),
    PlaylistTracks(i64, String, Vec<Track>),
    Audio(u64, Result<Vec<u8>, String>),
    Toast(String),
    Notice { text: String, refresh_pl: bool },
    Noop,
}

enum Action {
    Play(Vec<Track>, usize),
    Enqueue(Track),
    Fav(Track),
    AddPl(i64, Track),
    RemovePl(i64, Track),
    JumpQueue(usize),
    RemoveQueue(usize),
}

struct RowEnv<'a> {
    favs: &'a HashSet<String>,
    playlists: &'a [Playlist],
    current: Option<String>,
    in_playlist: Option<i64>,
    in_queue: bool,
    accent: Color32,
}

pub struct MuseApp {
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    cfg: Config,
    api: Option<Api>,
    user: Option<User>,
    accent: Color32,

    // Đăng nhập
    server_url: String,
    username: String,
    password: String,
    login_busy: bool,
    login_err: String,

    page: Page,
    search_text: String,
    search_results: Vec<Track>,
    searching: bool,
    favs: Vec<Track>,
    fav_keys: HashSet<String>,
    history: Vec<Track>,
    recs: Vec<Track>,
    playlists: Vec<Playlist>,
    open_playlist: Option<(i64, String, Vec<Track>)>,
    new_pl_name: String,

    queue: Vec<Track>,
    cur: Option<usize>,
    player: Player,
    loading: bool,
    play_gen: u64,
    repeat: bool,
    /// Vị trí cuộn ngang của hàng thẻ "Nhạc mới mỗi ngày".
    carousel_x: f32,
    toast: Option<(String, Instant)>,
}

impl MuseApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_fonts(&cc.egui_ctx);
        apply_theme(&cc.egui_ctx, DEFAULT_ACCENT);
        let (tx, rx) = channel();
        let cfg = load_config();
        // Mặc định: server chạy trên máy desktop-p2k8jsq, truy cập qua Tailscale (cổng 3000).
        // Có thể đổi lúc build bằng biến môi trường MUSE_URL, hoặc sửa ngay ở màn hình đăng nhập.
        let default_server = option_env!("MUSE_URL")
            .filter(|s| !s.is_empty())
            .unwrap_or("http://100.112.197.104:3000")
            .to_string();
        let server_url = if cfg.server.is_empty() {
            default_server
        } else {
            cfg.server.clone()
        };
        let mut app = MuseApp {
            tx,
            rx,
            cfg,
            api: None,
            user: None,
            accent: DEFAULT_ACCENT,
            server_url,
            username: String::new(),
            password: String::new(),
            login_busy: false,
            login_err: String::new(),
            page: Page::Home,
            search_text: String::new(),
            search_results: Vec::new(),
            searching: false,
            favs: Vec::new(),
            fav_keys: HashSet::new(),
            history: Vec::new(),
            recs: Vec::new(),
            playlists: Vec::new(),
            open_playlist: None,
            new_pl_name: String::new(),
            queue: Vec::new(),
            cur: None,
            player: Player::new(),
            loading: false,
            play_gen: 0,
            repeat: false,
            carousel_x: 0.0,
            toast: None,
        };
        app.try_auto_login(&cc.egui_ctx);
        app
    }

    // ---------- Chạy việc nền ----------
    fn spawn_job<T: Send + 'static>(
        &self,
        ctx: &egui::Context,
        job: impl FnOnce(Api) -> T + Send + 'static,
        wrap: impl FnOnce(T) -> Msg + Send + 'static,
    ) {
        let Some(api) = self.api.clone() else { return };
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let r = job(api);
            let _ = tx.send(wrap(r));
            ctx.request_repaint();
        });
    }

    fn toast(&mut self, s: impl Into<String>) {
        self.toast = Some((s.into(), Instant::now()));
    }

    // ---------- Đăng nhập ----------
    fn try_auto_login(&mut self, ctx: &egui::Context) {
        if self.cfg.token.is_empty() || self.cfg.server.is_empty() {
            return;
        }
        self.login_busy = true;
        let (server, token) = (self.cfg.server.clone(), self.cfg.token.clone());
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let r = Api::new(&server, &token).me().map(|u| (token.clone(), u));
            let _ = tx.send(Msg::LoggedIn(r));
            ctx.request_repaint();
        });
    }

    fn do_login(&mut self, ctx: &egui::Context, register: bool) {
        if self.login_busy {
            return;
        }
        if self.username.trim().is_empty() || self.password.is_empty() {
            self.login_err = "Nhập tên đăng nhập và mật khẩu".into();
            return;
        }
        self.login_busy = true;
        self.login_err.clear();
        let (base, u, p) = (
            self.server_url.clone(),
            self.username.trim().to_string(),
            self.password.clone(),
        );
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let api = Api::new(&base, "");
            let r = if register {
                api.register(&u, &p)
            } else {
                api.login(&u, &p)
            };
            let _ = tx.send(Msg::LoggedIn(r));
            ctx.request_repaint();
        });
    }

    fn logout(&mut self, ctx: &egui::Context) {
        self.spawn_job(ctx, |a| a.logout(), |_| Msg::Noop);
        self.player.stop();
        self.queue.clear();
        self.cur = None;
        self.cfg.token.clear();
        save_config(&self.cfg);
        self.api = None;
        self.user = None;
        self.password.clear();
        self.favs.clear();
        self.fav_keys.clear();
        self.history.clear();
        self.recs.clear();
        self.playlists.clear();
        self.open_playlist = None;
        self.search_results.clear();
        apply_theme(ctx, DEFAULT_ACCENT);
    }

    fn load_all(&mut self, ctx: &egui::Context) {
        self.load_favs(ctx);
        self.load_history(ctx);
        self.load_playlists(ctx);
        self.spawn_job(
            ctx,
            |a| a.recommend(),
            |r| match r {
                Ok(v) => Msg::Recs(v),
                Err(_) => Msg::Recs(Vec::new()),
            },
        );
    }

    fn load_favs(&self, ctx: &egui::Context) {
        self.spawn_job(
            ctx,
            |a| a.favorites(),
            |r| match r {
                Ok(v) => Msg::Favs(v),
                Err(e) => Msg::Toast(e),
            },
        );
    }

    fn load_history(&self, ctx: &egui::Context) {
        self.spawn_job(
            ctx,
            |a| a.history(),
            |r| match r {
                Ok(v) => Msg::History(v),
                Err(e) => Msg::Toast(e),
            },
        );
    }

    fn load_playlists(&self, ctx: &egui::Context) {
        self.spawn_job(
            ctx,
            |a| a.playlists(),
            |r| match r {
                Ok(v) => Msg::Playlists(v),
                Err(e) => Msg::Toast(e),
            },
        );
    }

    fn load_playlist(&self, ctx: &egui::Context, id: i64) {
        self.spawn_job(
            ctx,
            move |a| a.playlist(id),
            move |r| match r {
                Ok((name, tracks)) => Msg::PlaylistTracks(id, name, tracks),
                Err(e) => Msg::Toast(e),
            },
        );
    }

    // ---------- Nhận kết quả từ thread ----------
    fn pump(&mut self, ctx: &egui::Context) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::LoggedIn(r) => {
                    self.login_busy = false;
                    match r {
                        Ok((token, user)) => {
                            let base = self.server_url.trim().trim_end_matches('/').to_string();
                            self.cfg.server = base.clone();
                            self.cfg.token = token.clone();
                            save_config(&self.cfg);
                            self.api = Some(Api::new(&base, &token));
                            self.accent = parse_hex(&user.accent, DEFAULT_ACCENT);
                            apply_theme(ctx, self.accent);
                            self.user = Some(user);
                            self.password.clear();
                            self.page = Page::Home;
                            self.load_all(ctx);
                        }
                        Err(e) => self.login_err = e,
                    }
                }
                Msg::Search(r) => {
                    self.searching = false;
                    match r {
                        Ok(v) => self.search_results = v,
                        Err(e) => self.toast(e),
                    }
                }
                Msg::Favs(v) => {
                    self.fav_keys = v.iter().map(|t| t.key()).collect();
                    self.favs = v;
                }
                Msg::History(v) => self.history = v,
                Msg::Recs(v) => self.recs = v,
                Msg::Playlists(v) => self.playlists = v,
                Msg::PlaylistTracks(id, name, tracks) => {
                    self.open_playlist = Some((id, name, tracks))
                }
                Msg::Audio(gen, r) => {
                    if gen != self.play_gen {
                        continue;
                    }
                    self.loading = false;
                    match r {
                        Ok(bytes) => {
                            let Some(t) = self.cur.and_then(|i| self.queue.get(i)).cloned() else {
                                continue;
                            };
                            match self.player.load(bytes, t.dur) {
                                Ok(()) => {
                                    self.spawn_job(ctx, move |a| a.add_history(&t), |_| Msg::Noop);
                                }
                                Err(e) => self.toast(e),
                            }
                        }
                        Err(e) => self.toast(e),
                    }
                }
                Msg::Toast(s) => self.toast(s),
                Msg::Notice { text, refresh_pl } => {
                    self.toast(text);
                    if refresh_pl {
                        self.load_playlists(ctx);
                    }
                }
                Msg::Noop => {}
            }
        }
    }

    // ---------- Phát nhạc ----------
    fn play_list(&mut self, ctx: &egui::Context, list: Vec<Track>, idx: usize) {
        self.queue = list;
        self.play_index(ctx, idx);
    }

    fn play_index(&mut self, ctx: &egui::Context, idx: usize) {
        let Some(t) = self.queue.get(idx).cloned() else {
            return;
        };
        self.cur = Some(idx);
        self.player.stop();
        self.play_gen += 1;
        self.loading = true;
        let gen = self.play_gen;
        self.spawn_job(
            ctx,
            move |a| a.download_audio(&t),
            move |r| Msg::Audio(gen, r),
        );
    }

    fn next(&mut self, ctx: &egui::Context) {
        let Some(c) = self.cur else { return };
        if c + 1 < self.queue.len() {
            self.play_index(ctx, c + 1);
        } else if self.repeat && !self.queue.is_empty() {
            self.play_index(ctx, 0);
        } else {
            self.player.stop();
        }
    }

    fn prev(&mut self, ctx: &egui::Context) {
        let Some(c) = self.cur else { return };
        if self.player.position() > 3.0 || c == 0 {
            self.player.seek(0.0);
        } else {
            self.play_index(ctx, c - 1);
        }
    }

    fn toggle_fav(&mut self, ctx: &egui::Context, t: Track) {
        let key = t.key();
        if self.fav_keys.remove(&key) {
            self.favs.retain(|x| x.key() != key);
        } else {
            self.fav_keys.insert(key);
            self.favs.insert(0, t.clone());
        }
        self.spawn_job(
            ctx,
            move |a| a.toggle_fav(&t),
            |r| match r {
                Ok(_) => Msg::Noop,
                Err(e) => Msg::Toast(e),
            },
        );
    }

    fn goto(&mut self, ctx: &egui::Context, page: Page) {
        if self.page == page {
            return;
        }
        // Giải phóng ảnh đã nạp ở trang cũ để giữ RAM thấp.
        ctx.forget_all_images();
        match &page {
            Page::History => self.load_history(ctx),
            Page::Playlist(id) => {
                self.open_playlist = None;
                self.load_playlist(ctx, *id);
            }
            _ => {}
        }
        self.page = page;
    }

    fn do_search(&mut self, ctx: &egui::Context) {
        let q = self.search_text.trim().to_string();
        if q.is_empty() || self.searching {
            return;
        }
        self.searching = true;
        ctx.forget_all_images();
        self.spawn_job(ctx, move |a| a.search(&q), Msg::Search);
    }

    fn apply(&mut self, ctx: &egui::Context, actions: Vec<Action>) {
        for a in actions {
            match a {
                Action::Play(list, idx) => self.play_list(ctx, list, idx),
                Action::Enqueue(t) => {
                    self.queue.push(t);
                    self.toast("Đã thêm vào hàng đợi");
                }
                Action::Fav(t) => self.toggle_fav(ctx, t),
                Action::AddPl(pid, t) => {
                    self.spawn_job(
                        ctx,
                        move |api| api.add_to_playlist(pid, &t),
                        |r| match r {
                            Ok(()) => Msg::Notice {
                                text: "Đã thêm vào playlist".into(),
                                refresh_pl: true,
                            },
                            Err(e) => Msg::Toast(e),
                        },
                    );
                }
                Action::RemovePl(pid, t) => {
                    let key = t.key();
                    if let Some((id, _, tracks)) = &mut self.open_playlist {
                        if *id == pid {
                            tracks.retain(|x| x.key() != key);
                        }
                    }
                    self.spawn_job(
                        ctx,
                        move |api| api.remove_from_playlist(pid, &t),
                        |r| match r {
                            Ok(()) => Msg::Notice {
                                text: "Đã xóa khỏi playlist".into(),
                                refresh_pl: true,
                            },
                            Err(e) => Msg::Toast(e),
                        },
                    );
                }
                Action::JumpQueue(i) => self.play_index(ctx, i),
                Action::RemoveQueue(i) => {
                    if i < self.queue.len() {
                        self.queue.remove(i);
                        match self.cur {
                            Some(c) if c == i => {
                                self.player.stop();
                                self.cur = None;
                            }
                            Some(c) if c > i => self.cur = Some(c - 1),
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    // ---------- Giao diện ----------
    fn login_ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(90.0);
                    ui.label(RichText::new("Muse").size(40.0).strong().color(TEXT));
                    ui.label(RichText::new("Nghe nhạc cùng bạn bè").color(MUTED));
                    ui.add_space(24.0);

                    let mut submit = false;
                    ui.add(
                        egui::TextEdit::singleline(&mut self.server_url)
                            .hint_text("Địa chỉ máy chủ")
                            .desired_width(320.0),
                    );
                    ui.add_space(6.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.username)
                            .hint_text("Tên đăng nhập")
                            .desired_width(320.0),
                    );
                    ui.add_space(6.0);
                    let pw = ui.add(
                        egui::TextEdit::singleline(&mut self.password)
                            .password(true)
                            .hint_text("Mật khẩu")
                            .desired_width(320.0),
                    );
                    if pw.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                        submit = true;
                    }
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        ui.add_space((ui.available_width() - 250.0).max(0.0) / 2.0);
                        if ui
                            .add_sized(
                                [120.0, 34.0],
                                egui::Button::new("Đăng nhập").fill(self.accent),
                            )
                            .clicked()
                        {
                            submit = true;
                        }
                        if ui
                            .add_sized([120.0, 34.0], egui::Button::new("Đăng ký"))
                            .clicked()
                            && !self.login_busy
                        {
                            self.do_login(ctx, true);
                        }
                    });
                    if submit {
                        self.do_login(ctx, false);
                    }
                    ui.add_space(12.0);
                    if self.login_busy {
                        ui.spinner();
                    }
                    if !self.login_err.is_empty() {
                        ui.label(
                            RichText::new(&self.login_err)
                                .color(Color32::from_rgb(0xff, 0x6b, 0x6b)),
                        );
                    }
                });
            });
    }

    fn sidebar(&mut self, ctx: &egui::Context) {
        let mut nav: Option<Page> = None;
        let mut create_pl = false;
        let mut do_logout = false;
        let base = self
            .api
            .as_ref()
            .map(|a| a.base.clone())
            .unwrap_or_default();

        egui::SidePanel::left("side")
            .exact_width(230.0)
            .frame(
                egui::Frame::none()
                    .fill(PANEL)
                    .inner_margin(Margin::same(12.0)),
            )
            .show(ctx, |ui| {
                ui.label(RichText::new("Muse").size(24.0).strong().color(TEXT));
                ui.add_space(10.0);
                let w = ui.available_width();
                let items = [
                    ("Trang chủ", Page::Home),
                    ("Tìm kiếm", Page::Search),
                    ("Yêu thích", Page::Favorites),
                    ("Lịch sử", Page::History),
                    ("Hàng đợi", Page::Queue),
                ];
                for (label, page) in items {
                    let sel = self.page == page;
                    let btn = egui::Button::new(RichText::new(label).color(if sel {
                        Color32::WHITE
                    } else {
                        TEXT
                    }))
                    .fill(if sel {
                        self.accent.gamma_multiply(0.55)
                    } else {
                        Color32::TRANSPARENT
                    })
                    .min_size(vec2(w, 34.0));
                    if ui.add(btn).clicked() {
                        nav = Some(page);
                    }
                }
                ui.add_space(12.0);
                ui.label(RichText::new("PLAYLIST").size(11.0).color(MUTED));
                ui.horizontal(|ui| {
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.new_pl_name)
                            .hint_text("Playlist mới")
                            .desired_width(w - 44.0),
                    );
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.button("+").clicked() || enter {
                        create_pl = true;
                    }
                });
                let list_h = (ui.available_height() - 70.0).max(60.0);
                egui::ScrollArea::vertical()
                    .max_height(list_h)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for p in &self.playlists {
                            let sel = self.page == Page::Playlist(p.id);
                            let text = format!("{}  ({})", clip(&p.name, 20), p.count);
                            let btn = egui::Button::new(RichText::new(text).color(if sel {
                                Color32::WHITE
                            } else {
                                TEXT
                            }))
                            .fill(if sel {
                                self.accent.gamma_multiply(0.55)
                            } else {
                                Color32::TRANSPARENT
                            })
                            .min_size(vec2(w, 30.0));
                            if ui.add(btn).clicked() {
                                nav = Some(Page::Playlist(p.id));
                            }
                        }
                    });
                ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                    if ui.button("Đăng xuất").clicked() {
                        do_logout = true;
                    }
                    if let Some(u) = &self.user {
                        ui.horizontal(|ui| {
                            avatar_ui(ui, &u.avatar, &base, 30.0);
                            ui.label(RichText::new(clip(&u.display_name, 18)).color(TEXT));
                        });
                    }
                });
            });

        if create_pl {
            let name = self.new_pl_name.trim().to_string();
            if !name.is_empty() {
                self.new_pl_name.clear();
                self.spawn_job(
                    ctx,
                    move |a| a.create_playlist(&name),
                    |r| match r {
                        Ok(()) => Msg::Notice {
                            text: "Đã tạo playlist".into(),
                            refresh_pl: true,
                        },
                        Err(e) => Msg::Toast(e),
                    },
                );
            }
        }
        if do_logout {
            self.logout(ctx);
            return;
        }
        if let Some(p) = nav {
            self.goto(ctx, p);
        }
    }

    fn player_bar(&mut self, ctx: &egui::Context) {
        let cur_track = self.cur.and_then(|i| self.queue.get(i)).cloned();
        let mut do_prev = false;
        let mut do_next = false;
        let mut do_toggle = false;
        let mut seek_to: Option<f64> = None;
        let mut fav_toggle: Option<Track> = None;
        let mut go_queue = false;

        egui::TopBottomPanel::bottom("bar")
            .exact_height(88.0)
            .frame(
                egui::Frame::none()
                    .fill(PANEL)
                    .inner_margin(Margin::symmetric(16.0, 10.0)),
            )
            .show(ctx, |ui| {
                ui.columns(3, |cols| {
                    // Cột trái: bài đang phát
                    cols[0].horizontal_centered(|ui| {
                        if let Some(t) = &cur_track {
                            thumb_ui(ui, &t.image, 56.0, 8.0, 160);
                            ui.vertical(|ui| {
                                ui.label(RichText::new(clip(&t.title, 28)).strong().color(TEXT));
                                ui.label(
                                    RichText::new(clip(&t.artist, 30)).size(12.0).color(MUTED),
                                );
                            });
                            let faved = self.fav_keys.contains(&t.key());
                            let heart = RichText::new("♥").size(20.0).color(if faved {
                                self.accent
                            } else {
                                MUTED
                            });
                            if ui.add(egui::Button::new(heart).frame(false)).clicked() {
                                fav_toggle = Some(t.clone());
                            }
                        } else {
                            ui.label(RichText::new("Chưa phát bài nào").color(MUTED));
                        }
                    });

                    // Cột giữa: điều khiển + thanh tua
                    cols[1].vertical_centered(|ui| {
                        ui.horizontal(|ui| {
                            ui.add_space((ui.available_width() - 150.0).max(0.0) / 2.0);
                            if ui
                                .add(egui::Button::new(RichText::new("⏮").size(20.0)).frame(false))
                                .clicked()
                            {
                                do_prev = true;
                            }
                            let icon = if self.loading {
                                "…"
                            } else if self.player.is_playing() {
                                "⏸"
                            } else {
                                "▶"
                            };
                            if ui
                                .add_sized(
                                    [44.0, 36.0],
                                    egui::Button::new(RichText::new(icon).size(22.0))
                                        .fill(self.accent),
                                )
                                .clicked()
                            {
                                do_toggle = true;
                            }
                            if ui
                                .add(egui::Button::new(RichText::new("⏭").size(20.0)).frame(false))
                                .clicked()
                            {
                                do_next = true;
                            }
                        });
                        let dur = self.player.duration.max(1.0) as f32;
                        let mut pos = self.player.position() as f32;
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(fmt_time(pos as f64)).size(12.0).color(MUTED));
                            ui.spacing_mut().slider_width = (ui.available_width() - 60.0).max(80.0);
                            let r =
                                ui.add(egui::Slider::new(&mut pos, 0.0..=dur).show_value(false));
                            if r.drag_stopped() || r.clicked() {
                                seek_to = Some(pos as f64);
                            }
                            ui.label(
                                RichText::new(fmt_time(self.player.duration))
                                    .size(12.0)
                                    .color(MUTED),
                            );
                        });
                    });

                    // Cột phải: lặp, hàng đợi, âm lượng
                    cols[2].with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let mut vol = self.player.volume;
                        ui.spacing_mut().slider_width = 110.0;
                        if ui
                            .add(egui::Slider::new(&mut vol, 0.0..=1.0).show_value(false))
                            .changed()
                        {
                            self.player.set_volume(vol);
                        }
                        ui.label(RichText::new("🔊").color(MUTED));
                        let rep = RichText::new("🔁").size(18.0).color(if self.repeat {
                            self.accent
                        } else {
                            MUTED
                        });
                        if ui
                            .add(egui::Button::new(rep).frame(false))
                            .on_hover_text("Lặp lại hàng đợi")
                            .clicked()
                        {
                            self.repeat = !self.repeat;
                        }
                        if ui
                            .add(
                                egui::Button::new(RichText::new("☰").size(18.0).color(MUTED))
                                    .frame(false),
                            )
                            .on_hover_text("Hàng đợi")
                            .clicked()
                        {
                            go_queue = true;
                        }
                    });
                });
            });

        if do_toggle {
            if self.player.has_track() {
                self.player.toggle();
            } else if let Some(c) = self.cur {
                self.play_index(ctx, c);
            }
        }
        if do_prev {
            self.prev(ctx);
        }
        if do_next {
            self.next(ctx);
        }
        if let Some(s) = seek_to {
            self.player.seek(s);
        }
        if let Some(t) = fav_toggle {
            self.toggle_fav(ctx, t);
        }
        if go_queue {
            self.goto(ctx, Page::Queue);
        }
    }

    fn main_panel(&mut self, ctx: &egui::Context) {
        let mut actions: Vec<Action> = Vec::new();
        let mut do_search = false;
        let mut delete_pl: Option<i64> = None;
        let current = self.cur.and_then(|i| self.queue.get(i)).map(|t| t.key());
        let mut carousel_x = self.carousel_x;

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG).inner_margin(Margin::same(16.0)))
            .show(ctx, |ui| {
                let env = RowEnv {
                    favs: &self.fav_keys,
                    playlists: &self.playlists,
                    current,
                    in_playlist: if let Page::Playlist(id) = self.page { Some(id) } else { None },
                    in_queue: self.page == Page::Queue,
                    accent: self.accent,
                };
                match &self.page {
                    Page::Home => {
                        let has_recs = !self.recs.is_empty();
                        if has_recs {
                            let n = self.recs.len().min(12);
                            featured_row(ui, "Nhạc mới mỗi ngày", &self.recs[..n], &env, &mut actions, &mut carousel_x);
                            ui.add_space(16.0);
                        }
                        if !self.history.is_empty() {
                            heading(ui, "Nghe gần đây");
                            track_list(ui, &self.history, &env, &mut actions);
                        } else if !has_recs {
                            hint(ui, "Chưa có gợi ý. Hãy tìm bài hát, thêm vào yêu thích hoặc playlist để bắt đầu.");
                        }
                    }
                    Page::Search => {
                        heading(ui, "Tìm kiếm");
                        ui.horizontal(|ui| {
                            let r = ui.add(
                                egui::TextEdit::singleline(&mut self.search_text)
                                    .hint_text("Bài hát, nghệ sĩ...")
                                    .desired_width((ui.available_width() - 110.0).max(120.0)),
                            );
                            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            if ui.button("Tìm").clicked() || enter {
                                do_search = true;
                            }
                        });
                        ui.add_space(8.0);
                        if self.searching {
                            ui.spinner();
                        } else if self.search_results.is_empty() {
                            hint(ui, "Nhập từ khoá rồi nhấn Enter.");
                        } else {
                            track_list(ui, &self.search_results, &env, &mut actions);
                        }
                    }
                    Page::Favorites => {
                        heading(ui, "Yêu thích");
                        if self.favs.is_empty() {
                            hint(ui, "Chưa có bài yêu thích. Bấm ♥ ở một bài bất kỳ.");
                        } else {
                            track_list(ui, &self.favs, &env, &mut actions);
                        }
                    }
                    Page::History => {
                        heading(ui, "Lịch sử nghe");
                        if self.history.is_empty() {
                            hint(ui, "Chưa nghe bài nào.");
                        } else {
                            track_list(ui, &self.history, &env, &mut actions);
                        }
                    }
                    Page::Queue => {
                        heading(ui, "Hàng đợi");
                        if self.queue.is_empty() {
                            hint(ui, "Hàng đợi trống.");
                        } else {
                            track_list(ui, &self.queue, &env, &mut actions);
                        }
                    }
                    Page::Playlist(id) => match &self.open_playlist {
                        Some((pid, name, tracks)) if pid == id => {
                            ui.horizontal(|ui| {
                                heading(ui, name);
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.button("Xóa playlist").clicked() {
                                        delete_pl = Some(*id);
                                    }
                                });
                            });
                            if tracks.is_empty() {
                                hint(ui, "Playlist trống. Bấm + ở một bài để thêm vào đây.");
                            } else {
                                track_list(ui, tracks, &env, &mut actions);
                            }
                        }
                        _ => {
                            ui.spinner();
                        }
                    },
                }
            });

        self.carousel_x = carousel_x;
        if do_search {
            self.do_search(ctx);
        }
        if let Some(id) = delete_pl {
            self.spawn_job(
                ctx,
                move |a| a.delete_playlist(id),
                |r| match r {
                    Ok(()) => Msg::Notice {
                        text: "Đã xóa playlist".into(),
                        refresh_pl: true,
                    },
                    Err(e) => Msg::Toast(e),
                },
            );
            self.playlists.retain(|p| p.id != id);
            self.page = Page::Home;
            self.open_playlist = None;
        }
        self.apply(ctx, actions);
    }

    fn toast_ui(&mut self, ctx: &egui::Context) {
        let expired = self
            .toast
            .as_ref()
            .map(|(_, at)| at.elapsed() > Duration::from_secs(4))
            .unwrap_or(false);
        if expired {
            self.toast = None;
        }
        if let Some((text, _)) = &self.toast {
            egui::Area::new(egui::Id::new("toast"))
                .anchor(egui::Align2::RIGHT_BOTTOM, [-16.0, -100.0])
                .show(ctx, |ui| {
                    egui::Frame::none()
                        .fill(CARD)
                        .rounding(10.0)
                        .inner_margin(Margin::symmetric(14.0, 10.0))
                        .show(ui, |ui| {
                            ui.label(RichText::new(text).color(TEXT));
                        });
                });
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }
}

impl eframe::App for MuseApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump(ctx);

        if self.user.is_none() {
            self.login_ui(ctx);
            self.toast_ui(ctx);
            return;
        }

        if self.player.finished() {
            self.next(ctx);
        }
        if self.player.is_playing() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        // Phím cách: phát/tạm dừng khi không đang gõ chữ.
        if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Space)) {
            self.player.toggle();
        }

        // Thanh phát nhạc vẽ trước để trải hết chiều ngang, nằm dưới cả thanh bên.
        self.player_bar(ctx);
        self.sidebar(ctx);
        if self.user.is_none() {
            return; // vừa đăng xuất
        }
        self.main_panel(ctx);
        self.toast_ui(ctx);
    }
}

// ---------- Hàm vẽ phụ ----------
fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(24.0).strong().color(TEXT));
    ui.add_space(8.0);
}

fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(MUTED));
}

fn track_list(ui: &mut egui::Ui, tracks: &[Track], env: &RowEnv, actions: &mut Vec<Action>) {
    let row_h = 56.0;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show_rows(ui, row_h, tracks.len(), |ui, range| {
            for i in range {
                let t = &tracks[i];
                let key = t.key();
                let is_cur = env.current.as_deref() == Some(key.as_str());
                let max_chars = (((ui.available_width() - 250.0) / 7.0).max(10.0)) as usize;
                ui.allocate_ui_with_layout(
                    vec2(ui.available_width(), row_h),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        let r = ui.max_rect();
                        let bg = if is_cur {
                            env.accent.gamma_multiply(0.3)
                        } else {
                            CARD
                        };
                        ui.painter().rect_filled(r, 8.0, bg);
                        ui.add_space(8.0);
                        thumb_ui(ui, &t.image, 44.0, 6.0, 96);
                        ui.vertical(|ui| {
                            let title = if t.title.is_empty() {
                                "(không tên)"
                            } else {
                                t.title.as_str()
                            };
                            ui.label(RichText::new(clip(title, max_chars)).strong().color(TEXT));
                            ui.label(
                                RichText::new(clip(&t.artist, max_chars))
                                    .size(12.0)
                                    .color(MUTED),
                            );
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_space(8.0);
                            if t.dur > 0.0 {
                                ui.label(RichText::new(fmt_time(t.dur)).size(12.0).color(MUTED));
                            }
                            let faved = env.favs.contains(&key);
                            let heart = RichText::new("♥").size(18.0).color(if faved {
                                env.accent
                            } else {
                                MUTED
                            });
                            if ui
                                .add(egui::Button::new(heart).frame(false))
                                .on_hover_text("Yêu thích")
                                .clicked()
                            {
                                actions.push(Action::Fav(t.clone()));
                            }
                            if let Some(pid) = env.in_playlist {
                                if ui
                                    .add(
                                        egui::Button::new(RichText::new("✕").color(MUTED))
                                            .frame(false),
                                    )
                                    .on_hover_text("Xóa khỏi playlist")
                                    .clicked()
                                {
                                    actions.push(Action::RemovePl(pid, t.clone()));
                                }
                            } else if env.in_queue {
                                if ui
                                    .add(
                                        egui::Button::new(RichText::new("✕").color(MUTED))
                                            .frame(false),
                                    )
                                    .on_hover_text("Bỏ khỏi hàng đợi")
                                    .clicked()
                                {
                                    actions.push(Action::RemoveQueue(i));
                                }
                            }
                            ui.menu_button(RichText::new("+").size(18.0).color(MUTED), |ui| {
                                if ui.button("Thêm vào hàng đợi").clicked() {
                                    actions.push(Action::Enqueue(t.clone()));
                                    ui.close_menu();
                                }
                                ui.separator();
                                if env.playlists.is_empty() {
                                    ui.label(RichText::new("Chưa có playlist").color(MUTED));
                                }
                                for p in env.playlists {
                                    if ui.button(clip(&p.name, 28)).clicked() {
                                        actions.push(Action::AddPl(p.id, t.clone()));
                                        ui.close_menu();
                                    }
                                }
                            });
                            let play = RichText::new("▶").size(18.0).color(if is_cur {
                                env.accent
                            } else {
                                TEXT
                            });
                            if ui
                                .add(egui::Button::new(play).frame(false))
                                .on_hover_text("Phát")
                                .clicked()
                            {
                                if env.in_queue {
                                    actions.push(Action::JumpQueue(i));
                                } else {
                                    actions.push(Action::Play(tracks.to_vec(), i));
                                }
                            }
                        });
                    },
                );
            }
        });
}

/// Vẽ ảnh bìa lấp đầy `rect`, cắt phần giữa cho vừa khung vuông (không méo ảnh).
fn cover_image(ui: &mut egui::Ui, url: &str, rect: egui::Rect, round: f32) {
    let placeholder = |ui: &egui::Ui| {
        ui.painter()
            .rect_filled(rect, round, Color32::from_rgb(0x33, 0x2e, 0x40));
    };
    if url.is_empty() {
        placeholder(ui);
        return;
    }
    match ui
        .ctx()
        .try_load_texture(url, egui::TextureOptions::LINEAR, egui::SizeHint::default())
    {
        Ok(egui::load::TexturePoll::Ready { texture }) => {
            let (w, h) = (texture.size.x.max(1.0), texture.size.y.max(1.0));
            let (mx, my) = if w > h {
                ((w - h) / (2.0 * w), 0.0)
            } else {
                (0.0, (h - w) / (2.0 * h))
            };
            let uv = egui::Rect::from_min_max(egui::pos2(mx, my), egui::pos2(1.0 - mx, 1.0 - my));
            egui::Image::from_texture(texture)
                .uv(uv)
                .rounding(round)
                .paint_at(ui, rect);
        }
        Ok(egui::load::TexturePoll::Pending { .. }) => {
            placeholder(ui);
            ui.ctx().request_repaint_after(Duration::from_millis(120));
        }
        Err(_) => placeholder(ui),
    }
}

/// Vẽ một dòng chữ một hàng, tự cắt bằng "…" khi quá rộng (đúng cả với tiếng Nhật không có dấu cách).
fn overlay_text(
    ui: &egui::Ui,
    pos: egui::Pos2,
    text: &str,
    size: f32,
    color: Color32,
    width: f32,
    italic: bool,
) {
    let mut job = egui::text::LayoutJob::simple(
        text.to_string(),
        egui::FontId::proportional(size),
        color,
        width,
    );
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');
    if let Some(sec) = job.sections.first_mut() {
        sec.format.italics = italic;
    }
    let galley = ui.fonts(|f| f.layout_job(job));
    ui.painter().galley(pos, galley, color);
}

/// Nền chuyển màu theo chiều dọc (đen mờ dần), dùng để chữ đọc rõ trên ảnh.
fn vgradient(ui: &egui::Ui, rect: egui::Rect, top: Color32, bottom: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(egui::Shape::mesh(mesh));
}

/// Hàng thẻ lớn cuộn ngang kiểu YouTube Music: ảnh bìa, tên bài + nghệ sĩ ở trên, "Dành cho người hâm mộ ..." ở dưới.
fn featured_row(
    ui: &mut egui::Ui,
    title: &str,
    tracks: &[Track],
    env: &RowEnv,
    actions: &mut Vec<Action>,
    scroll_x: &mut f32,
) {
    let card = ((ui.available_width() - 40.0) * 0.42).clamp(230.0, 340.0);
    let gap = 14.0;
    let round = 16.0;
    let step = card + gap;
    let mut scroll_delta = 0.0f32;

    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(24.0).strong().color(TEXT));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let play_all = egui::Button::new(RichText::new("Phát tất cả").strong().color(TEXT))
                .fill(Color32::TRANSPARENT)
                .stroke(egui::Stroke::new(1.5, MUTED))
                .rounding(20.0);
            if ui.add(play_all).clicked() {
                actions.push(Action::Play(tracks.to_vec(), 0));
            }
            if ui
                .add(egui::Button::new(RichText::new("›").size(22.0)).rounding(20.0))
                .clicked()
            {
                scroll_delta += step;
            }
            if ui
                .add(egui::Button::new(RichText::new("‹").size(22.0)).rounding(20.0))
                .clicked()
            {
                scroll_delta -= step;
            }
        });
    });
    ui.add_space(8.0);

    let target = (*scroll_x + scroll_delta).max(0.0);
    let out = egui::ScrollArea::horizontal()
        .id_salt("featured_row")
        .scroll_offset(egui::vec2(target, 0.0))
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (i, t) in tracks.iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(vec2(card, card), Sense::click());
                    let is_cur = env.current.as_deref() == Some(t.key().as_str());

                    cover_image(ui, &thumb_url(&t.image, 544), rect, round);
                    // Đen mờ ở đầu và cuối thẻ để chữ trắng đọc rõ trên mọi ảnh.
                    let top = egui::Rect::from_min_size(rect.min, vec2(card, 110.0));
                    let bottom = egui::Rect::from_min_max(
                        egui::pos2(rect.min.x, rect.max.y - 90.0),
                        rect.max,
                    );
                    vgradient(
                        ui,
                        top,
                        Color32::from_black_alpha(170),
                        Color32::TRANSPARENT,
                    );
                    vgradient(
                        ui,
                        bottom,
                        Color32::TRANSPARENT,
                        Color32::from_black_alpha(190),
                    );

                    let pad = 18.0;
                    let w = card - pad * 2.0;
                    let title = if t.title.is_empty() {
                        "(không tên)"
                    } else {
                        t.title.as_str()
                    };
                    overlay_text(
                        ui,
                        rect.min + vec2(pad, 16.0),
                        title,
                        17.0,
                        Color32::WHITE,
                        w,
                        false,
                    );
                    overlay_text(
                        ui,
                        rect.min + vec2(pad, 42.0),
                        &t.artist,
                        15.0,
                        Color32::from_white_alpha(185),
                        w,
                        false,
                    );
                    let fan = t.artist.split(',').next().unwrap_or("").trim();
                    if !fan.is_empty() {
                        let caption = format!("Dành cho người hâm mộ {fan}");
                        overlay_text(
                            ui,
                            rect.left_bottom() + vec2(pad, -34.0),
                            &caption,
                            15.0,
                            Color32::from_white_alpha(230),
                            w,
                            true,
                        );
                    }

                    if resp.hovered() || is_cur {
                        ui.painter()
                            .rect_stroke(rect, round, egui::Stroke::new(2.0, env.accent));
                    }
                    if resp.hovered() {
                        let c = rect.right_bottom() - vec2(40.0, 70.0);
                        ui.painter().circle_filled(c, 22.0, env.accent);
                        ui.painter().text(
                            c,
                            egui::Align2::CENTER_CENTER,
                            "▶",
                            egui::FontId::proportional(20.0),
                            Color32::WHITE,
                        );
                    }
                    if resp.clicked() {
                        actions.push(Action::Play(tracks.to_vec(), i));
                    }
                }
            });
        });
    *scroll_x = out.state.offset.x;
}

fn thumb_ui(ui: &mut egui::Ui, url: &str, size: f32, round: f32, px: u32) {
    let u = thumb_url(url, px);
    if u.is_empty() {
        let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
        ui.painter()
            .rect_filled(rect, round, Color32::from_rgb(0x33, 0x2e, 0x40));
    } else {
        ui.add(
            egui::Image::from_uri(u)
                .fit_to_exact_size(vec2(size, size))
                .rounding(round),
        );
    }
}

fn avatar_ui(ui: &mut egui::Ui, avatar: &str, base: &str, size: f32) {
    if avatar.starts_with('/') {
        ui.add(
            egui::Image::from_uri(format!("{base}{avatar}"))
                .fit_to_exact_size(vec2(size, size))
                .rounding(size / 2.0),
        );
    } else if avatar.starts_with("http") {
        ui.add(
            egui::Image::from_uri(avatar.to_string())
                .fit_to_exact_size(vec2(size, size))
                .rounding(size / 2.0),
        );
    } else {
        let a = if avatar.is_empty() { "🎧" } else { avatar };
        ui.label(RichText::new(a).size(size * 0.8));
    }
}

/// Xin ảnh nhỏ từ YouTube/Google để không tốn RAM cho ảnh bìa lớn.
fn thumb_url(url: &str, px: u32) -> String {
    if url.is_empty() {
        return String::new();
    }
    if url.contains("googleusercontent.com") || url.contains("ggpht.com") {
        if let Some(i) = url.rfind("=w") {
            return format!("{}=w{px}-h{px}-l90-rj", &url[..i]);
        }
        if let Some(i) = url.rfind("=s") {
            return format!("{}=s{px}", &url[..i]);
        }
    }
    if url.contains("ytimg.com") {
        if px >= 400 {
            return url.to_string(); // thẻ lớn: giữ nguyên ảnh gốc cho nét
        }
        return url
            .replace("maxresdefault", "mqdefault")
            .replace("sddefault", "mqdefault")
            .replace("hq720", "mqdefault")
            .replace("hqdefault", "mqdefault");
    }
    url.to_string()
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn fmt_time(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

fn parse_hex(s: &str, default: Color32) -> Color32 {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 {
        return default;
    }
    let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    match (p(0), p(2), p(4)) {
        (Some(r), Some(g), Some(b)) => Color32::from_rgb(r, g, b),
        _ => default,
    }
}

fn apply_theme(ctx: &egui::Context, accent: Color32) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.extreme_bg_color = Color32::from_rgb(0x10, 0x0e, 0x16);
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = accent.gamma_multiply(0.6);
    v.widgets.inactive.rounding = egui::Rounding::same(8.0);
    v.widgets.hovered.rounding = egui::Rounding::same(8.0);
    v.widgets.active.rounding = egui::Rounding::same(8.0);
    v.widgets.inactive.weak_bg_fill = CARD;
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x30, 0x2b, 0x3d);
    v.widgets.active.bg_fill = accent;
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = vec2(8.0, 8.0);
        s.spacing.button_padding = vec2(10.0, 6.0);
    });
}

/// Nạp font hệ thống để hiện đủ tiếng Việt và tiếng Nhật (nếu có). Không có thì dùng font mặc định của egui.
fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let dir = std::path::Path::new(r"C:\Windows\Fonts");
    let candidates = [("sys-latin", "segoeui.ttf"), ("sys-jp", "YuGothM.ttc")];
    let mut loaded: Vec<String> = Vec::new();
    for (name, file) in candidates {
        if let Ok(bytes) = std::fs::read(dir.join(file)) {
            fonts
                .font_data
                .insert(name.to_owned(), egui::FontData::from_owned(bytes));
            loaded.push(name.to_owned());
        }
    }
    if !loaded.is_empty() {
        let fam = fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default();
        for (i, name) in loaded.into_iter().enumerate() {
            fam.insert(i, name);
        }
        ctx.set_fonts(fonts);
    }
}
