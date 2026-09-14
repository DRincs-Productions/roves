package org.servo.servoshell

import android.app.Activity
import android.graphics.Color
import android.net.Uri
import android.os.Bundle
import android.os.SystemClock
import android.view.View
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
                if (primary.data != null) primary else assets.handle("www/index.html")
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
}
