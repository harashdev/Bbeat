use tauri::{WebviewUrl, WebviewWindowBuilder};
#[cfg(desktop)]
use serde_json::{json, Value};
#[cfg(desktop)]
use std::{
    io::{Read, Write},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
#[cfg(desktop)]
use tauri::State;

// Địa chỉ server Muse, cố định lúc build: MUSE_URL=https://muse.example.com cargo tauri build
const SERVER_URL: &str = match option_env!("MUSE_URL") {
    Some(u) => u,
    None => "http://localhost:3000",
};

#[cfg(desktop)]
mod discord_rpc {
    use super::*;
    // ===== Discord Rich Presence qua IPC cục bộ (không cần thư viện ngoài) =====
    // Giao thức: mỗi khung = opcode (u32 LE) + độ dài (u32 LE) + JSON. Windows: named pipe, Linux/macOS: unix socket.
    trait Conn: Read + Write + Send {}
    impl<T: Read + Write + Send> Conn for T {}

    #[derive(Default)]
    pub struct Discord {
        conn: Option<Box<dyn Conn>>,
        app_id: String,
        user: String,
    }
    pub struct DiscordState(pub Mutex<Discord>);

    fn open_ipc() -> Option<Box<dyn Conn>> {
        for i in 0..10 {
            #[cfg(windows)]
            {
                let p = format!(r"\\.\pipe\discord-ipc-{i}");
                if let Ok(f) = std::fs::OpenOptions::new().read(true).write(true).open(&p) {
                    return Some(Box::new(f));
                }
            }
            #[cfg(unix)]
            {
                let mut dirs: Vec<String> = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
                    .iter()
                    .filter_map(|k| std::env::var(k).ok())
                    .collect();
                dirs.push("/tmp".into());
                for d in dirs {
                    for sub in ["", "snap.discord/", "app/com.discordapp.Discord/"] {
                        let path = format!("{}/{}discord-ipc-{}", d.trim_end_matches('/'), sub, i);
                        if let Ok(s) = std::os::unix::net::UnixStream::connect(&path) {
                            return Some(Box::new(s));
                        }
                    }
                }
            }
        }
        None
    }

    fn send_frame(c: &mut dyn Conn, op: u32, v: &Value) -> std::io::Result<()> {
        let data = v.to_string().into_bytes();
        let mut buf = Vec::with_capacity(8 + data.len());
        buf.extend_from_slice(&op.to_le_bytes());
        buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&data);
        c.write_all(&buf)?;
        c.flush()
    }

    fn read_frame(c: &mut dyn Conn) -> std::io::Result<(u32, Value)> {
        let mut h = [0u8; 8];
        c.read_exact(&mut h)?;
        let op = u32::from_le_bytes([h[0], h[1], h[2], h[3]]);
        let len = u32::from_le_bytes([h[4], h[5], h[6], h[7]]) as usize;
        if len > 1 << 20 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "khung quá lớn"));
        }
        let mut body = vec![0u8; len];
        c.read_exact(&mut body)?;
        Ok((op, serde_json::from_slice(&body).unwrap_or(Value::Null)))
    }

    fn handshake(d: &mut Discord) -> Result<String, String> {
        let mut c = open_ipc().ok_or("Không tìm thấy Discord đang chạy")?;
        send_frame(&mut *c, 0, &json!({ "v": 1, "client_id": d.app_id })).map_err(|e| e.to_string())?;
        let (_, v) = read_frame(&mut *c).map_err(|e| e.to_string())?;
        if v.get("evt").and_then(|x| x.as_str()) != Some("READY") {
            return Err(v
                .pointer("/data/message")
                .and_then(|x| x.as_str())
                .unwrap_or("Discord từ chối kết nối")
                .to_string());
        }
        let user = v
            .pointer("/data/user/global_name")
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .or_else(|| v.pointer("/data/user/username").and_then(|x| x.as_str()))
            .unwrap_or("Discord")
            .to_string();
        d.conn = Some(c);
        d.user = user.clone();
        Ok(user)
    }

    fn now_ms() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
    }

    fn set_activity(d: &mut Discord, activity: Value) -> Result<(), String> {
        let nonce = now_ms().to_string();
        let payload = json!({
            "cmd": "SET_ACTIVITY",
            "args": { "pid": std::process::id(), "activity": activity },
            "nonce": nonce
        });
        let c = d.conn.as_mut().ok_or("Chưa kết nối Discord")?;
        let res = send_frame(&mut **c, 1, &payload).and_then(|_| read_frame(&mut **c).map(|_| ()));
        if let Err(e) = res {
            d.conn = None;
            d.user.clear();
            return Err(e.to_string());
        }
        Ok(())
    }

    #[tauri::command]
    pub fn discord_connect(state: State<'_, DiscordState>, app_id: String) -> Result<String, String> {
        let mut d = state.0.lock().map_err(|e| e.to_string())?;
        if d.conn.is_some() && d.app_id == app_id {
            return Ok(d.user.clone());
        }
        d.conn = None;
        d.app_id = app_id;
        handshake(&mut d)
    }

    #[tauri::command]
    pub fn discord_activity(
        state: State<'_, DiscordState>,
        title: String,
        artist: String,
        image: String,
        start_ms: u64,
        end_ms: u64,
    ) -> Result<(), String> {
        let mut d = state.0.lock().map_err(|e| e.to_string())?;
        let mut a = json!({
            "type": 2,
            "details": title.chars().take(120).collect::<String>(),
            "state": artist.chars().take(120).collect::<String>(),
        });
        if !artist.is_empty() {
            a["state"] = json!(artist.chars().take(120).collect::<String>());
        }
        let mut ts = json!({});
        if start_ms > 0 {
            ts["start"] = json!(start_ms);
        }
        if end_ms > 0 {
            ts["end"] = json!(end_ms);
        }
        a["timestamps"] = ts;
        if image.starts_with("http") {
            a["assets"] = json!({ "large_image": image, "large_text": title });
        }
        set_activity(&mut d, a)
    }

    #[tauri::command]
    pub fn discord_clear(state: State<'_, DiscordState>) -> Result<(), String> {
        let mut d = state.0.lock().map_err(|e| e.to_string())?;
        if d.conn.is_none() {
            return Ok(());
        }
        set_activity(&mut d, Value::Null)
    }
}

// ===== Discord Social SDK (tùy chọn, chỉ khi build có DISCORD_SDK_DIR -> cfg muse_sdk) =====
#[cfg(all(desktop, muse_sdk))]
mod sdk {
    use std::ffi::{c_char, c_int, CStr, CString};

    extern "C" {
        fn muse_pres_start(app_id: u64, token: *const c_char) -> c_int;
        fn muse_pres_update(title: *const c_char, artist: *const c_char, image: *const c_char, start_s: i64, end_s: i64);
        fn muse_pres_clear();
        fn muse_pres_status() -> c_int;
        fn muse_pres_error() -> *const c_char;
    }
    extern "system" {
        fn LoadLibraryA(name: *const c_char) -> *mut core::ffi::c_void;
    }

    fn c(s: &str) -> CString {
        CString::new(s.replace('\0', "")).unwrap_or_default()
    }

    #[tauri::command]
    pub fn sdk_available() -> bool {
        unsafe { !LoadLibraryA(c"discord_partner_sdk.dll".as_ptr()).is_null() }
    }

    #[tauri::command]
    pub fn sdk_presence_start(app_id: String, token: String) -> Result<(), String> {
        let id: u64 = app_id.parse().map_err(|_| "app_id không hợp lệ".to_string())?;
        let r = unsafe { muse_pres_start(id, c(&token).as_ptr()) };
        if r == 0 { Ok(()) } else { Err("Không khởi động được Discord SDK".into()) }
    }

    #[tauri::command]
    pub fn sdk_presence_status() -> Result<String, String> {
        unsafe {
            match muse_pres_status() {
                2 => Ok("ready".into()),
                3 => {
                    let p = muse_pres_error();
                    let e = if p.is_null() { String::new() } else { CStr::from_ptr(p).to_string_lossy().into_owned() };
                    Err(if e.is_empty() { "lỗi Discord".into() } else { e })
                }
                1 => Ok("connecting".into()),
                _ => Ok("idle".into()),
            }
        }
    }

    #[tauri::command]
    pub fn sdk_presence_update(title: String, artist: String, image: String, start_ms: i64, end_ms: i64) -> Result<(), String> {
        unsafe { muse_pres_update(c(&title).as_ptr(), c(&artist).as_ptr(), c(&image).as_ptr(), start_ms / 1000, end_ms / 1000) };
        Ok(())
    }

    #[tauri::command]
    pub fn sdk_presence_clear() -> Result<(), String> {
        unsafe { muse_pres_clear() };
        Ok(())
    }
}

// Không có SDK: các lệnh vẫn tồn tại để trang web không bị lỗi "command not found".
#[cfg(all(desktop, not(muse_sdk)))]
mod sdk {
    #[tauri::command]
    pub fn sdk_available() -> bool {
        false
    }
    #[tauri::command]
    pub fn sdk_presence_start(_app_id: String, _token: String) -> Result<(), String> {
        Err("Bản app này không có Discord SDK".into())
    }
    #[tauri::command]
    pub fn sdk_presence_status() -> Result<String, String> {
        Ok("idle".into())
    }
    #[tauri::command]
    pub fn sdk_presence_update(_title: String, _artist: String, _image: String, _start_ms: i64, _end_ms: i64) -> Result<(), String> {
        Ok(())
    }
    #[tauri::command]
    pub fn sdk_presence_clear() -> Result<(), String> {
        Ok(())
    }
}

// Mở liên kết ngoài bằng trình duyệt mặc định (chỉ http/https).
#[cfg(desktop)]
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    let url = url.trim().to_string();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("URL không hợp lệ".into());
    }
    #[cfg(windows)]
    let r = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", &url])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .or_else(|_| std::process::Command::new("explorer.exe").arg(&url).spawn())
    };
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(&url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(&url).spawn();
    r.map(|_| ()).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    #[cfg(desktop)]
    let builder = builder
        .manage(discord_rpc::DiscordState(Mutex::new(Default::default())))
        .invoke_handler(tauri::generate_handler![
            open_url,
            discord_rpc::discord_connect,
            discord_rpc::discord_activity,
            discord_rpc::discord_clear,
            sdk::sdk_available,
            sdk::sdk_presence_start,
            sdk::sdk_presence_status,
            sdk::sdk_presence_update,
            sdk::sdk_presence_clear,
        ]);

    builder
        .setup(|app| {
            let url: tauri::Url = SERVER_URL.parse().map_err(|e| format!("MUSE_URL không hợp lệ: {e}"))?;
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
                .title("BBeat")
                .inner_size(1280.0, 820.0)
                .min_inner_size(420.0, 600.0)
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("lỗi khi chạy Muse");
}
