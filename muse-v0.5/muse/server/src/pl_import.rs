// Nhập playlist từ Spotify / Apple Music / SoundCloud bằng link.
// Muse chỉ phát nhạc từ YouTube, nên mỗi bài được đọc (tên, nghệ sĩ, thời lượng) từ app gốc
// rồi khớp sang một bài trên YouTube Music trước khi lưu thành playlist của người dùng.
//   - Spotify: trang nhúng công khai open.spotify.com/embed/... (tối đa khoảng 100 bài mỗi playlist)
//   - Apple Music: dữ liệu có sẵn trong trang công khai music.apple.com/...
//   - SoundCloud: dùng yt-dlp (đã có sẵn cho phần phát nhạc) để liệt kê bộ sưu tập (/sets/)
use super::*;

/// Số bài tối đa mỗi lần nhập (mỗi bài cần 1-2 lượt tìm trên YouTube Music).
const IMPORT_MAX_TRACKS: usize = 300;
/// Số lượt tìm chạy song song.
const MATCH_CONCURRENCY: usize = 6;

const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Spotify,
    Apple,
    SoundCloud,
    YouTube,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Spotify => "Spotify",
            Kind::Apple => "Apple Music",
            Kind::SoundCloud => "SoundCloud",
            Kind::YouTube => "YouTube",
        }
    }
}

#[derive(Clone, Debug)]
struct SrcTrack {
    title: String,
    artist: String,
    secs: u64,
}

struct SrcList {
    name: String,
    tracks: Vec<SrcTrack>,
}

// ---------- Nhận diện link ----------

/// Trả về (loại app, đường dẫn đã chuẩn hoá để tải). Spotify trả thẳng link nhúng.
fn detect(input: &str) -> Option<(Kind, String)> {
    let s = input.trim();
    // URI của Spotify: spotify:playlist:ID
    if let Some(rest) = s.strip_prefix("spotify:") {
        let mut it = rest.split(':');
        let (kind, id) = (it.next()?, it.next()?);
        if matches!(kind, "playlist" | "album") && spotify_id_ok(id) {
            return Some((Kind::Spotify, format!("https://open.spotify.com/embed/{kind}/{id}")));
        }
        return None;
    }
    let no_scheme = s.strip_prefix("https://").or_else(|| s.strip_prefix("http://")).unwrap_or(s);
    let (host, path) = match no_scheme.find('/') {
        Some(i) => (&no_scheme[..i], &no_scheme[i..]),
        None => (no_scheme, ""),
    };
    let host = host.to_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
    let path_only = path.split(['?', '#']).next().unwrap_or("");
    let segs: Vec<&str> = path_only.split('/').filter(|x| !x.is_empty()).collect();

    match host.as_str() {
        "open.spotify.com" => {
            // /playlist/ID, /album/ID, đôi khi có tiền tố /intl-vi/ hoặc /embed/
            let i = segs.iter().position(|x| matches!(*x, "playlist" | "album"))?;
            let id = segs.get(i + 1)?;
            if !spotify_id_ok(id) {
                return None;
            }
            Some((Kind::Spotify, format!("https://open.spotify.com/embed/{}/{}", segs[i], id)))
        }
        "music.apple.com" | "itunes.apple.com" => {
            if segs.iter().any(|x| matches!(*x, "playlist" | "album")) {
                Some((Kind::Apple, format!("https://{host}{path_only}")))
            } else {
                None
            }
        }
        "soundcloud.com" | "m.soundcloud.com" => {
            if segs.len() >= 3 && segs[1] == "sets" {
                Some((Kind::SoundCloud, format!("https://soundcloud.com{path_only}")))
            } else {
                None
            }
        }
        "on.soundcloud.com" => {
            if segs.is_empty() {
                None
            } else {
                Some((Kind::SoundCloud, format!("https://on.soundcloud.com{path_only}")))
            }
        }
        "youtube.com" | "m.youtube.com" | "music.youtube.com" | "youtu.be" => Some((Kind::YouTube, s.to_string())),
        _ => None,
    }
}

fn spotify_id_ok(id: &str) -> bool {
    !id.is_empty() && id.len() <= 40 && id.chars().all(|c| c.is_ascii_alphanumeric())
}

// ---------- Tiện ích đọc HTML ----------

fn html_unescape(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Lấy nội dung JSON của thẻ <script ... id="ID" ...>...</script>.
fn script_json(html: &str, id: &str) -> Option<Value> {
    let key = format!("id=\"{id}\"");
    let i = html.find(&key)?;
    let start = html[i..].find('>')? + i + 1;
    let end = html[start..].find("</script>")? + start;
    serde_json::from_str(html[start..end].trim()).ok()
}

/// Lấy content của thẻ <meta property="PROP" content="...">.
fn meta_content(html: &str, prop: &str) -> Option<String> {
    let key = format!("property=\"{prop}\"");
    let i = html.find(&key)?;
    let tag_start = html[..i].rfind('<')?;
    let tag_end = html[i..].find('>')? + i;
    let tag = &html[tag_start..tag_end];
    let c = tag.find("content=\"")? + 9;
    let e = tag[c..].find('"')? + c;
    Some(html_unescape(&tag[c..e]))
}

async fn fetch_html(client: &reqwest::Client, url: &str, kind: Kind) -> Result<String, ApiErr> {
    let fail = |m: String| -> ApiErr { (StatusCode::BAD_GATEWAY, m) };
    let resp = client
        .get(url)
        .header("user-agent", BROWSER_UA)
        .header("accept-language", "en-US,en;q=0.9")
        .timeout(Duration::from_secs(25))
        .send()
        .await
        .map_err(|_| fail(format!("Không kết nối được {}", kind.label())))?;
    let code = resp.status();
    if code == StatusCode::NOT_FOUND {
        return Err(bad(&format!("{} báo không tìm thấy playlist này (sai link, hoặc playlist riêng tư)", kind.label())));
    }
    if !code.is_success() {
        return Err(fail(format!("{} trả về lỗi {}", kind.label(), code.as_u16())));
    }
    resp.text().await.map_err(|_| fail(format!("{} trả dữ liệu lạ", kind.label())))
}

fn clean(s: &str) -> String {
    s.replace('\u{a0}', " ").chars().filter(|c| !c.is_control()).collect::<String>().trim().to_string()
}

fn ms_to_secs(n: f64) -> u64 {
    if n <= 0.0 {
        0
    } else if n >= 1000.0 {
        (n / 1000.0) as u64
    } else {
        n as u64
    }
}

// ---------- Spotify ----------

fn parse_spotify(html: &str) -> Result<SrcList, ApiErr> {
    let data = script_json(html, "__NEXT_DATA__").ok_or_else(|| bad("Không đọc được trang Spotify (Spotify có thể đã đổi giao diện)"))?;
    let list = yt_find(&data, "trackList")
        .and_then(|x| x.as_array())
        .ok_or_else(|| bad("Spotify không trả danh sách bài (playlist riêng tư, không tồn tại hoặc Spotify đã đổi giao diện)"))?;
    let name = yt_find(&data, "entity")
        .map(|e| {
            ["name", "title"]
                .iter()
                .filter_map(|k| e.get(*k).and_then(|x| x.as_str()))
                .map(clean)
                .find(|x| !x.is_empty())
                .unwrap_or_default()
        })
        .unwrap_or_default();
    let mut tracks = Vec::new();
    for t in list {
        let title = clean(t.get("title").and_then(|x| x.as_str()).unwrap_or(""));
        if title.is_empty() {
            continue;
        }
        let artist = clean(t.get("subtitle").and_then(|x| x.as_str()).unwrap_or(""));
        let secs = t.get("duration").and_then(|x| x.as_f64()).map(ms_to_secs).unwrap_or(0);
        tracks.push(SrcTrack { title, artist, secs });
    }
    Ok(SrcList { name, tracks })
}

// ---------- Apple Music ----------

// Mỗi bài trong trang Apple Music là một object có title + artistName + duration (mili-giây).
// rank 3 = mục "track-lockup", 2 = mục có contentDescriptor.kind == "song", 1 = chỉ có duration.
fn apple_collect(v: &Value, out: &mut Vec<(u8, SrcTrack)>) {
    match v {
        Value::Object(m) => {
            let title = m.get("title").and_then(|x| x.as_str());
            let artist = m.get("artistName").and_then(|x| x.as_str());
            let dur = m.get("duration").and_then(|x| x.as_f64());
            if let (Some(title), Some(artist), Some(dur)) = (title, artist, dur) {
                let id = m.get("id").and_then(|x| x.as_str()).unwrap_or("");
                let kind = m.get("contentDescriptor").and_then(|c| c.get("kind")).and_then(|x| x.as_str()).unwrap_or("");
                let rank = if id.starts_with("track-lockup") {
                    3
                } else if kind == "song" {
                    2
                } else {
                    1
                };
                out.push((rank, SrcTrack { title: clean(title), artist: clean(artist), secs: ms_to_secs(dur) }));
                return;
            }
            for x in m.values() {
                apple_collect(x, out);
            }
        }
        Value::Array(a) => {
            for x in a {
                apple_collect(x, out);
            }
        }
        _ => {}
    }
}

fn parse_apple(html: &str) -> Result<SrcList, ApiErr> {
    let data = script_json(html, "serialized-server-data").ok_or_else(|| bad("Không đọc được trang Apple Music (Apple có thể đã đổi giao diện)"))?;
    let mut found: Vec<(u8, SrcTrack)> = Vec::new();
    apple_collect(&data, &mut found);
    let best = found.iter().map(|x| x.0).max().unwrap_or(0);
    let mut seen: HashSet<String> = HashSet::new();
    let mut tracks = Vec::new();
    for (rank, t) in found {
        if rank != best || t.title.is_empty() {
            continue;
        }
        if seen.insert(format!("{}|{}", norm_name(&t.title), norm_name(&t.artist))) {
            tracks.push(t);
        }
    }
    // og:title có dạng "Tên playlist by Người tạo on Apple Music"
    let name = meta_content(html, "og:title")
        .map(|t| {
            let t = t.strip_suffix(" on Apple Music").unwrap_or(&t).to_string();
            match t.rsplit_once(" by ") {
                Some((a, _)) if !a.trim().is_empty() => a.trim().to_string(),
                _ => t,
            }
        })
        .unwrap_or_default();
    Ok(SrcList { name, tracks })
}

// ---------- SoundCloud (qua yt-dlp) ----------

async fn ytdlp_list(url: &str, use_proxy: bool) -> Result<(String, Vec<SrcTrack>), ApiErr> {
    let fail = |m: String| -> ApiErr { (StatusCode::BAD_GATEWAY, m) };
    let bin = std::env::var("YTDLP_PATH").ok().filter(|x| !x.trim().is_empty()).unwrap_or_else(|| "yt-dlp".to_string());
    let mut cmd = tokio::process::Command::new(&bin);
    cmd.arg("--flat-playlist").arg("-J").arg("--no-warnings").arg("--yes-playlist");
    if use_proxy {
        if let Ok(p) = std::env::var("YT_PROXY") {
            let p = p.trim().to_string();
            if !p.is_empty() {
                cmd.arg("--proxy").arg(p);
            }
        }
    }
    cmd.arg(url);
    cmd.kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // không bật cửa sổ console
    let out = tokio::time::timeout(Duration::from_secs(90), cmd.output())
        .await
        .map_err(|_| fail("yt-dlp: quá thời gian khi đọc playlist SoundCloud".to_string()))?
        .map_err(|e| fail(format!("Không chạy được yt-dlp ({e}). Cần cài yt-dlp hoặc đặt YTDLP_PATH để nhập từ SoundCloud")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let line: String = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("lỗi").chars().take(200).collect();
        return Err(bad(&format!("Không đọc được playlist SoundCloud (cần là playlist công khai): {line}")));
    }
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|_| fail("yt-dlp: dữ liệu trả về không đọc được".to_string()))?;
    let name = clean(v.get("title").and_then(|x| x.as_str()).unwrap_or(""));
    let entries = v.get("entries").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    let mut tracks = Vec::new();
    for e in entries {
        let title = clean(e.get("title").and_then(|x| x.as_str()).unwrap_or(""));
        if title.is_empty() {
            continue;
        }
        let artist = clean(
            e.get("uploader")
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
                .or_else(|| e.get("channel").and_then(|x| x.as_str()))
                .unwrap_or(""),
        );
        let secs = e.get("duration").and_then(|x| x.as_f64()).unwrap_or(0.0).max(0.0) as u64;
        tracks.push(SrcTrack { title, artist, secs });
    }
    Ok((name, tracks))
}

// ---------- Khớp bài sang YouTube Music ----------

/// Bỏ phần trong ngoặc và đuôi " - Remastered ...", " - Radio Edit", " - Live ..." để so tên bài dễ hơn.
fn core_title(t: &str) -> String {
    let mut out = String::new();
    let mut depth = 0i32;
    for c in t.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    let base = out.split(" - ").next().unwrap_or(&out);
    norm_name(base)
}

/// Câu tìm kiếm. Tiêu đề SoundCloud hay đã có dạng "Nghệ sĩ - Bài" nên không ghép thêm tên kênh.
fn search_query(t: &SrcTrack, kind: Kind) -> String {
    let q = if (kind == Kind::SoundCloud && t.title.contains(" - ")) || t.artist.is_empty() {
        t.title.clone()
    } else {
        format!("{} {}", t.artist, t.title)
    };
    q.chars().take(100).collect()
}

fn score_candidate(src: &SrcTrack, cand: &Value) -> i32 {
    let ctitle = cand.get("title").and_then(|x| x.as_str()).unwrap_or("");
    let cartist = cand.get("artist").and_then(|x| x.as_str()).unwrap_or("");
    let cdur = cand.get("dur").and_then(|x| x.as_f64()).unwrap_or(0.0) as i64;
    let mut score = 0;

    let (st, ct) = (core_title(&src.title), core_title(ctitle));
    if !st.is_empty() && !ct.is_empty() {
        if st == ct {
            score += 3;
        } else if st.contains(&ct) || ct.contains(&st) {
            score += 2;
        }
    }
    // Có ít nhất một nghệ sĩ của nguồn xuất hiện trong chuỗi nghệ sĩ (hoặc tiêu đề) của kết quả
    let haystack = format!("{} {}", norm_name(cartist), norm_name(ctitle));
    if credit_names(&src.artist).iter().any(|a| haystack.contains(a.as_str())) {
        score += 2;
    }
    if src.secs > 0 && cdur > 0 {
        let diff = (src.secs as i64 - cdur).abs();
        score += match diff {
            0..=3 => 3,
            4..=8 => 2,
            9..=20 => 0,
            21..=40 => -2,
            _ => -4,
        };
    }
    score
}

async fn yt_music_search(client: &reqwest::Client, text: &str, filter: &str) -> Vec<Value> {
    let mut body = json!({
        "context": { "client": { "clientName": "WEB_REMIX", "clientVersion": "1.20250310.01.00", "hl": "en", "gl": "VN" } },
        "query": text,
    });
    if let Some(p) = yt_filter_params(filter) {
        body["params"] = json!(p);
    }
    for attempt in 0..2 {
        let sent = client
            .post("https://music.youtube.com/youtubei/v1/search?prettyPrint=false")
            .header("origin", "https://music.youtube.com")
            .header("referer", "https://music.youtube.com/")
            .json(&body)
            .timeout(Duration::from_secs(15))
            .send()
            .await;
        match sent {
            Ok(r) if r.status().is_success() => {
                if let Ok(v) = r.json::<Value>().await {
                    let mut out = Vec::new();
                    yt_collect_all(&v, &mut out, &mut Vec::new());
                    return out;
                }
                return Vec::new();
            }
            _ if attempt == 0 => tokio::time::sleep(Duration::from_millis(400)).await,
            _ => {}
        }
    }
    Vec::new()
}

/// Chọn kết quả khớp nhất trong 6 kết quả đầu; điểm dưới 4 thì coi là không khớp (tránh lưu nhầm bài khác).
fn best_match(src: &SrcTrack, cands: &[Value]) -> Option<Value> {
    let mut best: Option<(i32, usize)> = None;
    for (i, c) in cands.iter().take(6).enumerate() {
        let s = score_candidate(src, c);
        if best.map(|(bs, _)| s > bs).unwrap_or(true) {
            best = Some((s, i));
        }
    }
    match best {
        Some((s, i)) if s >= 4 => Some(cands[i].clone()),
        _ => None,
    }
}

async fn match_track(client: &reqwest::Client, src: &SrcTrack, kind: Kind) -> Option<Value> {
    let q = search_query(src, kind);
    if let Some(m) = best_match(src, &yt_music_search(client, &q, "songs").await) {
        return Some(m);
    }
    // Không thấy bản "Songs" (vd. remix/bootleg trên SoundCloud) thì thử thêm trong video.
    best_match(src, &yt_music_search(client, &q, "videos").await)
}

// ---------- Endpoint ----------

// POST /api/pl-import/url  body {"url": "..."} (hoặc "urls": lấy link đầu tiên)
// Nhận link Spotify / Apple Music / SoundCloud; link YouTube được chuyển cho bộ nhập YouTube sẵn có.
pub async fn pl_import_url(State(st): State<AppState>, headers: HeaderMap, Json(body): Json<Value>) -> R<Value> {
    let u = require(&st, &headers)?;
    let raw = body
        .get("url")
        .and_then(|x| x.as_str())
        .or_else(|| body.get("urls").and_then(|x| x.as_str()))
        .and_then(|s| s.lines().map(|l| l.trim()).find(|l| !l.is_empty()))
        .unwrap_or("")
        .to_string();
    let Some((kind, target)) = detect(&raw) else {
        return Err(bad(
            "Link không được hỗ trợ. Hãy dán link playlist từ Spotify, Apple Music, SoundCloud (dạng soundcloud.com/…/sets/…) hoặc YouTube",
        ));
    };
    if kind == Kind::YouTube {
        return yt_import_url(State(st), headers, Json(body)).await;
    }

    let (_, use_proxy) = user_net_prefs(&st, u.id);
    let client = pick_client(&st, use_proxy);

    let list = match kind {
        Kind::Spotify => parse_spotify(&fetch_html(&client, &target, kind).await?)?,
        Kind::Apple => parse_apple(&fetch_html(&client, &target, kind).await?)?,
        Kind::SoundCloud => {
            let (name, tracks) = ytdlp_list(&target, use_proxy).await?;
            SrcList { name, tracks }
        }
        Kind::YouTube => unreachable!(),
    };
    if list.tracks.is_empty() {
        return Err(bad(&format!("Không đọc được bài nào từ playlist {} này (có thể playlist đang riêng tư hoặc trống)", kind.label())));
    }
    let total = list.tracks.len();
    let truncated = total > IMPORT_MAX_TRACKS;
    let src_tracks: Vec<SrcTrack> = list.tracks.into_iter().take(IMPORT_MAX_TRACKS).collect();

    // Khớp song song nhưng giữ đúng thứ tự gốc của playlist.
    let results: Vec<(SrcTrack, Option<Value>)> = futures_util::stream::iter(src_tracks)
        .map(|t| {
            let client = client.clone();
            async move {
                let m = match_track(&client, &t, kind).await;
                (t, m)
            }
        })
        .buffered(MATCH_CONCURRENCY)
        .collect()
        .await;

    let mut seen: HashSet<String> = HashSet::new();
    let mut matched: Vec<Value> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    for (src, m) in results {
        match m {
            Some(t) => {
                let id = t.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
                if seen.insert(id) {
                    matched.push(t);
                }
            }
            None => {
                let label = if src.artist.is_empty() { src.title } else { format!("{} – {}", src.artist, src.title) };
                skipped.push(label);
            }
        }
    }
    if matched.is_empty() {
        return Err(bad("Không tìm được bài nào của playlist này trên YouTube Music. Thử lại sau ít phút."));
    }

    let mut title = list.name;
    if let Some(t) = body.get("title").and_then(|x| x.as_str()).map(|s| s.trim()).filter(|s| !s.is_empty()) {
        title = t.to_string();
    }
    if title.is_empty() {
        title = format!("Playlist {}", kind.label());
    }
    let title: String = title.chars().take(100).collect();

    let (np, nt) = {
        let db = st.db.lock().unwrap();
        insert_playlists(&db, u.id, &[json!({ "name": title, "tracks": matched })])?
    };
    let skipped_count = skipped.len();
    skipped.truncate(15);
    Ok(Json(json!({
        "playlists": np,
        "tracks": nt,
        "name": title,
        "source": kind.label(),
        "total": total,
        "skipped": skipped_count,
        "skipped_list": skipped,
        "truncated": truncated,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_links() {
        let (k, u) = detect("https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M?si=abc").unwrap();
        assert_eq!(k, Kind::Spotify);
        assert_eq!(u, "https://open.spotify.com/embed/playlist/37i9dQZF1DXcBWIGoYBM5M");
        let (k, u) = detect("https://open.spotify.com/intl-vi/album/4aawyAB9vmqN3uQ7FjRGTy").unwrap();
        assert_eq!(k, Kind::Spotify);
        assert_eq!(u, "https://open.spotify.com/embed/album/4aawyAB9vmqN3uQ7FjRGTy");
        assert_eq!(detect("spotify:playlist:37i9dQZF1DXcBWIGoYBM5M").unwrap().0, Kind::Spotify);
        assert_eq!(detect("https://music.apple.com/vn/playlist/chill/pl.abc123").unwrap().0, Kind::Apple);
        assert_eq!(detect("music.apple.com/us/album/x/123456").unwrap().0, Kind::Apple);
        assert_eq!(detect("https://soundcloud.com/user/sets/my-set?si=1").unwrap().0, Kind::SoundCloud);
        assert!(detect("https://soundcloud.com/user/some-track").is_none());
        assert_eq!(detect("https://www.youtube.com/playlist?list=PLabc").unwrap().0, Kind::YouTube);
        assert!(detect("https://example.com/playlist/1").is_none());
        assert!(detect("https://open.spotify.com/track/abc").is_none());
    }

    #[test]
    fn spotify_embed() {
        let html = r#"<html><script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{"state":{"data":{"entity":{"name":"Chill Mix","trackList":[{"title":"Song A","subtitle":"Artist 1, Artist 2","duration":215000},{"title":"Song B","subtitle":"Artist 3","duration":180000}]}}}}}}</script></html>"#;
        let l = parse_spotify(html).unwrap();
        assert_eq!(l.name, "Chill Mix");
        assert_eq!(l.tracks.len(), 2);
        assert_eq!(l.tracks[0].artist, "Artist 1, Artist 2");
        assert_eq!(l.tracks[0].secs, 215);
    }

    #[test]
    fn apple_page() {
        let html = r#"<meta property="og:title" content="Chill &amp; Co by Apple Music on Apple Music"><script type="application/json" id="serialized-server-data">[{"data":{"sections":[{"items":[{"id":"track-lockup - pl.x - 1","title":"Song A","artistName":"Artist 1","duration":200000},{"id":"track-lockup - pl.x - 2","title":"Song B","artistName":"Artist 2","duration":190000}]},{"items":[{"id":"album-x","title":"Other album","artistName":"Someone","duration":1}]}]}}]</script>"#;
        let l = parse_apple(html).unwrap();
        assert_eq!(l.name, "Chill & Co");
        assert_eq!(l.tracks.len(), 2);
        assert_eq!(l.tracks[1].title, "Song B");
        assert_eq!(l.tracks[0].secs, 200);
    }

    #[test]
    fn matching() {
        let src = SrcTrack { title: "Blinding Lights - Remastered".into(), artist: "The Weeknd".into(), secs: 200 };
        let good = json!({ "title": "Blinding Lights", "artist": "The Weeknd", "dur": 200 });
        let wrong = json!({ "title": "Blinding Lights (Cover)", "artist": "Random Kid", "dur": 260 });
        assert!(best_match(&src, &[wrong.clone(), good.clone()]).is_some());
        assert!(best_match(&src, &[wrong]).is_none());
    }
}
