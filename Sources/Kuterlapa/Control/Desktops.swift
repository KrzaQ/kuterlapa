import AppKit
import ApplicationServices

// macOS has no public interface for virtual desktops (Spaces). These
// private CoreGraphics calls have been stable for many years and are used
// by window managers such as yabai and Amethyst. They only read; moving
// between desktops uses the normal ctrl+arrow keys.
private typealias CGSConnectionID = Int32
@_silgen_name("CGSMainConnectionID") private func CGSMainConnectionID() -> CGSConnectionID
@_silgen_name("CGSCopyManagedDisplaySpaces") private func CGSCopyManagedDisplaySpaces(_ cid: CGSConnectionID) -> CFArray
@_silgen_name("CGSCopySpacesForWindows")
private func CGSCopySpacesForWindows(_ cid: CGSConnectionID, _ mask: Int32, _ windows: CFArray) -> CFArray

/// Windows of all applications and the virtual desktops they are on; for
/// Chrome windows also the profile.
/// Desktops are numbered from 1 on the main display, in the order of
/// Mission Control; a full-screen app takes a desktop of its own.
enum Desktops {
    private struct Window {
        let id: CGWindowID
        let pid: pid_t
        let app: String
        let title: String
        let desktop: Int?
    }

    /// Windows from the last "windows" request, by number.
    private static var listed: [Int: Window] = [:]

    /// The desktops of the main display: space ids in order, and the
    /// number of the current one.
    private static func desktops() -> (ids: [Int], names: [String], current: Int) {
        let displays = CGSCopyManagedDisplaySpaces(CGSMainConnectionID()) as? [[String: Any]] ?? []
        guard let main = displays.first else { return ([], [], 0) }
        let spaces = main["Spaces"] as? [[String: Any]] ?? []
        let ids = spaces.map { $0["ManagedSpaceID"] as? Int ?? 0 }
        let names = spaces.map { space -> String in
            // Type 4 is a full-screen app.
            (space["type"] as? Int) == 4 ? "full screen" : "desktop"
        }
        let currentID = (main["Current Space"] as? [String: Any])?["ManagedSpaceID"] as? Int ?? 0
        let current = (ids.firstIndex(of: currentID) ?? 0) + 1
        return (ids, names, current)
    }

    private static func windows() -> [Window] {
        let (ids, _, current) = desktops()
        let info = CGWindowListCopyWindowInfo([.optionAll, .excludeDesktopElements], kCGNullWindowID)
            as? [[String: Any]] ?? []
        let regular = Set(NSWorkspace.shared.runningApplications
            .filter { $0.activationPolicy == .regular }.map(\.processIdentifier))
        var result: [Window] = []
        for w in info {
            guard (w[kCGWindowLayer as String] as? Int) == 0,
                  let pid = w[kCGWindowOwnerPID as String] as? pid_t, regular.contains(pid),
                  let id = w[kCGWindowNumber as String] as? CGWindowID,
                  let title = w[kCGWindowName as String] as? String, !title.isEmpty,
                  let bounds = w[kCGWindowBounds as String] as? [String: Double],
                  (bounds["Width"] ?? 0) > 50, (bounds["Height"] ?? 0) > 50
            else { continue }
            let spaces = CGSCopySpacesForWindows(CGSMainConnectionID(), 7, [id] as CFArray) as? [Int] ?? []
            let desktop = spaces.compactMap { ids.firstIndex(of: $0) }.first.map { $0 + 1 }
            // On the current desktop, a window that is not on screen is
            // hidden (a dialog kept for later, for example).
            let onScreen = w[kCGWindowIsOnscreen as String] as? Bool ?? false
            if desktop == current && !onScreen { continue }
            let app = w[kCGWindowOwnerName as String] as? String ?? "?"
            result.append(Window(id: id, pid: pid, app: app, title: title, desktop: desktop))
        }
        return result.sorted { ($0.desktop ?? 99, $0.app) < ($1.desktop ?? 99, $1.app) }
    }

    /// One line for the socket: the desktops, then the windows, joined
    /// with " | ".
    static func list() -> String {
        let (ids, names, current) = desktops()
        var lines = ["Desktops: \(ids.count), current: \(current)"
            + names.enumerated().filter { $0.element != "desktop" }.map { ", \($0.offset + 1): \($0.element)" }.joined()]
        listed = [:]
        let profiles = chromeProfiles()
        for (n, window) in windows().enumerated() {
            listed[n + 1] = window
            var place = window.desktop.map { "desktop \($0)" } ?? "minimized or other display"
            if window.app == "Google Chrome", let profile = chromeProfile(window, among: profiles) {
                place += ", profile \(profile)"
            }
            lines.append("[\(n + 1)] \(window.app): \"\(window.title.prefix(100))\" (\(place))")
        }
        return lines.joined(separator: " | ")
    }

    // MARK: Chrome profiles

    /// The names of Chrome's profiles, from its Local State file.
    private static func chromeProfiles() -> Set<String> {
        let file = Paths.home.appendingPathComponent("Library/Application Support/Google/Chrome/Local State")
        guard let data = try? Data(contentsOf: file),
              let state = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let cache = (state["profile"] as? [String: Any])?["info_cache"] as? [String: Any]
        else { return [] }
        return Set(cache.values.compactMap { ($0 as? [String: Any])?["name"] as? String })
    }

    /// Chrome names its accessibility windows "<page> - Google Chrome -
    /// <profile>" when it has more than one profile.
    private static func chromeProfile(_ window: Window, among profiles: Set<String>) -> String? {
        guard let element = axWindow(window), let title: String = value(element, kAXTitleAttribute),
              let range = title.range(of: " - Google Chrome - ", options: .backwards)
        else { return nil }
        let profile = String(title[range.upperBound...])
        return profiles.isEmpty || profiles.contains(profile) ? profile : nil
    }

    private static func axWindow(_ window: Window) -> AXUIElement? {
        let app = AXUIElementCreateApplication(window.pid)
        // Chromium builds its tree only for assistive technology that asks.
        AXUIElementSetAttributeValue(app, "AXManualAccessibility" as CFString, kCFBooleanTrue)
        let all: [AXUIElement] = value(app, kAXWindowsAttribute) ?? []
        // The accessibility title may be longer (Chrome adds its name and
        // the profile), so it only has to start with the window's title.
        return all.first { (value($0, kAXTitleAttribute) as String?)?.hasPrefix(window.title) == true }
    }

    private static func value<T>(_ element: AXUIElement, _ name: String) -> T? {
        var result: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, name as CFString, &result) == .success else { return nil }
        return result as? T
    }

    /// Goes to a desktop: a number, "left" or "right".
    static func go(_ target: String) -> String {
        let (ids, _, current) = desktops()
        let wanted: Int
        switch target {
        case "left": wanted = current - 1
        case "right": wanted = current + 1
        default:
            guard let n = Int(target) else { return "error: bad desktop \(target)" }
            wanted = n
        }
        guard wanted >= 1, wanted <= ids.count else { return "error: no desktop \(wanted) (there are \(ids.count))" }
        return step(from: current, to: wanted)
    }

    private static func step(from: Int, to: Int) -> String {
        guard from != to else { return "ok" }
        guard let combo = KeyCombo(to > from ? "ctrl+Right" : "ctrl+Left") else { return "error: no key" }
        for _ in 0..<abs(to - from) {
            Keystrokes.press(combo)
            usleep(400_000)  // the slide animation
        }
        return "ok"
    }

    /// Brings window `number` from the last list to the front: goes to its
    /// desktop, then raises it.
    static func focus(_ number: Int) -> String {
        guard let window = listed[number] else { return "error: no window \(number); ask for windows first" }
        if let desktop = window.desktop {
            let (_, _, current) = desktops()
            let moved = step(from: current, to: desktop)
            if moved != "ok" { return moved }
            usleep(200_000)
        }
        if let element = axWindow(window) {
            AXUIElementPerformAction(element, kAXRaiseAction as CFString)
            AXUIElementSetAttributeValue(element, kAXMainAttribute as CFString, kCFBooleanTrue)
        }
        NSRunningApplication(processIdentifier: window.pid)?.activate()
        return "ok"
    }
}
