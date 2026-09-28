import Foundation

/// Collects the two pipe outputs from two threads.
private final class Box: @unchecked Sendable {
    private let lock = NSLock()
    private var out = Data(), err = Data()
    func set(out d: Data) { lock.lock(); out = d; lock.unlock() }
    func set(err d: Data) { lock.lock(); err = d; lock.unlock() }
    func get() -> (Data, Data) { lock.lock(); defer { lock.unlock() }; return (out, err) }
}

/// Runs the lobo CLI. The app never talks to RunPod, Vast or the config file itself: this is the only door.
final class CLI {
    let binary: URL?
    /// Extra args on every call: `--config <path>` when LOBO_APP_CONFIG is set (tests, dev).
    let base: [String]

    init(binary: URL? = CLI.locate(), configOverride: String? = ProcessInfo.processInfo.environment["LOBO_APP_CONFIG"]) {
        self.binary = binary
        self.base = configOverride.map { ["--config", $0] } ?? []
    }

    /// Bundled binary first (same commit as the app), then the usual install places.
    static func locate() -> URL? {
        var candidates: [URL] = []
        if let r = Bundle.main.resourceURL { candidates.append(r.appendingPathComponent("lobo")) }
        let home = FileManager.default.homeDirectoryForCurrentUser
        candidates += [home.appendingPathComponent(".local/bin/lobo"),
                       URL(fileURLWithPath: "/opt/homebrew/bin/lobo"),
                       URL(fileURLWithPath: "/usr/local/bin/lobo")]
        return candidates.first { FileManager.default.isExecutableFile(atPath: $0.path) }
    }

    struct Result {
        var out: Data
        var err: String
        var code: Int32
        var ok: Bool { code == 0 }
        /// The CLI prints "error: …" on failure; keep the last such line.
        var message: String {
            let lines = err.split(separator: "\n").map(String.init)
            return lines.last(where: { $0.hasPrefix("error:") }).map { String($0.dropFirst(7)) } ?? lines.last ?? "exit \(code)"
        }
    }

    enum Failure: Error, LocalizedError {
        case missingBinary
        var errorDescription: String? { "lobo CLI not found (run `make install` or rebuild lobocode.app)" }
    }

    private func process(_ args: [String]) throws -> Process {
        guard let binary else { throw Failure.missingBinary }
        let p = Process()
        p.executableURL = binary
        p.arguments = args + base
        p.currentDirectoryURL = FileManager.default.temporaryDirectory
        return p
    }

    /// Runs to completion and returns stdout, stderr and the exit code. `stdin` feeds the child
    /// (used for secrets: never put them in argv, `ps` shows every process's arguments).
    func run(_ args: [String], stdin: Data? = nil) async throws -> Result {
        let p = try process(args)
        let out = Pipe(), err = Pipe()
        p.standardOutput = out
        p.standardError = err
        let input = Pipe()
        p.standardInput = input
        return try await withCheckedThrowingContinuation { cont in
            // Drain both pipes on background threads: a full pipe would block the child forever.
            let box = Box()
            let group = DispatchGroup()
            group.enter(); group.enter()
            DispatchQueue.global().async { box.set(out: out.fileHandleForReading.readDataToEndOfFile()); group.leave() }
            DispatchQueue.global().async { box.set(err: err.fileHandleForReading.readDataToEndOfFile()); group.leave() }
            p.terminationHandler = { proc in
                group.notify(queue: .global()) {
                    let (o, e) = box.get()
                    cont.resume(returning: Result(out: o, err: String(decoding: e, as: UTF8.self), code: proc.terminationStatus))
                }
            }
            do {
                try p.run()
                if let stdin { input.fileHandleForWriting.write(stdin) }
                try? input.fileHandleForWriting.close()
            } catch { cont.resume(throwing: error) }
        }
    }

    /// Starts a long-running command and calls onLine for every stdout line and onExit once at the end
    /// (with stderr). Both callbacks arrive on the main queue.
    @discardableResult
    func stream(_ args: [String], onLine: @escaping (Data) -> Void, onExit: @escaping (Int32, String) -> Void) throws -> Process {
        let p = try process(args)
        let out = Pipe(), err = Pipe()
        p.standardOutput = out
        p.standardError = err
        var buf = Data()
        var errText = Data()
        let lock = NSLock()
        out.fileHandleForReading.readabilityHandler = { h in
            let chunk = h.availableData
            if chunk.isEmpty { h.readabilityHandler = nil; return }
            lock.lock()
            buf.append(chunk)
            var lines: [Data] = []
            while let nl = buf.firstIndex(of: 0x0A) {
                lines.append(buf[buf.startIndex..<nl])
                buf.removeSubrange(buf.startIndex...nl)
            }
            lock.unlock()
            for l in lines where !l.isEmpty { DispatchQueue.main.async { onLine(Data(l)) } }
        }
        err.fileHandleForReading.readabilityHandler = { h in
            let chunk = h.availableData
            if chunk.isEmpty { h.readabilityHandler = nil; return }
            lock.lock(); errText.append(chunk); lock.unlock()
        }
        p.terminationHandler = { proc in
            // Let the readability handlers flush the last bytes first.
            DispatchQueue.global().asyncAfter(deadline: .now() + 0.3) {
                lock.lock()
                let e = String(decoding: errText, as: UTF8.self)
                lock.unlock()
                DispatchQueue.main.async { onExit(proc.terminationStatus, e) }
            }
        }
        try p.run()
        return p
    }
}
