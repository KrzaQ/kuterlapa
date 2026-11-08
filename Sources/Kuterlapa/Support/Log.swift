import Foundation

/// Writes one line with a timestamp to standard output.
/// `make run` sends standard output to the log file.
func log(_ message: String) {
    let stamp = ISO8601DateFormatter().string(from: Date())
    print("\(stamp) \(message)")
    fflush(stdout)
}
