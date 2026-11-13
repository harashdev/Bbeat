package vn.muse.app

import android.os.Handler
import android.os.Looper

/** Trạng thái phát hiện tại (do web gửi sang) + kênh gửi lệnh ngược lại cho web. Dùng chung giữa Activity và Service. */
object Playback {
    @Volatile var title: String = ""
    @Volatile var artist: String = ""
    @Volatile var art: String = ""
    @Volatile var playing: Boolean = false
    @Volatile var posMs: Long = 0
    @Volatile var durMs: Long = 0
    @Volatile var hasTrack: Boolean = false

    /** Activity gán hàm này để chạy JS trong WebView. */
    @Volatile var sendToWeb: ((String) -> Unit)? = null
    /** Service gán hàm này để được báo khi trạng thái đổi. */
    @Volatile var onChanged: (() -> Unit)? = null

    private val main = Handler(Looper.getMainLooper())

    fun command(cmd: String, arg: Long = 0) {
        val js = if (cmd == "seek") "window.museCmd&&museCmd('seek',$arg)" else "window.museCmd&&museCmd('$cmd')"
        main.post { sendToWeb?.invoke(js) }
    }
}
