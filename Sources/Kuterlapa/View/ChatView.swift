import AppKit
import WebKit

/// The large chat view: the conversation of the current session, drawn by
/// a web page (view/ in the app's Resources) with Markdown, formulas and
/// highlighted code.
///
/// Like the overlay, it never takes the keyboard focus: the core drives it
/// with the controller (scrolling, other conversations), so the app below
/// keeps the focus and nothing has to be given back when it closes.
final class ChatView: NSObject, WKNavigationDelegate {
    private let panel: NSPanel
    private let web: WKWebView
    private var loaded = false
    private var pending: String?

    var isVisible: Bool { panel.isVisible }

    override init() {
        panel = NSPanel(contentRect: NSRect(x: 0, y: 0, width: 1000, height: 700),
                        styleMask: [.nonactivatingPanel, .borderless],
                        backing: .buffered, defer: true)
        panel.level = .floating
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle]
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.hidesOnDeactivate = false

        let configuration = WKWebViewConfiguration()
        configuration.preferences.setValue(true, forKey: "allowFileAccessFromFileURLs")
        web = WKWebView(frame: .zero, configuration: configuration)
        web.setValue(false, forKey: "drawsBackground")
        web.wantsLayer = true
        web.layer?.cornerRadius = 14
        web.layer?.masksToBounds = true
        super.init()
        web.navigationDelegate = self
        panel.contentView = web
        if let page = Bundle.main.url(forResource: "index", withExtension: "html", subdirectory: "view") {
            web.loadFileURL(page, allowingReadAccessTo: page.deletingLastPathComponent())
        } else {
            NSLog("chat view: view/index.html is missing from the app")
        }
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        loaded = true
        if let state = pending {
            pending = nil
            render(state)
        }
    }

    /// Shows the view with this state (JSON from the core), or hides it
    /// ({"visible": false}).
    func update(_ json: String, visible: Bool) {
        guard visible else {
            panel.orderOut(nil)
            return
        }
        place()
        if loaded { render(json) } else { pending = json }
        panel.orderFrontRegardless()
    }

    /// "page-up", "page-down", "top", "bottom" or pixels (positive: down).
    func scroll(_ how: String) {
        guard loaded, isVisible else { return }
        let argument = Double(how).map { String($0) } ?? "\"\(how)\""
        web.evaluateJavaScript("window.kuterlapa.scroll(\(argument))")
    }

    private func render(_ json: String) {
        web.evaluateJavaScript("window.kuterlapa.render(\(json))") { _, error in
            if let error { NSLog("chat view: \(error)") }
        }
    }

    /// About 70% of the screen, in the middle.
    private func place() {
        let screen = NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let width = min(screen.width * 0.72, 1300), height = screen.height * 0.82
        panel.setFrame(NSRect(x: screen.midX - width / 2, y: screen.midY - height / 2,
                              width: width, height: height), display: true)
    }
}
