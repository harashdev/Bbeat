package vn.muse.app

import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.MediaStore
import android.util.Base64
import android.webkit.JavascriptInterface
import java.io.File
import java.io.FileOutputStream
import java.io.OutputStream
import androidx.core.content.ContextCompat

/** Cầu nối JS -> native. Trong web gọi: MuseNative.update(title, artist, art, playing, posMs, durMs, hasTrack). */
class MuseBridge(private val ctx: Context) {
    @JavascriptInterface
    fun update(title: String, artist: String, art: String, playing: Boolean, posMs: Long, durMs: Long, hasTrack: Boolean) {
        val trackChanged = title != Playback.title || artist != Playback.artist || art != Playback.art
        val playChanged = playing != Playback.playing
        Playback.title = title; Playback.artist = artist; Playback.art = art
        Playback.playing = playing; Playback.posMs = posMs; Playback.durMs = durMs; Playback.hasTrack = hasTrack
        if (hasTrack && playing) {
            // Chỉ khởi động service khi người dùng đang phát nhạc (app đang ở trước nền).
            runCatching { ContextCompat.startForegroundService(ctx, Intent(ctx, KeepAliveService::class.java)) }
        }
        if (trackChanged || playChanged || hasTrack) Playback.onChanged?.invoke()
    }

    // ---- Lưu file nhạc tải về (WAV / m4a / webm...) vào Music/BBeat ----
    // Web gọi lần lượt: saveBegin(tên, mime) -> saveChunk(base64) nhiều lần -> saveEnd(true|false). Trả "" nếu ổn, "err:..." nếu lỗi.
    private var saveOut: OutputStream? = null
    private var saveUri: Uri? = null
    private var saveFile: File? = null

    @JavascriptInterface
    @Synchronized
    fun saveBegin(name: String, mime: String): String {
        saveEnd(false) // dọn lần lưu dở trước đó (nếu có)
        return try {
            val safe = name.filter { it >= ' ' }.replace(Regex("[\\/:*?\"<>|]"), "_").take(120).ifBlank { "bai-hat" }
            if (Build.VERSION.SDK_INT >= 29) {
                val v = ContentValues().apply {
                    put(MediaStore.Audio.Media.DISPLAY_NAME, safe)
                    put(MediaStore.Audio.Media.MIME_TYPE, mime.ifBlank { "audio/mp4" })
                    put(MediaStore.Audio.Media.RELATIVE_PATH, "Music/BBeat")
                    put(MediaStore.Audio.Media.IS_PENDING, 1)
                }
                val uri = ctx.contentResolver.insert(MediaStore.Audio.Media.EXTERNAL_CONTENT_URI, v)
                    ?: return "err:không tạo được file trong Music/BBeat"
                saveUri = uri
                saveOut = ctx.contentResolver.openOutputStream(uri) ?: return "err:không mở được file để ghi"
            } else {
                // Android 8-9: ghi vào thư mục riêng của app (không cần xin quyền): Android/data/vn.muse.app/files/Music/BBeat
                val dir = File(ctx.getExternalFilesDir(Environment.DIRECTORY_MUSIC), "BBeat")
                dir.mkdirs()
                val f = File(dir, safe)
                saveFile = f
                saveOut = FileOutputStream(f)
            }
            ""
        } catch (e: Exception) {
            saveEnd(false)
            "err:" + (e.message ?: "lỗi tạo file")
        }
    }

    @JavascriptInterface
    @Synchronized
    fun saveChunk(b64: String): String {
        val o = saveOut ?: return "err:chưa mở file để ghi"
        return try {
            o.write(Base64.decode(b64, Base64.DEFAULT))
            ""
        } catch (e: Exception) {
            "err:" + (e.message ?: "lỗi ghi file")
        }
    }

    @JavascriptInterface
    @Synchronized
    fun saveEnd(ok: Boolean) {
        try { saveOut?.flush(); saveOut?.close() } catch (_: Exception) {}
        saveOut = null
        val uri = saveUri
        val f = saveFile
        saveUri = null
        saveFile = null
        try {
            if (uri != null) {
                if (ok) {
                    ctx.contentResolver.update(uri, ContentValues().apply { put(MediaStore.Audio.Media.IS_PENDING, 0) }, null, null)
                } else {
                    ctx.contentResolver.delete(uri, null, null)
                }
            } else if (f != null && !ok) {
                f.delete()
            }
        } catch (_: Exception) {}
    }

    // ---- Discord Social SDK (Rich Presence không cần app Discord) ----
    /** Chia sẻ bài hát: mở bảng chia sẻ của Android (Zalo, Messenger, Telegram...). Trả false nếu không mở được. */
    @JavascriptInterface
    fun share(text: String): Boolean = try {
        val send = Intent(Intent.ACTION_SEND).apply { type = "text/plain"; putExtra(Intent.EXTRA_TEXT, text) }
        val chooser = Intent.createChooser(send, null).apply { addFlags(Intent.FLAG_ACTIVITY_NEW_TASK) }
        ctx.startActivity(chooser)
        true
    } catch (e: Exception) { false }

    @JavascriptInterface
    fun sdkAvailable(): Boolean = MuseSdk.available

    /** Trả "" nếu bắt đầu được, ngược lại là thông báo lỗi. */
    @JavascriptInterface
    fun sdkStart(appId: String, token: String): String = MuseSdk.start(appId, token)

    @JavascriptInterface
    fun sdkStatus(): String = MuseSdk.status()

    @JavascriptInterface
    fun sdkUpdate(title: String, artist: String, image: String, startMs: Long, endMs: Long) =
        MuseSdk.update(title, artist, image, startMs, endMs)

    @JavascriptInterface
    fun sdkClear() = MuseSdk.clear()
}
