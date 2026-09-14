package org.servo.servoshell

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.ContentValues
import android.content.Intent
import android.graphics.Color
import android.net.Uri
import android.os.Bundle
import android.os.Environment
import android.os.SystemClock
import android.provider.MediaStore
import android.util.Base64
import android.view.View
import android.webkit.JavascriptInterface
import android.webkit.URLUtil
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import androidx.webkit.ServiceWorkerClientCompat
import androidx.webkit.ServiceWorkerControllerCompat
import androidx.webkit.WebViewAssetLoader
import androidx.webkit.WebViewFeature

/** The game runs on Android's system WebView; no Servo or JNI library is loaded. */
class MainActivity : Activity() {
    private lateinit var webView: WebView
    private lateinit var container: FrameLayout
    private lateinit var splash: RovesSplashView
    private var splashStarted = 0L
    private var startupPending = true
    private var startupNavigation = 0L
    private var destroyed = false
    private var fullscreenView: View? = null
    private var fullscreenCallback: WebChromeClient.CustomViewCallback? = null
    private var pendingFileChooserCallback: ValueCallback<Array<Uri>>? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        container = FrameLayout(this)
        container.setBackgroundColor(Color.BLACK)
        splash = RovesSplashView(this)
        splashStarted = SystemClock.uptimeMillis()
        // Paint branding before constructing the potentially expensive system WebView.
        container.addView(splash, FrameLayout.LayoutParams(-1, -1))
        setContentView(container)
        webView = WebView(this)
        webView.setBackgroundColor(Color.BLACK)
        container.addView(webView, 0, FrameLayout.LayoutParams(-1, -1))
        val themeColor = getString(R.string.servoThemeColor)
        if (themeColor.isNotEmpty()) {
            runCatching { window.statusBarColor = Color.parseColor(themeColor) }
        }
        enterImmersiveMode()
        val assets = WebViewAssetLoader.AssetsPathHandler(this)
        val loader = WebViewAssetLoader.Builder()
            // Serve the game at the origin root so /images and /audio also work. SPA
            // fallback mirrors iOS's GameSchemeHandler/desktop's game.rs: a client-side
            // router navigating to a path with no matching asset (e.g. /level/3) still gets
            // index.html, letting the router itself decide what to render, instead of a 404.
            //
            // AssetsPathHandler.handle() never actually returns null on a missing asset --
            // confirmed by reading its real implementation (androidx.webkit source): on an
            // IOException it returns a *non-null* WebResourceResponse with mimeType/encoding/
            // data all null, not null itself. A plain `?:` Elvis fallback on that call is
            // therefore dead code (the left side is never null), so this checks the actual
            // `data` stream instead -- the same signal shouldInterceptRequest below uses to
            // decide whether a request truly failed.
            .addPathHandler("/") { path ->
                val primary = assets.handle("www/$path")
                if (primary != null && primary.data != null) primary else assets.handle("www/index.html")
            }
            .build()

        // Shared by the WebViewClient below and, further down, the ServiceWorkerClientCompat --
        // a page's own service worker script/fetches are a *separate* request pipeline from the
        // main document/subresource one WebViewClient.shouldInterceptRequest covers, and need
        // their own interception hook to see this same local content instead of hitting the
        // real network (which fails outright, since appassets.androidplatform.net isn't a real,
        // resolvable domain) -- confirmed via real-device Chrome DevTools remote inspection:
        // vite-plugin-pwa's registerSW.js failed with "An unknown error occurred when fetching
        // the script" for sw.js before this existed.
        fun intercept(url: Uri): WebResourceResponse? {
            val response = loader.shouldInterceptRequest(url)
            if (url.host == WebViewAssetLoader.DEFAULT_DOMAIN && response?.data == null) {
                val body = "Not Found".byteInputStream(Charsets.UTF_8)
                return WebResourceResponse("text/plain", "UTF-8", 404, "Not Found", emptyMap(), body)
            }
            return response
        }

        webView.settings.apply {
            javaScriptEnabled = true
            domStorageEnabled = true
            mediaPlaybackRequiresUserGesture = false
            allowFileAccess = false
            allowContentAccess = false
        }
        WebView.setWebContentsDebuggingEnabled(
            (applicationInfo.flags and android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE) != 0
        )
        webView.webViewClient = object : WebViewClient() {
            override fun onPageStarted(view: WebView, url: String, favicon: android.graphics.Bitmap?) {
                startupNavigation++
            }

            override fun onPageFinished(view: WebView, url: String) {
                if (!startupPending || url != view.url) return
                val navigation = startupNavigation
                // Page completion alone doesn't guarantee its contents can be drawn.
                view.postVisualStateCallback(navigation, object : WebView.VisualStateCallback() {
                    override fun onComplete(requestId: Long) {
                        val remaining = (500L - (SystemClock.uptimeMillis() - splashStarted)).coerceAtLeast(0L)
                        splash.postDelayed({
                            if (!destroyed && startupPending && navigation == startupNavigation) {
                                startupPending = false
                                container.removeView(splash)
                            }
                        }, remaining)
                    }
                })
            }

            override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? =
                intercept(request.url)
            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                return request.url.scheme !in listOf("https", "http")
            }
        }

        // Feature-checked because not every WebView provider/version implements service worker
        // interception (older WebView releases, some OEM forks) -- skipping it there just means
        // a service worker (if the game registers one at all) falls back to the real network,
        // same as before this existed, rather than crashing on an unsupported API call.
        if (WebViewFeature.isFeatureSupported(WebViewFeature.SERVICE_WORKER_BASIC_USAGE) &&
            WebViewFeature.isFeatureSupported(WebViewFeature.SERVICE_WORKER_SHOULD_INTERCEPT_REQUEST)
        ) {
            ServiceWorkerControllerCompat.getInstance().setServiceWorkerClient(object : ServiceWorkerClientCompat() {
                override fun shouldInterceptRequest(request: WebResourceRequest): WebResourceResponse? =
                    intercept(request.url)
            })
        }

        // A game's save export (typically `<a download>.click()` on a Blob/data URL, e.g. a
        // JSON save file) needs both of these to actually do anything in a WebView -- neither
        // exists by default, unlike a real browser tab. Confirmed missing on a real device
        // (2026-09-14): the save menu's own import/export buttons were silently inert.
        webView.addJavascriptInterface(RovesFileBridge(), "RovesFiles")
        webView.setDownloadListener { url, _, contentDisposition, mimeType, _ ->
            val fileName = URLUtil.guessFileName(url, contentDisposition, mimeType)
            when {
                // A blob: URL is only valid inside the page's own JS context -- this fetches
                // it back out as base64 there and hands the bytes to RovesFileBridge below,
                // rather than trying (and failing) to resolve it as a real network request.
                url.startsWith("blob:") -> webView.evaluateJavascript(
                    "fetch(${jsStringLiteral(url)}).then(r => r.blob()).then(b => { " +
                        "const r = new FileReader(); " +
                        "r.onloadend = () => RovesFiles.saveDataUrl(r.result, ${jsStringLiteral(fileName)}); " +
                        "r.readAsDataURL(b); });",
                    null,
                )
                url.startsWith("data:") -> saveDataUrl(url, fileName)
                else -> {
                    // A real http(s) download (not a save export) -- hand it to the system's
                    // own Download Manager instead of trying to special-case every possible
                    // use a game's content might have for a download link.
                    runCatching {
                        val request = android.app.DownloadManager.Request(Uri.parse(url))
                            .setMimeType(mimeType)
                            .setNotificationVisibility(
                                android.app.DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED,
                            )
                            .setDestinationInExternalPublicDir(Environment.DIRECTORY_DOWNLOADS, fileName)
                        (getSystemService(DOWNLOAD_SERVICE) as android.app.DownloadManager).enqueue(request)
                    }
                }
            }
        }

        webView.webChromeClient = object : WebChromeClient() {
            override fun onShowCustomView(view: View, callback: CustomViewCallback) {
                if (fullscreenView != null) {
                    callback.onCustomViewHidden()
                    return
                }
                fullscreenView = view
                fullscreenCallback = callback
                webView.visibility = View.GONE
                container.addView(view, FrameLayout.LayoutParams(-1, -1))
                if (startupPending) splash.bringToFront()
                enterImmersiveMode()
            }
            override fun onHideCustomView() = leaveFullscreen()

            // A game's save import (an `<input type="file">` picker, e.g. loading a JSON
            // save file back in) needs this to show any file picker at all in a WebView --
            // confirmed missing the same way as the download side above.
            override fun onShowFileChooser(
                webView: WebView,
                filePathCallback: ValueCallback<Array<Uri>>,
                fileChooserParams: FileChooserParams,
            ): Boolean {
                pendingFileChooserCallback?.onReceiveValue(null)
                pendingFileChooserCallback = filePathCallback
                return try {
                    startActivityForResult(fileChooserParams.createIntent(), FILE_CHOOSER_REQUEST_CODE)
                    true
                } catch (e: ActivityNotFoundException) {
                    pendingFileChooserCallback = null
                    false
                }
            }
        }
        if (savedInstanceState == null || webView.restoreState(savedInstanceState) == null) {
            // The bare origin root, not "/index.html" -- confirmed via a real device's Chrome
            // DevTools remote inspection to be the actual root cause of a "Not Found" screen
            // seen on real hardware: loading "/index.html" gives location.pathname that exact
            // value, which a client-side router (expecting "/" as its home route) doesn't
            // match, rendering the *game's own* "Not Found" page -- nothing to do with this
            // engine's asset loading, which was already serving files correctly. Mirrors
            // App.swift's identical `game://content/` (no "/index.html") for the same reason.
            webView.loadUrl("https://appassets.androidplatform.net/")
        }
    }

    /** Escapes `value` for embedding as a double-quoted JavaScript string literal. */
    private fun jsStringLiteral(value: String): String =
        "\"" + value.replace("\\", "\\\\").replace("\"", "\\\"") + "\""

    /**
     * The bridge a game's own JS never calls directly -- only the `evaluateJavascript` snippet
     * this file injects for a blob: download does, to hand a Blob's content back out as a data
     * URL once it's been read on the page side (see the `setDownloadListener` block above).
     */
    inner class RovesFileBridge {
        @JavascriptInterface
        fun saveDataUrl(dataUrl: String, fileName: String) {
            this@MainActivity.saveDataUrl(dataUrl, fileName)
        }
    }

    /** Decodes a `data:` URL (as produced by `FileReader.readAsDataURL`) and writes it out. */
    private fun saveDataUrl(dataUrl: String, fileName: String) {
        runCatching {
            val commaIndex = dataUrl.indexOf(',')
            if (!dataUrl.startsWith("data:") || commaIndex == -1) return
            val meta = dataUrl.substring(5, commaIndex)
            val mimeType = meta.substringBefore(";").ifEmpty { "application/octet-stream" }
            val payload = dataUrl.substring(commaIndex + 1)
            val bytes = if (meta.endsWith(";base64")) {
                Base64.decode(payload, Base64.DEFAULT)
            } else {
                Uri.decode(payload).toByteArray(Charsets.UTF_8)
            }
            saveToDocuments(fileName, mimeType, bytes)
        }
    }

    /**
     * Writes into `Documents/<this game's own launcher label>/<fileName>` -- a real,
     * player-visible location (Files app, a USB/MTP file browser, ...), not an
     * app-private directory another app or the player themselves can't get to. Uses the
     * `MediaStore` `Documents`/`Files` collection, not raw external storage: this needs no
     * `WRITE_EXTERNAL_STORAGE` permission at all on API 29+ (this app's own `minSdk`) -- a
     * missing permission was never actually the cause of the import/export buttons doing
     * nothing (see this same session's real-device report); they were simply never wired up
     * to anything before this.
     */
    private fun saveToDocuments(fileName: String, mimeType: String, bytes: ByteArray) {
        runCatching {
            val appLabel = packageManager.getApplicationLabel(applicationInfo).toString()
            val values = ContentValues().apply {
                put(MediaStore.MediaColumns.DISPLAY_NAME, fileName)
                put(MediaStore.MediaColumns.MIME_TYPE, mimeType)
                put(MediaStore.MediaColumns.RELATIVE_PATH, "${Environment.DIRECTORY_DOCUMENTS}/$appLabel")
            }
            val uri = contentResolver.insert(MediaStore.Files.getContentUri("external"), values) ?: return
            contentResolver.openOutputStream(uri)?.use { it.write(bytes) }
        }
    }

    /**
     * The game always runs edge-to-edge, with the status and navigation bars hidden -- this
     * isn't limited to HTML5 `<video>`/Fullscreen API content (`onShowCustomView` above), which
     * is the only case the system bars were previously hidden for. `IMMERSIVE_STICKY` lets a
     * swipe from a screen edge reveal the bars temporarily (required for the user to ever get
     * them back at all, e.g. to check the clock or notifications) without permanently exiting
     * this mode -- they auto-hide again on the next interaction. Reapplied in
     * `onWindowFocusChanged` because Android clears these flags whenever the window loses and
     * regains focus (e.g. the notification shade, a system dialog, or switching apps and back).
     */
    private fun enterImmersiveMode() {
        window.decorView.systemUiVisibility = View.SYSTEM_UI_FLAG_LAYOUT_STABLE or
            View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION or View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or
            View.SYSTEM_UI_FLAG_FULLSCREEN or View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or
            View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) enterImmersiveMode()
    }

    private fun leaveFullscreen() {
        fullscreenView?.let { container.removeView(it) }
        fullscreenView = null
        webView.visibility = View.VISIBLE
        enterImmersiveMode()
        fullscreenCallback?.onCustomViewHidden()
        fullscreenCallback = null
    }

    @Deprecated("Activity back navigation")
    override fun onBackPressed() {
        if (fullscreenView != null) leaveFullscreen()
        else if (webView.canGoBack()) webView.goBack()
        else super.onBackPressed()
    }

    @Deprecated("Deprecated in Java")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != FILE_CHOOSER_REQUEST_CODE) return
        val results = if (resultCode == RESULT_OK && data != null) {
            WebChromeClient.FileChooserParams.parseResult(resultCode, data)
        } else {
            null
        }
        pendingFileChooserCallback?.onReceiveValue(results)
        pendingFileChooserCallback = null
    }

    override fun onSaveInstanceState(outState: Bundle) {
        webView.saveState(outState)
        super.onSaveInstanceState(outState)
    }
    override fun onPause() { webView.onPause(); super.onPause() }
    override fun onResume() { super.onResume(); webView.onResume() }
    override fun onDestroy() {
        destroyed = true
        leaveFullscreen()
        container.removeView(webView)
        webView.destroy()
        super.onDestroy()
    }

    private companion object {
        const val FILE_CHOOSER_REQUEST_CODE = 51426
    }
}
