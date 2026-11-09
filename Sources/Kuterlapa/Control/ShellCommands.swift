import AppKit

/// Requests on this app's socket (~/.local/state/kuterlapa/shell.sock).
/// The core sends them; docs/protocol.md describes them.
///
///   type <text>    type the text into the active window
///   key <combo>    press a key combination, for example "key Return"
///   erase <n>      press Backspace n times
///   overlay <json> show the overlay with this state, or hide it
///   overlay-scale <f> the size of the overlay and its text (1 is normal)
///   view <json>    show the large chat view with this state, or hide it
///   view-scroll <how>  scroll it: page-up, page-down, top, bottom, or pixels
///   view-work <json>   what the model does right now, below the last
///                      message (null: it has finished)
///   panel-windows  "overlay=<n> view=<n> overlay-place=<x,y,w,h>": window
///                  numbers and the overlay's place, for screenshots
///   sticks <json>  what the analog sticks do: {"leftstick": "scroll", ...}
///   mouse <what>   click, double-click or right-click at the pointer
///   frontmost      the bundle identifier of the application in front
///   window         the application in front and its window's title:
///                  "<bundle id>\t<name>\t<title>"
///   status         one line with the app state
///
/// For the model's computer tools (Computer.swift); coordinates are the
/// model's (screen points scaled to at most 1024 wide), from the top left:
///   screen                    screenshot; answers "<path> <width> <height>"
///   ui                        elements of the window in front; answers a path
///   press <n>                 press element n from the last ui list
///   focus-element <n>         put the keyboard focus in element n
///   fill <n> <text>           focus element n, replace its text with text
///   click <x> <y> [how]       click, double-click or right-click at a point
///   scroll <x> <y> <dx> <dy>  scroll at a point by pixels
///   apps                      running applications, "*" in front
///   activate <name>           bring an application to the front, or open it
///   windows                   all windows and their desktops, numbered
///   focus <n>                 go to window n from the last list
///   desktop <n|left|right>    go to a virtual desktop
///
/// The answer is "ok", a status line, or "error: ...".
enum ShellCommands {
    /// Set by the app delegate once they exist.
    static var overlay: OverlayPanel?
    static var sticks: Sticks?
    static var chatView: ChatView?

    static func handle(_ line: String) -> String {
        let parts = line.split(separator: " ", maxSplits: 1, omittingEmptySubsequences: false)
        let verb = parts.first.map(String.init) ?? ""
        let argument = parts.count > 1 ? String(parts[1]) : ""
        switch verb {
        case "status":
            return "accessibility=\(Keystrokes.allowed)"
        case "frontmost":
            return NSWorkspace.shared.frontmostApplication?.bundleIdentifier ?? ""
        case "window":
            return frontWindow()
        case "overlay":
            guard let data = argument.data(using: .utf8),
                  let state = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
            else { return "error: bad overlay state" }
            overlay?.update(state)
            return "ok"
        case "screen":
            return Computer.screenshot()
        case "apps":
            return Computer.apps()
        case "activate":
            return Computer.activate(argument)
        case "windows":
            return Desktops.list()
        case "view":
            guard let data = argument.data(using: .utf8),
                  let state = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
            else { return "error: bad view state" }
            chatView?.update(argument, visible: state["visible"] as? Bool == true)
            return "ok"
        case "panel-windows":
            return "overlay=\(overlay?.windowNumber ?? 0) view=\(chatView?.windowNumber ?? 0) "
                + "overlay-place=\(overlay?.place ?? "")"
        case "view-scroll":
            chatView?.scroll(argument)
            return "ok"
        case "view-work":
            chatView?.work(argument)
            return "ok"
        case "overlay-scale":
            guard let value = Double(argument) else { return "error: bad scale \(argument)" }
            overlay?.setScale(CGFloat(value))
            return "ok"
        case "sticks":
            guard let data = argument.data(using: .utf8),
                  let roles = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
            else { return "error: bad sticks settings" }
            sticks?.configure(roles)
            return "ok"
        default:
            break
        }
        guard Keystrokes.allowed else { return "error: no Accessibility permission" }
        switch verb {
        case "type":
            Keystrokes.type(argument)
            return "ok"
        case "key":
            guard let combo = KeyCombo(argument) else { return "error: bad key \(argument)" }
            Keystrokes.press(combo)
            return "ok"
        case "mouse":
            switch argument {
            case "click": Mouse.click()
            case "double-click": Mouse.click(count: 2)
            case "right-click": Mouse.click(.right)
            default: return "error: unknown mouse action \(argument)"
            }
            return "ok"
        case "ui":
            return Computer.ui()
        case "press":
            guard let number = Int(argument) else { return "error: bad element \(argument)" }
            return Computer.press(number)
        case "focus-element":
            guard let number = Int(argument) else { return "error: bad element \(argument)" }
            return Computer.focus(number)
        case "fill":
            let parts = argument.split(separator: " ", maxSplits: 1, omittingEmptySubsequences: false)
            guard let number = parts.first.flatMap({ Int($0) }) else { return "error: usage: fill <n> <text>" }
            return Computer.fill(number, parts.count > 1 ? String(parts[1]) : "")
        case "click":
            let words = argument.split(separator: " ").map(String.init)
            guard words.count >= 2, let x = Double(words[0]), let y = Double(words[1]) else {
                return "error: usage: click <x> <y> [click|double-click|right-click]"
            }
            return Computer.click(at: Computer.point(x, y), how: words.count > 2 ? words[2] : "click")
        case "scroll":
            let n = argument.split(separator: " ").compactMap { Double($0) }
            guard n.count == 4 else { return "error: usage: scroll <x> <y> <dx> <dy>" }
            return Computer.scroll(at: Computer.point(n[0], n[1]), dx: Int32(n[2]), dy: Int32(n[3]))
        case "focus":
            guard let number = Int(argument) else { return "error: bad window \(argument)" }
            return Desktops.focus(number)
        case "desktop":
            return Desktops.go(argument)
        case "erase":
            guard let count = Int(argument), count >= 0, let backspace = KeyCombo("Backspace") else {
                return "error: bad count \(argument)"
            }
            for _ in 0..<count {
                Keystrokes.press(backspace)
                usleep(1500)
            }
            return "ok"
        default:
            return "error: unknown request \(verb)"
        }
    }
}

/// The application in front and the title of its focused window, from the
/// accessibility tree (Chrome adds its profile to the title there).
private func frontWindow() -> String {
    guard let app = NSWorkspace.shared.frontmostApplication else { return "" }
    var title = ""
    let element = AXUIElementCreateApplication(app.processIdentifier)
    var window: CFTypeRef?
    if AXUIElementCopyAttributeValue(element, kAXFocusedWindowAttribute as CFString, &window) == .success,
       let window {
        var value: CFTypeRef?
        // swiftlint:disable:next force_cast
        AXUIElementCopyAttributeValue(window as! AXUIElement, kAXTitleAttribute as CFString, &value)
        title = value as? String ?? ""
    }
    let clean = { (s: String) in s.replacingOccurrences(of: "\t", with: " ").replacingOccurrences(of: "\n", with: " ") }
    return [app.bundleIdentifier ?? "", app.localizedName ?? "", title].map(clean).joined(separator: "\t")
}
