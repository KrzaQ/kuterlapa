import AppKit

/// Requests on this app's socket (~/.local/state/kuterlapa/shell.sock).
/// The core sends them; docs/protocol.md describes them.
///
///   type <text>    type the text into the active window
///   key <combo>    press a key combination, for example "key Return"
///   erase <n>      press Backspace n times
///   overlay <json> show the overlay with this state, or hide it
///   frontmost      the bundle identifier of the application in front
///   status         one line with the app state
///
/// The answer is "ok", a status line, or "error: ...".
enum ShellCommands {
    /// Set by the app delegate once the overlay exists.
    static var overlay: OverlayPanel?

    static func handle(_ line: String) -> String {
        let parts = line.split(separator: " ", maxSplits: 1, omittingEmptySubsequences: false)
        let verb = parts.first.map(String.init) ?? ""
        let argument = parts.count > 1 ? String(parts[1]) : ""
        switch verb {
        case "status":
            return "accessibility=\(Keystrokes.allowed)"
        case "frontmost":
            return NSWorkspace.shared.frontmostApplication?.bundleIdentifier ?? ""
        case "overlay":
            guard let data = argument.data(using: .utf8),
                  let state = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
            else { return "error: bad overlay state" }
            overlay?.update(state)
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
