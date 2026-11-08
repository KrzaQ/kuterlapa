import Foundation

/// Requests on this app's socket (~/.local/state/kuterlapa/shell.sock).
/// The core sends them; docs/protocol.md describes them.
///
///   type <text>    type the text into the active window
///   key <combo>    press a key combination, for example "key Return"
///   erase <n>      press Backspace n times
///   status         one line with the app state
///
/// The answer is "ok", a status line, or "error: ...".
enum ShellCommands {
    static func handle(_ line: String) -> String {
        let parts = line.split(separator: " ", maxSplits: 1, omittingEmptySubsequences: false)
        let verb = parts.first.map(String.init) ?? ""
        let argument = parts.count > 1 ? String(parts[1]) : ""
        if verb == "status" {
            return "accessibility=\(Keystrokes.allowed)"
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
