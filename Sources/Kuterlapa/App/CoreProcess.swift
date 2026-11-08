import Foundation

/// Runs the core (`kuterlapa daemon`, a Rust program inside this app
/// bundle) as a child process and starts it again if it stops.
///
/// The core and its speech provider are children of this app, so they use
/// the app's macOS permissions (the microphone, for example). The core's
/// stdin is a pipe: when this app exits, the pipe closes and the core exits.
final class CoreProcess {
    private let executable: URL
    private var process: Process?
    private var input: Pipe?
    private var stopping = false

    init() {
        executable = Bundle.main.bundleURL.appendingPathComponent("Contents/Helpers/kuterlapa")
    }

    var isRunning: Bool { process?.isRunning ?? false }

    func start() {
        guard FileManager.default.isExecutableFile(atPath: executable.path) else {
            return log("core not found: \(executable.path)")
        }
        launch()
    }

    func stop() {
        stopping = true
        process?.terminate()
    }

    /// Sends one request to the core and returns the answer.
    func send(_ request: String) -> String? {
        SocketClient.request(Paths.coreSocket.path, request)
    }

    private func launch() {
        let p = Process()
        let pipe = Pipe()
        p.executableURL = executable
        p.arguments = ["daemon"]
        p.standardInput = pipe
        p.terminationHandler = { [weak self] finished in
            log("core exited with status \(finished.terminationStatus)")
            DispatchQueue.main.asyncAfter(deadline: .now() + 3) {
                guard let self, !self.stopping else { return }
                self.launch()
            }
        }
        do {
            try p.run()
            process = p
            input = pipe
            log("core started (pid \(p.processIdentifier))")
        } catch {
            log("cannot start core: \(error)")
        }
    }
}
