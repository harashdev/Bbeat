/* Service worker: mở được giao diện khi mất mạng (dữ liệu danh sách phát lấy từ bộ nhớ cục bộ, nhạc đã tải nằm trong IndexedDB).
   - Trang chính: ưu tiên mạng nhưng chỉ chờ tối đa 4 giây rồi dùng bản đã lưu (mạng "treo" thì vẫn mở được app).
   - Ảnh bìa (YouTube/Google): lưu lại khi xem online để offline vẫn còn hình. */
const CACHE = 'muse-shell-v4', IMG = 'muse-img-v2', IMG_MAX = 400;
self.addEventListener('install', e => { e.waitUntil(caches.open(CACHE).then(c => c.add('/')).catch(() => {})); self.skipWaiting(); });
self.addEventListener('activate', e => { e.waitUntil(caches.keys().then(ks => Promise.all(ks.filter(k => k !== CACHE && k !== IMG).map(k => caches.delete(k)))).then(() => self.clients.claim())); });
const timeout = (ms) => new Promise((_, rej) => setTimeout(() => rej(new Error('timeout')), ms));
self.addEventListener('fetch', e => {
  const r = e.request;
  if (r.method !== 'GET') return;
  if (r.mode === 'navigate') {
    e.respondWith(Promise.race([fetch(r), timeout(4000)])
      .then(res => { const cp = res.clone(); caches.open(CACHE).then(c => c.put('/', cp)).catch(() => {}); return res; })
      .catch(() => caches.match('/').then(x => x || fetch(r))));
    return;
  }
  if (r.destination === 'image' && /(^|\.)(ytimg\.com|ggpht\.com|googleusercontent\.com)$/.test(new URL(r.url).hostname)) {
    /* Cache-first: ảnh đã xem hiện ngay. Chỉ lưu phản hồi hợp lệ (CORS, 200) – bản "opaque" có thể là lỗi 429/404 và bị lưu mãi → ảnh trống. */
    e.respondWith(caches.open(IMG).then(async c => {
      const hit = await c.match(r.url);
      if (hit) return hit;
      try {
        const res = await fetch(r.url, { mode: 'cors', credentials: 'omit' });
        if (res.ok) { c.put(r.url, res.clone()).then(() => c.keys()).then(ks => { if (ks.length > IMG_MAX) c.delete(ks[0]); }).catch(() => {}); }
        return res;
      } catch (_) { return fetch(r); }
    }));
  }
});
