package org.servo.servoshell

import android.app.Activity
import android.graphics.Color
import android.os.Bundle
import android.os.SystemClock
import android.view.View
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import androidx.webkit.WebViewAssetLoader

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
        val assets = WebViewAssetLoader.AssetsPathHandler(this)
        val loader = WebViewAssetLoader.Builder()
            // Serve the game at the origin root so /images and /audio also work.
            .addPathHandler("/") { path -> assets.handle("www/$path") }
            .build()
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

            override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? {
                val response = loader.shouldInterceptRequest(request.url)
                if (request.url.host == WebViewAssetLoader.DEFAULT_DOMAIN && response == null) {
                    return WebResourceResponse("text/plain", "UTF-8", 404, "Not Found", emptyMap(), null)
                }
                return response
            }
            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                return request.url.scheme !in listOf("https", "http")
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
                window.decorView.systemUiVisibility = View.SYSTEM_UI_FLAG_FULLSCREEN or
                    View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
            }
            override fun onHideCustomView() = leaveFullscreen()
        }
        if (savedInstanceState == null || webView.restoreState(savedInstanceState) == null) {
            webView.loadUrl("https://appassets.androidplatform.net/index.html")
        }
    }

    private fun leaveFullscreen() {
        fullscreenView?.let { container.removeView(it) }
        fullscreenView = null
        webView.visibility = View.VISIBLE
        window.decorView.systemUiVisibility = View.SYSTEM_UI_FLAG_VISIBLE
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
