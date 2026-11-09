import AppKit

/// The macOS shell of Kuterlapa. It does only what needs the system:
/// it reads controller buttons, types text and presses keys, and holds the
/// macOS permissions. The core (Rust, shared by all systems) decides what
/// each button does.
final class AppDelegate: NSObject, NSApplicationDelegate {
    private let core = CoreProcess()
    private lazy var gamepad = Gamepad { [weak self] name, pressed in self?.button(name, pressed) }
    private let shell = SocketServer(path: Paths.shellSocket.path, handler: ShellCommands.handle)
    private var statusMenu: StatusMenu?

    func applicationDidFinishLaunching(_ notification: Notification) {
        Paths.ensureDirectories()
        log("Kuterlapa started")
        ShellCommands.overlay = OverlayPanel()
        ShellCommands.chatView = ChatView()
        ShellCommands.sticks = Sticks(gamepad: gamepad)
        log("accessibility: \(Keystrokes.requestPermission())")
        shell.start()
        core.start()
        gamepad.start()
        log(LoginItem.apply())
        statusMenu = StatusMenu(
            status: { [weak self] in self?.statusLines() ?? [] },
            quit: { NSApp.terminate(nil) }
        )
    }

    func applicationWillTerminate(_ notification: Notification) {
        core.stop()
    }

    /// The core may type or press keys in answer, through this app's own
    /// socket, so the request must not wait on the main thread. A serial
    /// queue keeps each press before its release.
    private let toCore = DispatchQueue(label: "kuterlapa.buttons", qos: .userInteractive)

    private func button(_ name: String, _ pressed: Bool) {
        toCore.async { [core] in
            let request = "\(pressed ? "press" : "release") \(name)"
            let answer = core.send(request) ?? "no answer from core"
            if answer != "ok" { log("\(request): \(answer)") }
        }
    }

    private func statusLines() -> [String] {
        let pads = gamepad.connectedNames
        return [
            "Dostępność: " + (Keystrokes.allowed ? "tak" : "brak zgody"),
            "Rdzeń: " + (core.isRunning ? "działa" : "nie działa"),
            "Pad: " + (pads.isEmpty ? "brak" : pads.joined(separator: ", ")),
        ]
    }
}
