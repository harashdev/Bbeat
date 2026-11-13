package vn.muse.app

import android.Manifest
import android.annotation.SuppressLint
import android.app.AlertDialog
import android.content.ActivityNotFoundException
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Color
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.webkit.RenderProcessGoneDetail
import android.text.InputType
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.webkit.CookieManager
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.Button
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import androidx.activity.ComponentActivity
import androidx.activity.OnBackPressedCallback
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import kotlin.concurrent.thread

/**
 * Muse cho Android = đúng giao diện web của server (giống app máy tính), đặt trong WebView.
 * Địa chỉ server gắn sẵn lúc build (MUSE_URL) nên không phải nhập IP.
 */
class MainActivity : ComponentActivity() {
    private val bg = Color.parseColor("#07070B")
    private lateinit var root: FrameLayout
    private lateinit var web: WebView
    private lateinit var errorBox: LinearLayout
    private lateinit var errorText: TextView
    private val prefs by lazy { getSharedPreferences("muse", MODE_PRIVATE) }

    private var fileCb: ValueCallback<Array<Uri>>? = null
    private var customView: View? = null
    private var customCb: WebChromeClient.CustomViewCallback? = null

    private val askNotif = registerForActivityResult(ActivityResultContracts.RequestPermission()) {}
    private val pickFile = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { r ->
        fileCb?.onReceiveValue(WebChromeClient.FileChooserParams.parseResult(r.resultCode, r.data))
        fileCb = null
    }

    // Bản sao giao diện web (HTML) lưu trong máy: khi không kết nối được server, vẫn mở app được để nghe các bài đã tải.
    // Nạp bằng loadDataWithBaseURL với địa chỉ server nên dùng chung localStorage / IndexedDB với bản online.
    private val shellFile by lazy { File(filesDir, "shell.html") }
    private var lastShellFetch = 0L

    private fun cacheShell() {
        val now = System.currentTimeMillis()
        if (now - lastShellFetch < 6 * 3600_000L && shellFile.exists()) return
        val base = serverUrl()
        if (base.isBlank()) return
        lastShellFetch = now
        thread(name = "muse-shell") {
            try {
                val c = URL("$base/").openConnection() as HttpURLConnection
                c.connectTimeout = 8000; c.readTimeout = 8000
                c.setRequestProperty("Accept", "text/html")
                if (c.responseCode == 200) {
                    val html = c.inputStream.bufferedReader(Charsets.UTF_8).use { it.readText() }
                    if (html.contains("<title>BBeat")) shellFile.writeText(html, Charsets.UTF_8)
                }
                c.disconnect()
            } catch (e: Exception) { lastShellFetch = 0L /* thử lại lần sau */ }
        }
    }

    /** true nếu đã nạp được bản lưu sẵn. */
    private fun loadCachedShell(): Boolean {
        if (!shellFile.exists()) return false
        return try {
            val base = serverUrl() + "/"
            errorBox.visibility = View.GONE
            web.visibility = View.VISIBLE
            web.loadDataWithBaseURL(base, shellFile.readText(Charsets.UTF_8), "text/html", "UTF-8", base)
            true
        } catch (e: Exception) { false }
    }

    private fun serverUrl(): String =
        (prefs.getString("server", "")?.takeIf { it.isNotBlank() } ?: BuildConfig.MUSE_URL).trim().trimEnd('/')

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.statusBarColor = bg
        window.navigationBarColor = bg

        root = FrameLayout(this).apply { setBackgroundColor(bg) }
        web = WebView(this).apply {
            setBackgroundColor(bg)
            layoutParams = FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
        }
        root.addView(web)
        buildErrorBox()
        root.addView(errorBox)
        setContentView(root)

        setupWebView()
        onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() {
                when {
                    customView != null -> chrome.onHideCustomView()
                    web.canGoBack() -> web.goBack()
                    else -> moveTaskToBack(true) // thoát ra nhưng nhạc vẫn chạy nền
                }
            }
        })

        if (Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) askNotif.launch(Manifest.permission.POST_NOTIFICATIONS)
        // Dịch vụ nền được MuseBridge bật khi bắt đầu phát nhạc, không bật ngay lúc mở app.
        Playback.sendToWeb = { js -> web.evaluateJavascript(js, null) }

        // Giữ cache WebView (LOAD_DEFAULT vẫn tự kiểm tra bản mới qua ETag) -> mở app và tải ảnh bìa nhanh hơn nhiều.
        loadServer()
    }

    private val chrome = object : WebChromeClient() {
        override fun onShowFileChooser(
            view: WebView, cb: ValueCallback<Array<Uri>>, params: WebChromeClient.FileChooserParams,
        ): Boolean {
            fileCb?.onReceiveValue(null)
            fileCb = cb
            return try {
                pickFile.launch(params.createIntent()); true
            } catch (e: Exception) {
                fileCb = null; cb.onReceiveValue(null); false
            }
        }

        override fun onShowCustomView(view: View, callback: WebChromeClient.CustomViewCallback) {
            if (customView != null) { callback.onCustomViewHidden(); return }
            customView = view; customCb = callback
            root.addView(view, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
            web.visibility = View.GONE
        }

        override fun onHideCustomView() {
            customView?.let { root.removeView(it) }
            customView = null
            customCb?.onCustomViewHidden()
            customCb = null
            web.visibility = View.VISIBLE
        }
    }

    @SuppressLint("SetJavaScriptEnabled")
    private fun setupWebView() {
        val s = web.settings
        s.javaScriptEnabled = true
        s.domStorageEnabled = true
        s.databaseEnabled = true
        s.mediaPlaybackRequiresUserGesture = false
        s.mixedContentMode = WebSettings.MIXED_CONTENT_COMPATIBILITY_MODE
        s.loadWithOverviewMode = true
        s.useWideViewPort = true
        s.userAgentString = s.userAgentString.replace("; wv", "") + " MuseAndroid" // YouTube coi như Chrome; web nhận ra app để dùng giao diện máy tính
        // Vẽ sẵn phần ngoài màn hình chỉ có lợi trên máy khoẻ; máy RAM thấp bật lên lại gây giật và tốn bộ nhớ.
        val am = getSystemService(ACTIVITY_SERVICE) as android.app.ActivityManager
        s.setOffscreenPreRaster(!am.isLowRamDevice && am.memoryClass >= 256)
        s.cacheMode = WebSettings.LOAD_DEFAULT
        s.allowFileAccess = false
        s.setGeolocationEnabled(false)
        s.setSupportZoom(true)
        s.builtInZoomControls = true
        s.displayZoomControls = false
        s.textZoom = 100
        CookieManager.getInstance().apply {
            setAcceptCookie(true)
            setAcceptThirdPartyCookies(web, true)
        }
        WebView.setWebContentsDebuggingEnabled(BuildConfig.DEBUG)
        MuseSdk.init(this)
        web.addJavascriptInterface(MuseBridge(applicationContext), "MuseNative")
        // Không ép LAYER_TYPE_HARDWARE cho cả WebView: nó bắt WebView vẽ qua thêm một texture lớn rồi mới ghép -> tốn GPU/RAM, cuộn giật.
        // Để mặc định (Activity đã bật hardwareAccelerated trong manifest), Chromium tự ghép lớp hiệu quả hơn.
        web.setLayerType(View.LAYER_TYPE_NONE, null)
        // Giữ tiến trình vẽ ở mức ưu tiên cao khi app đang hiện (hệ thống ít "bỏ đói" renderer hơn).
        web.setRendererPriorityPolicy(WebView.RENDERER_PRIORITY_IMPORTANT, false)
        web.webChromeClient = chrome
        web.webViewClient = object : WebViewClient() {
            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                if (!request.isForMainFrame) return false
                val u = request.url
                if (u.host != null && u.host == Uri.parse(serverUrl()).host) return false
                // Đăng nhập Discord (OAuth) chạy ngay trong app: xong Discord chuyển về server (cùng địa chỉ với app) nên không phải sang trình duyệt rồi quay lại.
                val h = u.host?.lowercase().orEmpty()
                if (h == "discord.com" || h.endsWith(".discord.com") || h == "discordapp.com" || h.endsWith(".discordapp.com")) return false
                // Các link ra ngoài khác mở bằng trình duyệt.
                try { startActivity(Intent(Intent.ACTION_VIEW, u)) } catch (e: ActivityNotFoundException) { /* không có app mở link */ }
                return true
            }

            override fun onReceivedError(view: WebView, request: WebResourceRequest, error: WebResourceError) {
                if (request.isForMainFrame) {
                    if (loadCachedShell()) return   // offline: mở giao diện đã lưu, web tự chuyển sang danh sách bài đã tải
                    showError("Không kết nối được tới\n${serverUrl()}\n\n(${error.description})")
                }
            }

            override fun onRenderProcessGone(view: WebView, detail: RenderProcessGoneDetail): Boolean {
                runCatching { root.removeView(web); web.destroy() }
                web = WebView(this@MainActivity).apply {
                    setBackgroundColor(bg)
                    layoutParams = FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
                }
                root.addView(web, 0)
                setupWebView(); loadServer()
                return true
            }

            override fun onPageFinished(view: WebView, url: String?) {
                if (errorBox.visibility != View.VISIBLE) web.visibility = View.VISIBLE
                if (url != null && url.startsWith(serverUrl())) cacheShell()
            }
        }
    }

    private fun buildErrorBox() {
        val dp = resources.displayMetrics.density
        errorText = TextView(this).apply {
            setTextColor(Color.parseColor("#EDEDF2")); textSize = 16f; gravity = Gravity.CENTER
        }
        val hint = TextView(this).apply {
            text = "Hãy bật Tailscale (nếu dùng) và chắc chắn server Muse đang chạy."
            setTextColor(Color.parseColor("#A6A6B4")); textSize = 13f; gravity = Gravity.CENTER
            setPadding(0, (12 * dp).toInt(), 0, (20 * dp).toInt())
        }
        val retry = Button(this).apply { text = "Thử lại"; setOnClickListener { loadServer() } }
        val change = Button(this).apply { text = "Đổi địa chỉ server"; setOnClickListener { askServer() } }
        errorBox = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER
            setPadding((28 * dp).toInt(), 0, (28 * dp).toInt(), 0)
            setBackgroundColor(bg)
            visibility = View.GONE
            addView(errorText); addView(hint); addView(retry); addView(change)
            layoutParams = FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
            isClickable = true
        }
    }

    private fun showError(msg: String) {
        errorText.text = msg
        errorBox.visibility = View.VISIBLE
        web.visibility = View.INVISIBLE
    }

    private fun loadServer() {
        val url = serverUrl()
        if (url.isBlank()) { askServer(); return }
        errorBox.visibility = View.GONE
        web.visibility = View.VISIBLE
        web.loadUrl(url)
    }

    /** Chỉ hỏi khi build không gắn sẵn địa chỉ, hoặc khi bấm "Đổi địa chỉ server". */
    private fun askServer() {
        val input = EditText(this).apply {
            inputType = InputType.TYPE_TEXT_VARIATION_URI
            hint = "https://ten-may.xxxx.ts.net"
            setText(serverUrl())
        }
        AlertDialog.Builder(this)
            .setTitle("Địa chỉ server Muse")
            .setView(input)
            .setPositiveButton("Lưu") { _, _ ->
                var v = input.text.toString().trim().trimEnd('/')
                if (v.isNotEmpty() && !v.contains("://")) {
                    val local = Regex("^(localhost|\\d{1,3}(\\.\\d{1,3}){3})(:\\d+)?$").matches(v)
                    v = (if (local) "http://" else "https://") + v
                }
                prefs.edit().putString("server", v).apply()
                loadServer()
            }
            .setNegativeButton("Hủy", null)
            .show()
    }

    override fun onResume() {
        super.onResume()
        web.onResume(); web.resumeTimers()
    }

    /** Tắt màn hình / ra màn hình chính: giữ WebView và bộ đếm giờ chạy tiếp để nhạc không bị dừng. */
    override fun onStop() {
        super.onStop()
        if (Playback.playing) { web.onResume(); web.resumeTimers() }
    }

    override fun onDestroy() {
        Playback.sendToWeb = null
        if (isFinishing) {
            stopService(Intent(this, KeepAliveService::class.java))
            web.destroy()
        }
        super.onDestroy()
    }
}
