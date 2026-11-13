use std::{env, fs, path::Path};

// Các lệnh Rust mà trang Muse (tải từ MUSE_URL, tức nguồn "từ xa") được phép gọi.
// Tauri 2 chặn nguồn từ xa gọi lệnh tự viết nếu chưa cấp quyền rõ ràng, nên phải khai báo ở đây
// (AppManifest) và liệt kê quyền `allow-<tên-lệnh>` trong capability bên dưới.
// Thêm lệnh mới vào lib.rs thì nhớ thêm tên vào danh sách này.
const COMMANDS: &[&str] = &[
    "open_url",
    "discord_connect",
    "discord_activity",
    "discord_clear",
    "sdk_available",
    "sdk_presence_start",
    "sdk_presence_status",
    "sdk_presence_update",
    "sdk_presence_clear",
];

// Tạo capability cho phép trang Muse (tải từ MUSE_URL) gọi các lệnh của app (mở link ngoài, Discord Rich Presence).
fn main() {
    let url = env::var("MUSE_URL").unwrap_or_default();
    let url = if url.trim().is_empty() {
        "http://localhost:3000".to_string()
    } else {
        url.trim().trim_end_matches('/').to_string()
    };
    let perms = COMMANDS
        .iter()
        .map(|c| format!("\"allow-{}\"", c.replace('_', "-")))
        .collect::<Vec<_>>()
        .join(",");
    let cap = format!(
        r#"{{"identifier":"remote-muse","description":"Cho phép trang Muse từ server gọi lệnh của app (mở link, Discord)","windows":["main"],"remote":{{"urls":["{u}","{u}/*"]}},"permissions":["core:default",{p}]}}"#,
        u = url,
        p = perms
    );
    let dir = Path::new("capabilities");
    fs::create_dir_all(dir).expect("không tạo được thư mục capabilities");
    fs::write(dir.join("remote-muse.json"), cap).expect("không ghi được capability");
    println!("cargo:rerun-if-env-changed=MUSE_URL");
    build_discord_sdk();
    let attrs = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    tauri_build::try_build(attrs).expect("tauri-build lỗi");
}

// Discord Social SDK (tùy chọn): đặt DISCORD_SDK_DIR trỏ tới thư mục "discord_social_sdk" đã giải nén.
// Có SDK -> biên dịch lớp C++ ../../native/muse_presence.cpp và bật cfg `muse_sdk`; không có thì app vẫn build, chỉ thiếu tính năng.
fn build_discord_sdk() {
    println!("cargo:rustc-check-cfg=cfg(muse_sdk)");
    println!("cargo:rerun-if-env-changed=DISCORD_SDK_DIR");
    let Ok(dir) = env::var("DISCORD_SDK_DIR") else {
        return;
    };
    let sdk = Path::new(&dir);
    let inc = sdk.join("include");
    if !inc.join("discordpp.h").exists()
        || env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() != "windows"
    {
        return;
    }
    let lib = sdk.join("lib").join("release");
    let native = Path::new("../../native");
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .include(&inc)
        .include(native)
        .file(native.join("muse_presence.cpp"))
        .compile("muse_presence");
    println!("cargo:rerun-if-changed=../../native/muse_presence.cpp");
    println!("cargo:rustc-link-search=native={}", lib.display());
    println!("cargo:rustc-link-lib=dylib=discord_partner_sdk");
    // Nạp DLL muộn: app vẫn khởi động được nếu thiếu DLL; lib.rs tự LoadLibrary trước khi dùng.
    println!("cargo:rustc-link-lib=dylib=delayimp");
    println!("cargo:rustc-link-arg=/DELAYLOAD:discord_partner_sdk.dll");
    println!("cargo:rustc-cfg=muse_sdk");
}
