//! Phát âm thanh native bằng rodio (giải mã AAC/MP4 qua symphonia). Không có WebView, không có player YouTube.

use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::io::Cursor;
use std::time::{Duration, Instant};

pub struct Player {
    _stream: Option<OutputStream>,
    handle: Option<OutputStreamHandle>,
    sink: Option<Sink>,
    pub volume: f32,
    /// Số giây đã phát tính đến lần bấm phát gần nhất.
    base: f64,
    /// Some(..) khi đang phát, None khi tạm dừng/chưa có bài.
    resumed: Option<Instant>,
    pub duration: f64,
}

impl Player {
    pub fn new() -> Self {
        let (stream, handle) = match OutputStream::try_default() {
            Ok((s, h)) => (Some(s), Some(h)),
            Err(_) => (None, None),
        };
        Player {
            _stream: stream,
            handle,
            sink: None,
            volume: 0.8,
            base: 0.0,
            resumed: None,
            duration: 0.0,
        }
    }

    /// Nạp bài mới từ bytes AAC/MP4 và phát ngay.
    pub fn load(&mut self, bytes: Vec<u8>, dur_hint: f64) -> Result<(), String> {
        let handle = self
            .handle
            .as_ref()
            .ok_or("Không tìm thấy thiết bị âm thanh")?;
        let dec = Decoder::new(Cursor::new(bytes))
            .map_err(|e| format!("Không giải mã được bài này: {e}"))?;
        let total = dec.total_duration().map(|d| d.as_secs_f64());
        let sink = Sink::try_new(handle).map_err(|e| format!("Lỗi thiết bị âm thanh: {e}"))?;
        sink.set_volume(self.volume);
        sink.append(dec);
        if let Some(old) = self.sink.replace(sink) {
            old.stop();
        }
        self.duration = total.filter(|d| *d > 0.0).unwrap_or(dur_hint);
        self.base = 0.0;
        self.resumed = Some(Instant::now());
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(s) = self.sink.take() {
            s.stop();
        }
        self.base = 0.0;
        self.resumed = None;
        self.duration = 0.0;
    }

    pub fn has_track(&self) -> bool {
        self.sink.is_some()
    }

    pub fn is_playing(&self) -> bool {
        self.sink.is_some() && self.resumed.is_some()
    }

    pub fn toggle(&mut self) {
        let Some(s) = &self.sink else { return };
        if let Some(t) = self.resumed.take() {
            self.base += t.elapsed().as_secs_f64();
            s.pause();
        } else {
            s.play();
            self.resumed = Some(Instant::now());
        }
    }

    pub fn position(&self) -> f64 {
        let mut p = self.base
            + self
                .resumed
                .map(|t| t.elapsed().as_secs_f64())
                .unwrap_or(0.0);
        if self.duration > 0.0 {
            p = p.min(self.duration);
        }
        p
    }

    pub fn seek(&mut self, secs: f64) {
        let Some(s) = &self.sink else { return };
        let secs = secs.max(0.0);
        if s.try_seek(Duration::from_secs_f64(secs)).is_ok() {
            self.base = secs;
            if self.resumed.is_some() {
                self.resumed = Some(Instant::now());
            }
        }
    }

    pub fn set_volume(&mut self, v: f32) {
        self.volume = v.clamp(0.0, 1.0);
        if let Some(s) = &self.sink {
            s.set_volume(self.volume);
        }
    }

    /// Bài đã phát hết (hàng đợi trong sink rỗng khi đang ở trạng thái phát).
    pub fn finished(&self) -> bool {
        self.resumed.is_some() && self.sink.as_ref().map(|s| s.empty()).unwrap_or(false)
    }
}
