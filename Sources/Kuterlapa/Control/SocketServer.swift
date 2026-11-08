import Foundation

/// A Unix socket server for one-line requests. Each connection sends one
/// line and gets one line back. The handler runs on the main thread.
final class SocketServer {
    private let path: String
    private let handler: (String) -> String

    init(path: String, handler: @escaping (String) -> String) {
        self.path = path
        self.handler = handler
    }

    func start() {
        unlink(path)
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { return log("socket() failed") }
        var address = sockaddr_un()
        address.sun_family = sa_family_t(AF_UNIX)
        let capacity = MemoryLayout.size(ofValue: address.sun_path)
        guard path.utf8.count < capacity else { return log("socket path too long: \(path)") }
        _ = withUnsafeMutableBytes(of: &address.sun_path) { buffer in
            path.utf8CString.withUnsafeBytes { memcpy(buffer.baseAddress!, $0.baseAddress!, $0.count) }
        }
        let bound = withUnsafePointer(to: &address) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                bind(fd, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        guard bound == 0, listen(fd, 8) == 0 else { return log("cannot listen on \(path)") }
        chmod(path, 0o600)
        log("listening on \(path)")
        Thread.detachNewThread { [handler] in
            while true {
                let client = accept(fd, nil, nil)
                if client < 0 { continue }
                let line = Self.readLine(client)
                var reply = ""
                DispatchQueue.main.sync { reply = handler(line) }
                reply += "\n"
                _ = reply.withCString { write(client, $0, strlen($0)) }
                close(client)
            }
        }
    }

    private static func readLine(_ fd: Int32) -> String {
        var data = Data()
        var buffer = [UInt8](repeating: 0, count: 4096)
        while true {
            let count = read(fd, &buffer, buffer.count)
            if count <= 0 { break }
            data.append(buffer, count: count)
            if buffer[..<count].contains(10) { break }
        }
        var line = String(decoding: data, as: UTF8.self)
        if line.hasSuffix("\n") { line.removeLast() }
        return line
    }
}

/// A client for one-line Unix socket requests, used to talk to the
/// speech helper.
enum SocketClient {
    static func request(_ path: String, _ line: String) -> String? {
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { return nil }
        defer { close(fd) }
        var address = sockaddr_un()
        address.sun_family = sa_family_t(AF_UNIX)
        _ = withUnsafeMutableBytes(of: &address.sun_path) { buffer in
            path.utf8CString.withUnsafeBytes {
                memcpy(buffer.baseAddress!, $0.baseAddress!, min(buffer.count - 1, $0.count))
            }
        }
        let connected = withUnsafePointer(to: &address) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                connect(fd, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        guard connected == 0 else { return nil }
        let message = line + "\n"
        _ = message.withCString { write(fd, $0, strlen($0)) }
        var buffer = [UInt8](repeating: 0, count: 1024)
        let count = read(fd, &buffer, buffer.count)
        guard count > 0 else { return "" }
        return String(decoding: buffer[..<count], as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
