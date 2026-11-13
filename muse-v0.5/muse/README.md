# Muse – nghe nhạc cùng bạn bè (Rust)

```
muse/
├─ server/   Axum + SQLite: đăng nhập, avatar GIF, bạn bè, playlist, yêu thích, lịch sử, phòng nghe chung + giao diện web
└─ desktop/  Tauri v2: app desktop kết nối tới server
```

## Chạy server
```bash
cd server
export DONATE_URL=https://ko-fi.com/ban    # tuỳ chọn
cargo run --release                        # http://localhost:3000
```
Dữ liệu: `data/muse.db` (SQLite, chế độ WAL) + `data/avatars/`. Khi backup hãy chép **cả thư mục `data/`** (gồm `muse.db-wal` và `muse.db-shm`), nếu không sẽ mất dữ liệu mới nhất.
Đưa lên mạng: VPS + Caddy/Nginx có HTTPS (cần cho wss:// và YouTube player).

## Đóng gói desktop
```bash
cargo install tauri-cli --version "^2"
cd desktop/src-tauri
cargo tauri icon /duong/dan/logo.png
MUSE_URL=https://muse.example.com cargo tauri build
```

## Ra file .exe (Windows)
Xem hướng dẫn trong cuộc trò chuyện, hoặc dùng GitHub Actions: đẩy thư mục này lên GitHub,
đặt biến `MUSE_URL` (Settings → Secrets and variables → Actions → Variables), rồi chạy workflow
"Build desktop (Muse)" và tải file .exe ở mục Artifacts.

## Nguồn nhạc & tính năng (v0.3)
- Nguồn nhạc duy nhất: YouTube / YouTube Music (tìm qua InnerTube phía server, phát bằng player chính thức của YouTube). SoundCloud, Jamendo và Audius đã được gỡ; bài cũ của hai nguồn sau trong yêu thích/lịch sử/playlist vẫn phát được.
- Trang Khám phá và Thịnh hành lấy từ các truy vấn tìm kiếm YouTube Music (không phải bảng xếp hạng chính thức).
- **SponsorBlock**: luôn bật, tự bỏ qua đoạn quảng cáo/intro/outro trong video YouTube (không còn mục cài đặt riêng).
- **Return YouTube Dislike**: hiện số 👍/👎 của video YouTube cạnh tên nghệ sĩ.
- **Lời bài hát**: SimpMusic Lyrics (chính) → LRCLIB (dự phòng); ưu tiên bản có thời gian (karaoke).
- Chưa có: nhận diện bài hát bằng Shazam.


## Giao diện v0.4
- **Banner hồ sơ kiểu Discord**: tải ảnh PNG/JPG/WEBP hoặc **GIF động** (≤ 10MB) ở trang *Hồ sơ & giao diện*; chưa có ảnh thì dùng màu banner. Kèm dòng **trạng thái** hiện trong bong bóng cạnh avatar. Bấm avatar của mình (góc trên) hoặc của bạn bè để xem thẻ hồ sơ. Ảnh lưu ở `data/banners/` (nhớ backup cùng thư mục `data/`).
- **Nền hồ sơ** (trước đây là "Nền/skin"): chọn nền hoặc tải ảnh/GIF riêng, có khung **xem trước** ngay bên phải (chưa lưu thì chưa áp dụng) và nút *Xem thử toàn app*.
- **Bấm vào thanh nhạc** để mở panel *Đang phát* bên phải, với 3 tab: Đang phát · Lời (karaoke, bấm vào dòng để tua) · Tiếp theo (hàng đợi). Bấm lại để đóng.
- Trang chủ có mục **Chọn nhanh** (lưới bài gọn) lấy cảm hứng từ SimpMusic.
- DB cũ tự được nâng cấp (thêm cột `banner`, `banner_color`, `status`). API mới: `POST/DELETE /api/banner`.
- **Modal chỉnh ảnh** (kiểu Discord) hiện ngay khi chọn ảnh/GIF cho avatar hoặc banner: kéo để di chuyển, cuộn chuột hoặc kéo thanh để zoom, giữ thanh X/Y/Zoom; zoom nhỏ hơn mức phủ kín để thấy trọn ảnh (phần trống lấy màu banner). Ảnh gốc giữ nguyên (GIF vẫn động), thông số lưu trong cột `crop`.
- Thẻ hồ sơ: banner nằm phía sau avatar và tên.
- Thanh điều hướng trang chủ: nút **Tâm trạng** mở danh sách dài 17 tâm trạng (Thư giãn, Buồn, Lãng mạn, Tiệc tùng, Ngủ ngon…); chọn một mục để xem các mục nhạc theo tâm trạng đó.


## Cài đặt & Hồ sơ (API mới)
- `POST /api/account/name`, `POST /api/account/password` – đổi tên hiển thị / mật khẩu (đổi mật khẩu sẽ đăng xuất các thiết bị khác).
- `GET|POST /api/settings` – cài đặt theo tài khoản (`track_history`, `save_state`, `crossfade`, `crossfade_secs`).
- `DELETE /api/history` – xóa lịch sử nghe.
- `GET /api/backup`, `POST /api/restore`, `POST /api/import` – sao lưu, khôi phục, nhập danh sách phát (JSON).


## v0.5 – Cài đặt mới
Biến môi trường mới cho server (tuỳ chọn): `YT_PROXY` (vd. `http://host:port` hoặc `socks5://host:port`), `DISCORD_CLIENT_ID` (Application ID tạo ở discord.com/developers).

- **Bộ chỉnh âm 10 dải / Độ trễ (echo) / Độ vang (phòng, hội trường, tấm kim loại)**: xử lý bằng Web Audio. Vì player iframe của YouTube không cho can thiệp âm thanh, khi bật bất kỳ hiệu ứng nào nhạc YouTube sẽ phát qua `GET /api/yt/audio/:id` (server lấy luồng âm thanh qua InnerTube rồi chuyển tiếp từng đoạn Range). Nếu không lấy được luồng, app tự quay về player YouTube. Khi dùng luồng này sẽ không có chế độ xem video.
- **Phát bài hát chứa nội dung nhạy cảm**: mặc định tắt; khi tắt, bài có nhãn Explicit bị ẩn khỏi kết quả tìm kiếm và bị bỏ qua trong hàng đợi.
- **Giữ danh sách phát hiển thị khi offline**: lưu danh sách phát vào bộ nhớ trình duyệt; mất mạng vẫn mở được app và xem danh sách (kèm `sw.js` để mở giao diện offline).
- **Proxy**: bật thì tìm kiếm và luồng âm thanh của bạn đi qua `YT_PROXY` trên server (nhạc được phát qua luồng của server thay vì iframe).
- **Discord Rich Presence** (chỉ app desktop): cần `DISCORD_CLIENT_ID` trên server và Discord đang mở trên máy. Bấm *Kết nối Discord* rồi bật *Kích hoạt Rich Presence*. App desktop tự tạo capability cho `MUSE_URL` lúc build (`desktop/src-tauri/build.rs`).
- API: `/api/settings` có thêm `explicit, offline_pl, proxy, discord_rpc, eq_on, eq_gains[10], eq_pre, delay_on/ms/fb/mix, reverb_on/type/mix`.


## Android (v0.7.0: WebView + dịch vụ phát nhạc nền native)
```
android/   app Android = giao diện web của server đặt trong WebView + dịch vụ nền (KeepAliveService)
```
- Phát nhạc nền: khi bắt đầu phát, `MuseBridge` (JS `MuseNative.update`) bật **dịch vụ foreground** có `MediaSession`: thông báo Trước / Phát-Tạm dừng / Sau, ảnh bìa, điều khiển trên màn hình khoá và tai nghe, tự tạm dừng khi rút tai nghe hoặc có cuộc gọi, giữ WakeLock + WifiLock khi đang phát. Lệnh từ thông báo gửi ngược vào web qua `window.museCmd(...)`.
- Trên Android, web tự dùng luồng âm thanh `<audio>` (`/api/yt/audio/:id`) thay cho iframe YouTube, vì iframe bị Android dừng khi tắt màn hình.
- Giảm lag: bỏ `backdrop-filter`/blur trên Android, bỏ transition thuộc tính tuỳ biến trên `:root`, không vẽ lại thanh tiến trình khi app bị ẩn, WebView bật pre-raster, tự tải lại khi tiến trình WebView bị hệ thống thu hồi.
- Lần đầu mở app sẽ hỏi cho phép bỏ tối ưu pin (giúp nhạc không bị cắt khi tắt màn hình lâu).
- Nếu bấm phát mà nhạc dừng ngay: đặt `focusOff = true` ở đầu `KeepAliveService.kt` (tắt xử lý audio focus).

**Lấy APK:** đẩy code lên GitHub → Actions → "Build Android (Muse)" → tải `muse-android` ở Artifacts. Nhớ build/chạy lại **server** vì `static/index.html` cũng đã đổi.
**Build trên máy:** mở `android/` bằng Android Studio hoặc `gradle assembleDebug` (JDK 17 + Android SDK 34).

## Đăng nhập Discord (bắt buộc để bật Rich Presence)
Người dùng phải đăng nhập Discord (OAuth2, scope `identify`) ở *Cài đặt → Tích hợp Discord → Đăng nhập Discord* thì mới bật được Rich Presence. Server từ chối `discord_rpc=true` nếu tài khoản chưa liên kết; hủy liên kết sẽ tắt luôn Rich Presence.

Cấu hình (3 biến môi trường trên server):
- `DISCORD_CLIENT_ID` – Application ID
- `DISCORD_CLIENT_SECRET` – OAuth2 → Client Secret (giữ bí mật, không đưa vào app)
- `PUBLIC_URL` – địa chỉ công khai của server, vd. `https://muse.example.com` (giống `MUSE_URL` của app)

Ở discord.com/developers → ứng dụng của bạn → **OAuth2 → Redirects** thêm đúng: `https://muse.example.com/api/discord/callback`.

API: `GET /api/discord/status`, `POST /api/discord/link` (trả URL đăng nhập), `DELETE /api/discord/link`, `GET /api/discord/callback`. DB tự thêm cột `discord_id`, `discord_name`, `discord_avatar` và bảng `discord_states`.

Lưu ý: Rich Presence thật sự (hiện "Đang nghe…" trên hồ sơ) vẫn chỉ chạy ở app desktop (qua Discord IPC). Trên Android (app native) chưa có mục này; muốn hiện trạng thái từ điện thoại cần Discord Social SDK cho Android (code Kotlin/Java riêng).


## Khi YouTube chặn luồng âm thanh (403 / "Sign in to confirm you're not a bot")
Server thử lần lượt: ANDROID_VR → IOS → TV nhúng → **yt-dlp** → Piped; nguồn nào bị chặn được nhớ 10 phút. Mỗi đoạn tải thử 3 cách (Range header, `?range=`, đoạn 256KB).
Cách chắc nhất: cài `yt-dlp` (đặt `yt-dlp.exe` cạnh `muse-server.exe`, hoặc đặt `YTDLP_PATH`) và đặt `YT_COOKIES` = file cookies.txt xuất từ trình duyệt đã đăng nhập YouTube (có cookies thì yt-dlp được thử đầu tiên). Nên dùng tài khoản phụ, vì tài khoản dùng cookies cho bot vẫn có rủi ro bị YouTube khóa. Cập nhật yt-dlp thường xuyên (`yt-dlp -U`).

Nếu link yt-dlp bị googlevideo trả 403 (link gắn với client/PO token của yt-dlp), server tự cho yt-dlp tải file âm thanh về thư mục tạm (`%TEMP%\muse-audio`, tự dọn sau 6 giờ) rồi phát từ file đó.


## Nhập playlist từ YouTube (bằng link, không cần Google)
Vào *Cài đặt → Nhập playlist từ YouTube*, dán link playlist (mỗi link một dòng, tối đa 10 link) rồi bấm *Nhập playlist*. Không cần đăng nhập hay cấu hình gì thêm.
- Playlist phải ở chế độ **Công khai** hoặc **Không công khai**. Playlist riêng tư và "Video đã thích" cần đăng nhập nên không nhập được. Video riêng tư/đã xóa bị bỏ qua.
- Link hợp lệ có dạng `https://www.youtube.com/playlist?list=PL...` hoặc `https://www.youtube.com/watch?v=...&list=PL...` (cũng nhận thẳng ID playlist). Mix tự tạo của YouTube (`list=RD...`) không nhập được.
- API: `POST /api/yt-import/url` với body `{"urls":"link1\nlink2"}`.

## Nhập playlist từ Spotify, Apple Music, SoundCloud
Cùng ô *Cài đặt → Nhập playlist từ app khác*: dán link playlist công khai (mỗi lần một link). Muse chỉ phát nhạc từ YouTube, nên server đọc danh sách (tên bài, nghệ sĩ, thời lượng) từ app gốc rồi **tìm từng bài trên YouTube Music** và lưu thành playlist của bạn, giữ đúng thứ tự gốc. Bài không tìm được bản khớp (tên + nghệ sĩ + thời lượng) sẽ bị bỏ qua; giao diện báo số bài bỏ qua, rê chuột vào dòng thông báo để xem danh sách.
- **Spotify**: `open.spotify.com/playlist/…` hoặc `/album/…` (cũng nhận `spotify:playlist:ID`). Đọc từ trang nhúng công khai nên chỉ lấy được tối đa khoảng 100 bài đầu mỗi playlist. Playlist riêng tư, "Liked Songs" và playlist do Spotify cá nhân hóa (Daily Mix…) không nhập được.
- **Apple Music**: `music.apple.com/…/playlist/…` hoặc `/album/…`. Đọc dữ liệu có sẵn trong trang công khai.
- **SoundCloud**: link bộ sưu tập dạng `soundcloud.com/<user>/sets/<tên>` (và link rút gọn `on.soundcloud.com/…`). Dùng **yt-dlp** (như phần phát nhạc: đặt `yt-dlp` cạnh server hoặc `YTDLP_PATH`). Remix/bootleg không có trên YouTube Music có thể bị bỏ qua.
- Mỗi lần nhập tối đa 300 bài. Link YouTube trên cùng ô vẫn đi qua bộ nhập YouTube như cũ.
- API: `POST /api/pl-import/url` với body `{"url":"https://open.spotify.com/playlist/…"}` → `{name, tracks, total, skipped, skipped_list, truncated, source}`. Code ở `server/src/pl_import.rs` (có test cho phần đọc link, đọc trang và chấm điểm khớp bài: `cargo test`).
- Các trang Spotify/Apple đổi cấu trúc là chuyện thường; nếu một app bỗng báo "không đọc được trang", hàm `parse_spotify` / `parse_apple` là chỗ cần chỉnh.

