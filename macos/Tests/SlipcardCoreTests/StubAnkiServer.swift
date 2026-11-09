import Foundation

/// A tiny HTTP server on 127.0.0.1 standing in for AnkiConnect: it answers
/// each action with a canned body (recorded from a real AnkiConnect) and
/// records what it was asked. `.silent` accepts connections and never
/// answers, which is what a hung Anki looks like.
final class StubAnkiServer: @unchecked Sendable {
    enum Behaviour { case respond, silent, status(Int) }

    let port: UInt16
    var url: URL { URL(string: "http://127.0.0.1:\(port)")! }

    private let fd: Int32
    private let behaviour: Behaviour
    private let replies: [String: Data]
    private let lock = NSLock()
    private var _requests: [[String: Any]] = []
    private var _authorizations: [String?] = []

    /// The JSON bodies received, in order.
    var requests: [[String: Any]] { lock.withLock { _requests } }
    /// The Authorization header of each request (nil when absent), in order.
    var authorizations: [String?] { lock.withLock { _authorizations } }
    var actions: [String] { requests.compactMap { $0["action"] as? String } }

    init(replies: [String: Data] = [:], behaviour: Behaviour = .respond) throws {
        self.replies = replies
        self.behaviour = behaviour
        let sock = socket(AF_INET, SOCK_STREAM, 0)
        var yes: Int32 = 1
        setsockopt(sock, SOL_SOCKET, SO_REUSEADDR, &yes, socklen_t(MemoryLayout<Int32>.size))
        var addr = sockaddr_in()
        addr.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
        addr.sin_family = sa_family_t(AF_INET)
        addr.sin_port = 0
        addr.sin_addr.s_addr = inet_addr("127.0.0.1")
        let bound = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                bind(sock, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        guard bound == 0, listen(sock, 16) == 0 else {
            close(sock)
            throw POSIXError(.EADDRINUSE)
        }
        var len = socklen_t(MemoryLayout<sockaddr_in>.size)
        _ = withUnsafeMutablePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { getsockname(sock, $0, &len) }
        }
        fd = sock
        port = UInt16(bigEndian: addr.sin_port)
        if case .silent = behaviour { return }  // the kernel accepts; nobody answers
        Thread.detachNewThread { [self] in acceptLoop() }
    }

    deinit {
        shutdown(fd, SHUT_RDWR)
        close(fd)
    }

    /// A body as AnkiConnect sends it.
    static func reply(result: Any?, error: String? = nil) -> Data {
        try! JSONSerialization.data(withJSONObject: ["result": result ?? NSNull(), "error": error ?? NSNull()])
    }

    private func acceptLoop() {
        while true {
            let client = accept(fd, nil, nil)
            if client < 0 { return }
            handle(client)
            close(client)
        }
    }

    private func handle(_ client: Int32) {
        var data = Data()
        var buffer = [UInt8](repeating: 0, count: 4096)
        // Read the headers, then Content-Length bytes of body.
        while true {
            if let end = data.range(of: Data("\r\n\r\n".utf8)) {
                let head = String(decoding: data[..<end.lowerBound], as: UTF8.self)
                let length = head.split(separator: "\r\n")
                    .first { $0.lowercased().hasPrefix("content-length:") }
                    .flatMap { Int($0.split(separator: ":")[1].trimmingCharacters(in: .whitespaces)) } ?? 0
                if data.count - end.upperBound >= length {
                    let body = data[end.upperBound..<(end.upperBound + length)]
                    let authorization = head.split(separator: "\r\n")
                        .first { $0.lowercased().hasPrefix("authorization:") }
                        .map { String($0.dropFirst("authorization:".count)).trimmingCharacters(in: .whitespaces) }
                    lock.withLock { _authorizations.append(authorization) }
                    respond(client, to: body)
                    return
                }
            }
            let n = read(client, &buffer, buffer.count)
            if n <= 0 { return }
            data.append(buffer, count: n)
        }
    }

    private func respond(_ client: Int32, to body: Data) {
        let request = (try? JSONSerialization.jsonObject(with: body)) as? [String: Any] ?? [:]
        lock.withLock { _requests.append(request) }
        var status = 200
        var payload = Data()
        switch behaviour {
        case .status(let code): status = code
        case .respond, .silent:
            let action = request["action"] as? String ?? ""
            payload = replies[action] ?? Self.reply(result: nil, error: "unsupported action")
        }
        let head = "HTTP/1.1 \(status) X\r\nContent-Type: application/json\r\n"
            + "Content-Length: \(payload.count)\r\nConnection: close\r\n\r\n"
        let out = Data(head.utf8) + payload
        _ = out.withUnsafeBytes { write(client, $0.baseAddress, out.count) }
    }
}
