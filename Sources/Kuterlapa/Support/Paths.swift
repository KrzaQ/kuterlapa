import Foundation

/// Where Kuterlapa keeps its files. Follows the XDG base directory layout,
/// as many command-line tools on macOS do:
///   config  ~/.config/kuterlapa         settings the user edits
///   data    ~/.local/share/kuterlapa    recordings, history
///   state   ~/.local/state/kuterlapa    sockets
enum Paths {
    static let home = FileManager.default.homeDirectoryForCurrentUser

    static let config = base("XDG_CONFIG_HOME", ".config")
    static let data = base("XDG_DATA_HOME", ".local/share")
    static let state = base("XDG_STATE_HOME", ".local/state")

    /// The core's socket: this app sends controller buttons there.
    static let coreSocket = state.appendingPathComponent("core.sock")
    /// This app's socket: the core asks it to type text and press keys.
    static let shellSocket = state.appendingPathComponent("shell.sock")

    private static func base(_ variable: String, _ fallback: String) -> URL {
        let env = ProcessInfo.processInfo.environment[variable] ?? ""
        let root = env.isEmpty ? home.appendingPathComponent(fallback) : URL(fileURLWithPath: env)
        return root.appendingPathComponent("kuterlapa")
    }

    static func ensureDirectories() {
        for dir in [config, data, state] {
            try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        }
    }
}
