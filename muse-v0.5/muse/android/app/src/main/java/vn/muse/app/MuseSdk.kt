package vn.muse.app

import android.app.Activity

/**
 * Discord Social SDK: Rich Presence bằng token OAuth, không cần mở app Discord.
 * Thư viện chỉ có trong bản build có kèm SDK; thiếu thì [available] = false và app chạy bình thường.
 */
object MuseSdk {
    @Volatile private var loaded = false
    @Volatile private var failure = ""

    val available: Boolean get() = loaded

    @Synchronized
    fun init(activity: Activity) {
        if (loaded) return
        try {
            System.loadLibrary("discord_partner_sdk") // chạy JNI_OnLoad của SDK
            val c = Class.forName("com.discord.socialsdk.DiscordSocialSdkInit")
            c.getMethod("setEngineActivity", Activity::class.java).invoke(null, activity)
            System.loadLibrary("muse_presence")
            loaded = true
        } catch (t: Throwable) {
            failure = t.javaClass.simpleName + ": " + (t.message ?: "")
        }
    }

    fun start(appId: String, token: String): String {
        if (!loaded) return "Bản app này chưa gắn Discord SDK" + if (failure.isNotEmpty()) " ($failure)" else ""
        val id = appId.trim().toLongOrNull() ?: return "DISCORD_CLIENT_ID không hợp lệ"
        return if (nStart(id, token) == 0) "" else "Không khởi động được Discord SDK"
    }

    /** "ready" | "connecting" | "off" | "err:<chi tiết>" */
    fun status(): String {
        if (!loaded) return "off"
        val err = nError()
        return when (nStatus()) {
            2 -> "ready"
            1 -> "connecting"
            3 -> "err:$err"
            else -> if (err.isNotEmpty()) "err:$err" else "off"
        }
    }

    fun update(title: String, artist: String, image: String, startMs: Long, endMs: Long) {
        if (loaded) nUpdate(title, artist, image, startMs / 1000, endMs / 1000)
    }

    fun clear() { if (loaded) nClear() }

    @JvmStatic private external fun nStart(appId: Long, token: String): Int
    @JvmStatic private external fun nUpdate(title: String, artist: String, image: String, startS: Long, endS: Long)
    @JvmStatic private external fun nClear()
    @JvmStatic private external fun nStatus(): Int
    @JvmStatic private external fun nError(): String
}
