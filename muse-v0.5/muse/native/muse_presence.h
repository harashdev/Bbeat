#pragma once
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
// Rich Presence bằng Discord Social SDK (đăng nhập OAuth, không cần mở app Discord).
// token: access token OAuth có scope sdk.social_layer_presence. Trả 0 nếu bắt đầu được.
int muse_pres_start(uint64_t app_id, const char* token);
// Cập nhật "Đang nghe". start_s/end_s là giây epoch (0 = bỏ qua).
void muse_pres_update(const char* title, const char* artist, const char* image, int64_t start_s, int64_t end_s);
void muse_pres_clear(void);
void muse_pres_stop(void);
// 0 = chưa chạy, 1 = đang kết nối, 2 = sẵn sàng, 3 = lỗi. Chi tiết lỗi ở muse_pres_error().
int muse_pres_status(void);
const char* muse_pres_error(void);
#ifdef __cplusplus
}
#endif
