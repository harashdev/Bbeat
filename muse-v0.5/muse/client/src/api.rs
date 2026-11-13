//! Gọi API của server Muse (Axum). Mọi hàm ở đây là blocking: UI chạy chúng trong thread riêng.

use reqwest::blocking::{Client, RequestBuilder, Response};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

fn id_to_string<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let v = Value::deserialize(d)?;
    Ok(match v {
        Value::String(s) => s,
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    })
}

/// Bài hát. Khớp với JSON mà server lưu/trả: { src, id, title, artist, image, url, dur, explicit }.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Track {
    pub src: String,
    #[serde(deserialize_with = "id_to_string")]
    pub id: String,
    pub title: String,
    pub artist: String,
    pub image: String,
    pub url: String,
    pub dur: f64,
    pub explicit: bool,
}

impl Track {
    /// Khoá bài hát giống `track_key` ở server ("src:id").
    pub fn key(&self) -> String {
        format!("{}:{}", self.src, self.id)
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub avatar: String,
    pub accent: String,
    pub code: String,
    pub status: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub count: i64,
}

#[derive(Clone)]
pub struct Api {
    pub base: String,
    token: String,
    http: Client,
}

impl Api {
    pub fn new(base: &str, token: &str) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(40))
            .user_agent("MuseNative/0.6")
            .build()
            .unwrap_or_else(|_| Client::new());
        Api {
            base: base.trim().trim_end_matches('/').to_string(),
            token: token.to_string(),
            http,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    fn auth(&self, rb: RequestBuilder) -> RequestBuilder {
        if self.token.is_empty() {
            rb
        } else {
            rb.bearer_auth(&self.token)
        }
    }

    fn send(&self, rb: RequestBuilder) -> Result<Response, String> {
        let r = self
            .auth(rb)
            .send()
            .map_err(|e| format!("Không kết nối được máy chủ: {e}"))?;
        if r.status().is_success() {
            Ok(r)
        } else {
            let code = r.status();
            let body = r.text().unwrap_or_default();
            Err(if body.trim().is_empty() {
                format!("Lỗi {code}")
            } else {
                body
            })
        }
    }

    fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        self.send(self.http.get(self.url(path)))?
            .json::<T>()
            .map_err(|e| format!("Dữ liệu lạ từ máy chủ: {e}"))
    }

    fn post_json<T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T, String> {
        self.send(self.http.post(self.url(path)).json(body))?
            .json::<T>()
            .map_err(|e| format!("Dữ liệu lạ từ máy chủ: {e}"))
    }

    fn post_ok(&self, path: &str, body: &Value) -> Result<(), String> {
        self.send(self.http.post(self.url(path)).json(body))
            .map(|_| ())
    }

    // ---------- Tài khoản ----------
    pub fn login(&self, username: &str, password: &str) -> Result<(String, User), String> {
        let v: Value = self.post_json(
            "/api/login",
            &json!({ "username": username, "password": password }),
        )?;
        parse_auth(v)
    }

    pub fn register(&self, username: &str, password: &str) -> Result<(String, User), String> {
        let v: Value = self.post_json(
            "/api/register",
            &json!({ "username": username, "password": password, "display_name": username }),
        )?;
        parse_auth(v)
    }

    pub fn me(&self) -> Result<User, String> {
        self.get_json("/api/me")
    }

    pub fn logout(&self) -> Result<(), String> {
        self.post_ok("/api/logout", &json!({}))
    }

    // ---------- Nhạc ----------
    pub fn search(&self, q: &str) -> Result<Vec<Track>, String> {
        let r = self.send(self.http.get(self.url("/api/yt/search")).query(&[("q", q)]))?;
        r.json::<Vec<Track>>()
            .map_err(|e| format!("Dữ liệu lạ từ máy chủ: {e}"))
    }

    pub fn recommend(&self) -> Result<Vec<Track>, String> {
        self.get_json("/api/recommend")
    }

    pub fn favorites(&self) -> Result<Vec<Track>, String> {
        self.get_json("/api/favorites")
    }

    /// Bật/tắt yêu thích, trả về trạng thái mới.
    pub fn toggle_fav(&self, t: &Track) -> Result<bool, String> {
        let v: Value = self.post_json("/api/favorites", &track_json(t))?;
        Ok(v.get("fav").and_then(|x| x.as_bool()).unwrap_or(false))
    }

    pub fn history(&self) -> Result<Vec<Track>, String> {
        self.get_json("/api/history")
    }

    pub fn add_history(&self, t: &Track) -> Result<(), String> {
        self.post_ok("/api/history", &track_json(t))
    }

    pub fn playlists(&self) -> Result<Vec<Playlist>, String> {
        self.get_json("/api/playlists")
    }

    pub fn playlist(&self, id: i64) -> Result<(String, Vec<Track>), String> {
        let v: Value = self.get_json(&format!("/api/playlists/{id}"))?;
        let name = v
            .get("name")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let tracks: Vec<Track> = v
            .get("tracks")
            .cloned()
            .and_then(|t| serde_json::from_value(t).ok())
            .unwrap_or_default();
        Ok((name, tracks))
    }

    pub fn create_playlist(&self, name: &str) -> Result<(), String> {
        self.post_ok("/api/playlists", &json!({ "name": name }))
    }

    pub fn delete_playlist(&self, id: i64) -> Result<(), String> {
        self.send(self.http.delete(self.url(&format!("/api/playlists/{id}"))))
            .map(|_| ())
    }

    pub fn add_to_playlist(&self, id: i64, t: &Track) -> Result<(), String> {
        self.post_ok(&format!("/api/playlists/{id}/tracks"), &track_json(t))
    }

    pub fn remove_from_playlist(&self, id: i64, t: &Track) -> Result<(), String> {
        self.send(
            self.http
                .delete(self.url(&format!("/api/playlists/{id}/tracks/{}", t.key()))),
        )
        .map(|_| ())
    }

    // ---------- Âm thanh ----------
    /// Tải trọn một bài về RAM (khoảng 3-6 MB, định dạng AAC trong MP4).
    /// Server trả từng đoạn theo header Range nên ta xin lần lượt đến khi đủ.
    pub fn download_audio(&self, t: &Track) -> Result<Vec<u8>, String> {
        const MAX: usize = 80 * 1024 * 1024;
        if t.src != "yt" {
            // Nguồn cũ (jm/au): link trực tiếp nằm trong `url`.
            if t.url.is_empty() {
                return Err("Bài này không có link phát".into());
            }
            let u = if t.url.starts_with("http") {
                t.url.clone()
            } else {
                self.url(&t.url)
            };
            let r = self
                .http
                .get(u)
                .timeout(Duration::from_secs(120))
                .send()
                .map_err(|e| format!("Không tải được bài: {e}"))?;
            if !r.status().is_success() {
                return Err(format!("Lỗi tải bài: {}", r.status()));
            }
            return r
                .bytes()
                .map(|b| b.to_vec())
                .map_err(|e| format!("Tải bài bị đứt: {e}"));
        }

        let path = format!("/api/yt/audio/{}", t.id);
        let mut out: Vec<u8> = Vec::new();
        let mut total: u64 = 0;
        for _ in 0..400 {
            let start = out.len() as u64;
            let rb = self
                .http
                .get(self.url(&path))
                .query(&[("f", "mp4"), ("t", self.token.as_str())])
                .header("Range", format!("bytes={start}-"))
                .timeout(Duration::from_secs(60));
            let r = rb
                .send()
                .map_err(|e| format!("Không tải được âm thanh: {e}"))?;
            let status = r.status();
            if !status.is_success() {
                let body = r.text().unwrap_or_default();
                return Err(if body.trim().is_empty() {
                    format!("Lỗi âm thanh {status}")
                } else {
                    body
                });
            }
            let cr = r
                .headers()
                .get("content-range")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            let bytes = r.bytes().map_err(|e| format!("Tải âm thanh bị đứt: {e}"))?;
            if bytes.is_empty() {
                return Err("Máy chủ trả về đoạn âm thanh rỗng".into());
            }
            if status.as_u16() == 200 {
                // Server bỏ qua Range và trả cả file.
                if out.is_empty() {
                    return Ok(bytes.to_vec());
                }
                out.extend_from_slice(&bytes[(start as usize).min(bytes.len())..]);
                return Ok(out);
            }
            if total == 0 {
                total = cr
                    .as_deref()
                    .and_then(|s| s.rsplit('/').next())
                    .and_then(|x| x.trim().parse::<u64>().ok())
                    .unwrap_or(0);
            }
            out.extend_from_slice(&bytes);
            if out.len() > MAX {
                return Err("File âm thanh quá lớn".into());
            }
            if total > 0 && out.len() as u64 >= total {
                return Ok(out);
            }
            if total == 0 {
                // Không biết tổng dung lượng: coi như đã đủ sau đoạn đầu.
                return Ok(out);
            }
        }
        Err("Tải âm thanh quá nhiều đoạn".into())
    }
}

fn parse_auth(v: Value) -> Result<(String, User), String> {
    let token = v
        .get("token")
        .and_then(|x| x.as_str())
        .ok_or("Phản hồi đăng nhập thiếu token")?
        .to_string();
    let user: User = v
        .get("user")
        .cloned()
        .and_then(|u| serde_json::from_value(u).ok())
        .ok_or("Phản hồi đăng nhập thiếu thông tin tài khoản")?;
    Ok((token, user))
}

fn track_json(t: &Track) -> Value {
    json!({
        "src": t.src, "id": t.id, "title": t.title, "artist": t.artist,
        "image": t.image, "url": t.url, "dur": t.dur as u64, "explicit": t.explicit
    })
}
