import UIKit
import UniformTypeIdentifiers
import WebKit

@main
class AppDelegate: UIResponder, UIApplicationDelegate {
    var window: UIWindow?

    func application(_ application: UIApplication,
                     didFinishLaunchingWithOptions options: [UIApplication.LaunchOptionsKey: Any]?) -> Bool {
        let window = UIWindow(frame: UIScreen.main.bounds)
        window.rootViewController = GameViewController()
        window.makeKeyAndVisible()
        self.window = window
        return true
    }
}

/// Serves the bundled game under the virtual origin `game://content/` instead of a raw
/// `file://` path, mirroring the engine's own desktop/Android `game:` protocol handler (see
/// `ports/servoshell/protocols/game.rs`) and Android's `WebViewAssetLoader` origin in this
/// same mobile pivot. `WKWebView.loadFileURL` gives `location.pathname` the real on-disk
/// path and leaves root-relative references (`<script src="/assets/x.js">`, the default
/// virtually every bundler emits) resolving against the OS filesystem root -- the exact bug
/// this project already hit and fixed for `file://` on every other platform it ships. A real
/// origin fixes both: `pathname` is `/` at boot (so a client-side history router's root route
/// matches), and root-relative paths resolve against this handler's own `root` instead.
///
/// Deliberately a plain custom scheme, not `https`: `WKURLSchemeHandler` is per-*scheme*, not
/// per-host, so registering for `https` itself would intercept every real network request too
/// (fonts, CDNs, analytics) -- reimplementing an entire pass-through HTTP client is out of
/// scope for what's still an initial container. `game://` isn't a WebKit "secure context"
/// (no Service Workers, some newer APIs gated on that) -- an accepted, pre-existing tradeoff:
/// desktop's own `game://` (see `ports/servoshell/protocols/game.rs`'s doc comment) makes the
/// exact same call for the exact same reason.
final class GameSchemeHandler: NSObject, WKURLSchemeHandler {
    static let scheme = "game"
    static let host = "content"

    private let root: URL

    init(root: URL) {
        self.root = root
    }

    func webView(_ webView: WKWebView, start urlSchemeTask: WKURLSchemeTask) {
        guard let url = urlSchemeTask.request.url else {
            urlSchemeTask.didFailWithError(URLError(.badURL))
            return
        }
        // SPA fallback: any path with no matching file serves the entry document, the same
        // way `protocols/game.rs`'s `GameProtocolHandler::load` does -- a client-side router
        // navigating to e.g. `game://content/level/3` has no file at that path, but should
        // still get `index.html` (and let the router itself decide what to render), not a 404.
        var relativePath = url.path
        if relativePath.isEmpty { relativePath = "/" }
        var fileURL = root.appendingPathComponent(String(relativePath.dropFirst()))
        var isDirectory: ObjCBool = false
        let exists = FileManager.default.fileExists(atPath: fileURL.path, isDirectory: &isDirectory)
        if !exists || isDirectory.boolValue {
            fileURL = root.appendingPathComponent("index.html")
        }

        guard let data = try? Data(contentsOf: fileURL) else {
            respond(urlSchemeTask, url: url, statusCode: 404, data: Data("Not Found".utf8), mimeType: "text/plain")
            return
        }

        let mimeType = Self.mimeType(for: fileURL.pathExtension)
        // Range support: a `<video>`/`<audio>` element seeks by issuing a byte-range request,
        // which `WKURLSchemeTask` never synthesizes for you -- ignoring it entirely still
        // "works" for playback from the start, but silently breaks seeking/scrubbing.
        if let rangeHeader = urlSchemeTask.request.value(forHTTPHeaderField: "Range"),
           let (start, end) = Self.parseRange(rangeHeader, totalLength: data.count) {
            let slice = data.subdata(in: start..<(end + 1))
            let headers = [
                "Content-Range": "bytes \(start)-\(end)/\(data.count)",
                "Accept-Ranges": "bytes",
                "Content-Length": String(slice.count),
            ]
            respond(urlSchemeTask, url: url, statusCode: 206, data: slice, mimeType: mimeType, headers: headers)
            return
        }

        respond(urlSchemeTask, url: url, statusCode: 200, data: data, mimeType: mimeType,
                headers: ["Accept-Ranges": "bytes", "Content-Length": String(data.count)])
    }

    func webView(_ webView: WKWebView, stop urlSchemeTask: WKURLSchemeTask) {}

    private func respond(_ task: WKURLSchemeTask, url: URL, statusCode: Int, data: Data, mimeType: String, headers: [String: String] = [:]) {
        let response = HTTPURLResponse(url: url, statusCode: statusCode, httpVersion: "HTTP/1.1",
                                        headerFields: headers.merging(["Content-Type": mimeType]) { a, _ in a })!
        task.didReceive(response)
        task.didReceive(data)
        task.didFinish()
    }

    private static func parseRange(_ header: String, totalLength: Int) -> (Int, Int)? {
        guard header.hasPrefix("bytes="), totalLength > 0 else { return nil }
        let spec = header.dropFirst("bytes=".count)
        let parts = spec.split(separator: "-", maxSplits: 1, omittingEmptySubsequences: false)
        guard let start = Int(parts.first ?? "") else { return nil }
        let end = parts.count > 1 ? (Int(parts[1]) ?? totalLength - 1) : totalLength - 1
        guard start >= 0, end < totalLength, start <= end else { return nil }
        return (start, end)
    }

    private static let mimeTypes: [String: String] = [
        "html": "text/html", "htm": "text/html", "js": "text/javascript", "mjs": "text/javascript",
        "css": "text/css", "json": "application/json", "wasm": "application/wasm",
        "png": "image/png", "jpg": "image/jpeg", "jpeg": "image/jpeg", "gif": "image/gif",
        "svg": "image/svg+xml", "webp": "image/webp", "ico": "image/x-icon",
        "woff": "font/woff", "woff2": "font/woff2", "ttf": "font/ttf", "otf": "font/otf",
        "mp3": "audio/mpeg", "wav": "audio/wav", "ogg": "audio/ogg",
        "mp4": "video/mp4", "webm": "video/webm",
        "txt": "text/plain", "xml": "application/xml",
    ]

    private static func mimeType(for ext: String) -> String {
        mimeTypes[ext.lowercased()] ?? "application/octet-stream"
    }
}

/// Native branding shown while the game's own document loads, independent of the game's
/// assets and launcher icon -- the iOS equivalent of Android's `RovesSplashView.kt` in this
/// same mobile pivot (see that file's own doc comment). Black background, engine icon, Metal
/// Mania wordmark, animated loading bar; removed once the WKWebView navigation delegate
/// reports the initial document finished, with the same "stay at least 500ms" floor so a
/// near-instant load doesn't just flash.
final class RovesSplashView: UIView {
    private let barTrack = UIView()
    private let barHighlight = UIView()

    init() {
        super.init(frame: .zero)
        backgroundColor = .black
        isAccessibilityElement = true
        accessibilityLabel = "Roves"

        let icon = UIImageView(image: UIImage(contentsOfFile:
            Bundle.main.path(forResource: "servo_1024", ofType: "png", inDirectory: "roves-brand") ?? ""))
        icon.contentMode = .scaleAspectFit
        icon.translatesAutoresizingMaskIntoConstraints = false

        let label = UILabel()
        label.text = "Roves"
        label.textColor = .white
        if let fontURL = Bundle.main.url(forResource: "MetalMania-Regular", withExtension: "ttf", subdirectory: "roves-brand"),
           let fontData = try? Data(contentsOf: fontURL),
           let provider = CGDataProvider(data: fontData as CFData),
           let cgFont = CGFont(provider),
           let name = cgFont.postScriptName {
            var error: Unmanaged<CFError>?
            CTFontManagerRegisterGraphicsFont(cgFont, &error)
            label.font = UIFont(name: name as String, size: 32) ?? UIFont.systemFont(ofSize: 32)
        } else {
            label.font = UIFont.systemFont(ofSize: 32)
        }
        label.translatesAutoresizingMaskIntoConstraints = false

        barTrack.backgroundColor = .darkGray
        barTrack.translatesAutoresizingMaskIntoConstraints = false
        barHighlight.backgroundColor = .white
        barTrack.clipsToBounds = true
        barTrack.addSubview(barHighlight)

        let row = UIStackView(arrangedSubviews: [icon, label])
        row.axis = .horizontal
        row.spacing = 8
        row.alignment = .center
        row.translatesAutoresizingMaskIntoConstraints = false

        addSubview(row)
        addSubview(barTrack)
        NSLayoutConstraint.activate([
            icon.widthAnchor.constraint(equalToConstant: 48),
            icon.heightAnchor.constraint(equalToConstant: 48),
            row.centerXAnchor.constraint(equalTo: centerXAnchor),
            row.centerYAnchor.constraint(equalTo: centerYAnchor, constant: -16),
            barTrack.centerXAnchor.constraint(equalTo: centerXAnchor),
            barTrack.topAnchor.constraint(equalTo: row.bottomAnchor, constant: 24),
            barTrack.widthAnchor.constraint(equalToConstant: 160),
            barTrack.heightAnchor.constraint(equalToConstant: 3),
        ])
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        guard window != nil else { return }
        barHighlight.frame = CGRect(x: -40, y: 0, width: 40, height: 3)
        animateBar()
    }

    private func animateBar() {
        UIView.animate(withDuration: 1.1, delay: 0, options: [.curveLinear], animations: {
            self.barHighlight.frame.origin.x = self.barTrack.bounds.width
        }, completion: { [weak self] _ in
            guard let self, self.window != nil else { return }
            self.barHighlight.frame.origin.x = -40
            self.animateBar()
        })
    }
}

/// Two things, both injected at document start so they're in place before the game's own
/// script runs:
///
/// 1. Intercepts `<a download>` clicks on `blob:`/`data:` URLs (a game's save export, e.g.
///    `save.download()` writing a JSON save file) and routes them to `rovesSaveFile` below
///    instead of leaving WebKit to handle them -- WKWebView has no built-in download handling
///    for this exact case: `WKNavigationDelegate`'s `decidePolicyFor navigationResponse`/
///    `WKDownloadDelegate` only fire for a real top-level navigation to a downloadable
///    response, never for a JS-triggered anchor click that never navigates the page at all
///    (confirmed missing on a real device on Android, where the equivalent gap needed the same
///    kind of workaround -- see MainActivity.kt's own `setDownloadListener` comment; this is
///    the same fix, adapted to WKWebView's own constraints since there's no `DownloadListener`
///    equivalent that fires for blob: URLs here at all).
/// 2. Neutralizes the Fullscreen API (`Element.requestFullscreen`/`Document.exitFullscreen`)
///    into a harmless resolved no-op -- this app already always runs edge-to-edge (see
///    `prefersStatusBarHidden`/`prefersHomeIndicatorAutoHidden` below), so there's nothing for
///    a "toggle fullscreen" call to meaningfully do. `WKPreferences.elementFullscreenEnabled`
///    is never turned on in this file, so WebKit's own Fullscreen API support is already off
///    by default here -- this override exists for predictability (a resolved promise, not
///    whatever WebKit's own default-disabled rejection looks like) and to match
///    `MainActivity.kt`'s equivalent override, which *is* load-bearing there (see its own doc
///    comment: `onShowCustomView` visibly breaks the game's UI on Android without it), so a
///    game targeting both platforms sees the same behavior either way.
private let downloadInterceptScript = """
(function() {
  document.addEventListener('click', function(event) {
    var a = event.target && event.target.closest && event.target.closest('a[download]');
    if (!a) return;
    var href = a.getAttribute('href') || '';
    if (href.indexOf('blob:') !== 0 && href.indexOf('data:') !== 0) return;
    event.preventDefault();
    var fileName = a.getAttribute('download') || 'download';
    function post(dataUrl) {
      window.webkit.messageHandlers.rovesSaveFile.postMessage({ dataUrl: dataUrl, fileName: fileName });
    }
    if (href.indexOf('data:') === 0) { post(href); return; }
    fetch(href).then(function(r) { return r.blob(); }).then(function(blob) {
      var reader = new FileReader();
      reader.onloadend = function() { post(reader.result); };
      reader.readAsDataURL(blob);
    });
  }, true);

  var fullscreenNoop = function() { return Promise.resolve(); };
  if (window.Element && Element.prototype) { Element.prototype.requestFullscreen = fullscreenNoop; }
  if (window.Document && Document.prototype) { Document.prototype.exitFullscreen = fullscreenNoop; }
})();
"""

final class GameViewController: UIViewController, WKNavigationDelegate, WKUIDelegate,
    WKScriptMessageHandler, UIDocumentPickerDelegate {
    private var webView: WKWebView!
    private var splash: RovesSplashView!
    private var splashStarted: CFAbsoluteTime = 0
    private var startupPending = true
    /// Held between `onShowFileChooser`'s Android equivalent (`runOpenPanelWith` below) and
    /// the document picker actually returning -- a game's save import (an
    /// `<input type="file">` picker).
    private var filePickerCompletionHandler: (([URL]?) -> Void)?

    // The game always runs edge-to-edge: no status bar, and no home indicator on Face ID
    // devices (the bottom "bar" equivalent to Android's gesture nav bar) -- mirrors
    // MainActivity.kt's `enterImmersiveMode`. `prefersStatusBarHidden` alone is enough to hide
    // the status bar since `UIViewControllerBasedStatusBarAppearance` defaults to true; both
    // are read once and never change, so no `setNeedsStatusBarAppearanceUpdate`/
    // `setNeedsUpdateOfHomeIndicatorAutoHidden` call is needed.
    override var prefersStatusBarHidden: Bool { true }
    override var prefersHomeIndicatorAutoHidden: Bool { true }

    override func loadView() {
        let contentRoot = Bundle.main.resourceURL?.appendingPathComponent("www")
        let container = UIView()
        container.backgroundColor = .black
        view = container

        let configuration = WKWebViewConfiguration()
        configuration.allowsInlineMediaPlayback = true
        configuration.mediaTypesRequiringUserActionForPlayback = []
        configuration.websiteDataStore = .default()
        if let contentRoot {
            configuration.setURLSchemeHandler(GameSchemeHandler(root: contentRoot), forURLScheme: GameSchemeHandler.scheme)
        }
        let contentController = WKUserContentController()
        contentController.add(self, name: "rovesSaveFile")
        contentController.addUserScript(
            WKUserScript(source: downloadInterceptScript, injectionTime: .atDocumentStart, forMainFrameOnly: false)
        )
        configuration.userContentController = contentController
        webView = WKWebView(frame: .zero, configuration: configuration)
        webView.navigationDelegate = self
        webView.uiDelegate = self
        webView.scrollView.bounces = false
        webView.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(webView)
        NSLayoutConstraint.activate([
            webView.topAnchor.constraint(equalTo: container.topAnchor),
            webView.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            webView.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            webView.trailingAnchor.constraint(equalTo: container.trailingAnchor),
        ])

        splash = RovesSplashView()
        splashStarted = CFAbsoluteTimeGetCurrent()
        splash.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(splash)
        NSLayoutConstraint.activate([
            splash.topAnchor.constraint(equalTo: container.topAnchor),
            splash.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            splash.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            splash.trailingAnchor.constraint(equalTo: container.trailingAnchor),
        ])
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        guard let root = Bundle.main.resourceURL?.appendingPathComponent("www"),
              FileManager.default.fileExists(atPath: root.appendingPathComponent("index.html").path) else {
            let label = UILabel()
            label.text = "Missing bundled game: www/index.html"
            label.textColor = .white
            label.textAlignment = .center
            view = label
            return
        }
        webView.load(URLRequest(url: URL(string: "game://\(GameSchemeHandler.host)/")!))
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        guard startupPending else { return }
        let remaining = max(0, 0.5 - (CFAbsoluteTimeGetCurrent() - splashStarted))
        DispatchQueue.main.asyncAfter(deadline: .now() + remaining) { [weak self] in
            guard let self, self.startupPending else { return }
            self.startupPending = false
            UIView.animate(withDuration: 0.2, animations: { self.splash.alpha = 0 }) { _ in
                self.splash.removeFromSuperview()
            }
        }
    }

    func webView(_ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction,
                 decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
        guard let scheme = navigationAction.request.url?.scheme,
              [GameSchemeHandler.scheme, "https", "http", "about"].contains(scheme) else {
            decisionHandler(.cancel)
            return
        }
        decisionHandler(.allow)
    }

    // MARK: - WKUIDelegate: a game's save import (`<input type="file">`)

    // WKUIDelegate only gained this method on iOS 18.4 (confirmed against Apple's own
    // documentation -- file input support in WKWebView on iOS is a genuinely recent addition;
    // it has existed on macOS since 10.12, but never on iOS/iPadOS until 18.4) -- this
    // project's own deployment target is iOS 15.0, well below that, so this must be
    // explicitly gated. On an older iOS a file `<input>` simply stays exactly as inert as it
    // is without this method at all -- the same behavior as before this existed, not a
    // regression -- and starts working the moment the OS itself supports it.
    @available(iOS 18.4, *)
    func webView(_ webView: WKWebView, runOpenPanelWith parameters: WKOpenPanelParameters,
                 initiatedByFrame frame: WKFrameInfo, completionHandler: @escaping ([URL]?) -> Void) {
        filePickerCompletionHandler?(nil)
        filePickerCompletionHandler = completionHandler
        let picker = UIDocumentPickerViewController(forOpeningContentTypes: [.item])
        picker.allowsMultipleSelection = parameters.allowsMultipleSelection
        picker.delegate = self
        present(picker, animated: true)
    }

    // MARK: - UIDocumentPickerDelegate

    func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
        filePickerCompletionHandler?(urls)
        filePickerCompletionHandler = nil
    }

    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
        filePickerCompletionHandler?(nil)
        filePickerCompletionHandler = nil
    }

    // MARK: - WKScriptMessageHandler: a game's save export (`<a download>` on a blob:/data: URL)

    func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
        guard message.name == "rovesSaveFile",
              let body = message.body as? [String: Any],
              let dataUrl = body["dataUrl"] as? String,
              let fileName = body["fileName"] as? String else { return }
        saveDataUrl(dataUrl, fileName: fileName)
    }

    /// Decodes a `data:` URL (produced either directly by the injected script, for a `data:`
    /// href, or via `FileReader.readAsDataURL` for a `blob:` one) and writes it into this
    /// app's own `Documents/` folder -- already private to this app and player-visible via
    /// the Files app (with "Supports opening documents in place"/"Application supports
    /// iTunes file sharing" set, which a game's own Info.plist can opt into same as any other
    /// iOS app), unlike Android's shared, system-wide `Documents/<app name>/` this mirrors:
    /// iOS has no equivalent shared collection to disambiguate between apps in the first
    /// place, so there's no per-game subfolder to create here.
    private func saveDataUrl(_ dataUrl: String, fileName: String) {
        guard dataUrl.hasPrefix("data:"), let commaIndex = dataUrl.firstIndex(of: ",") else { return }
        let meta = dataUrl[dataUrl.index(dataUrl.startIndex, offsetBy: 5)..<commaIndex]
        let payload = String(dataUrl[dataUrl.index(after: commaIndex)...])
        let data: Data?
        if meta.contains(";base64") {
            data = Data(base64Encoded: payload)
        } else {
            data = payload.removingPercentEncoding?.data(using: .utf8)
        }
        guard let data,
              let documentsDir = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first
        else { return }
        try? data.write(to: documentsDir.appendingPathComponent(fileName))
    }
}
