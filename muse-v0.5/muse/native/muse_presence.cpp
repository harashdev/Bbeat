#define DISCORDPP_IMPLEMENTATION
#include "discordpp.h"
#include "muse_presence.h"

#include <atomic>
#include <chrono>
#include <cstring>
#include <memory>
#include <mutex>
#include <string>
#include <thread>

namespace {
std::mutex g_mu;  // mọi lời gọi SDK (kể cả RunCallbacks) đều qua khóa này
std::shared_ptr<discordpp::Client> g_client;
std::thread g_thread;
std::atomic<bool> g_run{false};
std::atomic<int> g_status{0};
std::string g_error;

bool g_has_pending = false;  // hoạt động chờ gửi khi client sẵn sàng
bool g_pending_clear = false;
discordpp::Activity g_pending;

std::string cut(const char* s, size_t n) {
    std::string r = s ? s : "";
    // cắt theo ký tự UTF-8 để không làm hỏng chuỗi
    size_t chars = 0, i = 0;
    while (i < r.size()) {
        unsigned char c = (unsigned char)r[i];
        size_t len = c < 0x80 ? 1 : (c >> 5) == 6 ? 2 : (c >> 4) == 14 ? 3 : 4;
        if (chars == n) { r.resize(i); break; }
        i += len; chars++;
    }
    return r;
}

void set_error(const std::string& e) { g_error = e; g_status = 3; }

// Gọi khi đã giữ g_mu
void flush_pending_locked() {
    if (!g_client || g_status != 2) return;
    if (g_pending_clear) {
        g_client->ClearRichPresence();
        g_pending_clear = false;
        g_has_pending = false;
    } else if (g_has_pending) {
        g_client->UpdateRichPresence(g_pending, [](discordpp::ClientResult r) {
            if (!r.Successful()) {
                std::lock_guard<std::mutex> lk(g_mu);
                g_error = "UpdateRichPresence: " + r.Error();
            }
        });
        g_has_pending = false;
    }
}
}  // namespace

extern "C" {

int muse_pres_start(uint64_t app_id, const char* token) {
    if (!token || !*token || app_id == 0) return 1;
    muse_pres_stop();
    std::lock_guard<std::mutex> lk(g_mu);
    g_error.clear();
    g_status = 1;
    g_client = std::make_shared<discordpp::Client>();
    g_client->SetApplicationId(app_id);
    g_client->SetStatusChangedCallback([](discordpp::Client::Status s, discordpp::Client::Error e, int32_t detail) {
        // callback chạy trong RunCallbacks (đã giữ g_mu) -> không khóa lại
        if (s == discordpp::Client::Status::Ready) {
            g_status = 2;
            flush_pending_locked();
        } else if (e != discordpp::Client::Error::None) {
            set_error("Discord lỗi " + std::to_string((int)e) + " / " + std::to_string(detail));
        } else if (s == discordpp::Client::Status::Disconnected) {
            if (g_status != 3) g_status = 0;
        } else {
            g_status = 1;
        }
    });
    std::string tk = token;
    g_client->UpdateToken(discordpp::AuthorizationTokenType::Bearer, tk, [](discordpp::ClientResult r) {
        if (!r.Successful()) { set_error("Token: " + r.Error()); return; }
        if (g_client) g_client->Connect();
    });
    g_run = true;
    g_thread = std::thread([] {
        while (g_run) {
            {
                std::lock_guard<std::mutex> lk(g_mu);
                discordpp::RunCallbacks();
            }
            std::this_thread::sleep_for(std::chrono::milliseconds(60));
        }
    });
    return 0;
}

void muse_pres_update(const char* title, const char* artist, const char* image, int64_t start_s, int64_t end_s) {
    std::lock_guard<std::mutex> lk(g_mu);
    if (!g_client) return;
    discordpp::Activity a;
    a.SetType(discordpp::ActivityTypes::Listening);
    a.SetDetails(cut(title, 120));
    std::string ar = cut(artist, 120);
    a.SetState(ar.empty() ? std::string("Muse") : ar);
    if (start_s > 0 || end_s > 0) {
        discordpp::ActivityTimestamps ts;
        if (start_s > 0) ts.SetStart((uint64_t)start_s);
        if (end_s > 0) ts.SetEnd((uint64_t)end_s);
        a.SetTimestamps(ts);
    }
    if (image && strncmp(image, "https://", 8) == 0) {
        discordpp::ActivityAssets as;
        as.SetLargeImage(std::string(image));
        as.SetLargeText(cut(title, 120));
        a.SetAssets(as);
    }
    g_pending = a;
    g_has_pending = true;
    g_pending_clear = false;
    flush_pending_locked();
}

void muse_pres_clear(void) {
    std::lock_guard<std::mutex> lk(g_mu);
    g_has_pending = false;
    g_pending_clear = true;
    flush_pending_locked();
}

void muse_pres_stop(void) {
    g_run = false;
    if (g_thread.joinable()) g_thread.join();
    std::lock_guard<std::mutex> lk(g_mu);
    if (g_client) {
        if (g_status == 2) g_client->ClearRichPresence();
        g_client.reset();
    }
    g_has_pending = false;
    g_pending_clear = false;
    g_status = 0;
}

int muse_pres_status(void) { return g_status; }

const char* muse_pres_error(void) {
    static thread_local std::string copy;
    std::lock_guard<std::mutex> lk(g_mu);
    copy = g_error;
    return copy.c_str();
}

}  // extern "C"
