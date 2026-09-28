import AppKit
import Foundation
import UserNotifications

enum Phase: Equatable {
    case loading      // first status not back yet
    case noConfig     // no config file / keys missing
    case off
    case booting
    case ready
    case stopping
    case failed(String)

    var word: String {
        switch self {
        case .loading: return "SCAN"
        case .noConfig: return "SETUP"
        case .off: return "OFF"
        case .booting: return "BOOT"
        case .ready: return "RUN"
        case .stopping: return "STOP"
        case .failed: return "FAIL"
        }
    }
}

/// Boot steps shown as a log, in order. `agent` = the pod agent stages that map onto a step.
enum Step: String, CaseIterable, Identifiable {
    case rent, container, tunnel, gpu, download, load, ready
    var id: String { rawValue }

    static func from(upPhase p: String) -> Step? {
        switch p {
        case "create": return .rent
        case "image", "boot": return .container
        case "tunnel": return .tunnel
        case "gpu": return .gpu
        case "download", "verify": return .download
        case "load": return .load
        case "ready": return .ready
        default: return nil
        }
    }
}

@MainActor
final class Store: ObservableObject {
    @Published var phase: Phase = .loading
    @Published var snap: Snapshot?
    @Published var config: ConfigShow?
    @Published var download: Download?
    /// When each boot step was first seen, relative to bootStart.
    @Published var stepAt: [Step: TimeInterval] = [:]
    @Published var bootStart: Date?
    @Published var lastDetail: String = ""
    @Published var warning: String?        // status poll failed; last state kept
    @Published var logTail: [String] = []  // for the fail card
    @Published var provider: String = "runpod"
    @Published var model: String = "q8"
    @Published var readyURL: String?
    @Published var panelOpen = false
    @Published var now = Date()

    /// The app's one store (one poller). Previews and tests make their own with cli: nil.
    static let shared = Store(cli: CLI())

    let cli: CLI?
    private var up: Process?
    private var userStopped = false
    private var pollTask: Task<Void, Never>?
    private var clock: Timer?

    /// cli == nil: a static preview store (render mode, tests).
    init(cli: CLI?) {
        self.cli = cli
        guard cli != nil else { return }
        clock = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.now = Date() }
        }
        pollTask = Task { [weak self] in
            await self?.loadConfig()
            while !Task.isCancelled {
                await self?.refresh()
                let wait = self?.pollInterval ?? 30
                try? await Task.sleep(nanoseconds: UInt64(wait * 1e9))
            }
        }
        requestNotifications()
    }

    var pollInterval: Double {
        switch phase {
        case .booting, .stopping, .loading: return 3
        case .ready: return panelOpen ? 5 : 15
        default: return panelOpen ? 10 : 30
        }
    }

    var currentStep: Step? {
        guard phase == .booting else { return nil }
        return Step.allCases.last { stepAt[$0] != nil }
    }

    // MARK: - CLI calls

    func loadConfig() async {
        guard let cli else { return }
        guard let r = try? await cli.run(["config", "show", "--json"]), r.ok,
              let c = try? JSON.decoder.decode(ConfigShow.self, from: r.out) else { return }
        config = c
        if up == nil, phase != .booting {
            provider = c.defaultProvider
            model = c.defaultModel
        }
    }

    func refresh() async {
        guard let cli else { return }
        if config == nil || !(config?.ready ?? false) { await loadConfig() }
        guard config?.ready ?? false else { phase = .noConfig; return }
        do {
            let r = try await cli.run(["status", "--json"])
            guard r.ok else { warning = r.message; if phase == .loading { phase = .off }; return }
            let s = try JSON.decoder.decode(Snapshot.self, from: r.out)
            warning = nil
            apply(s)
        } catch {
            warning = error.localizedDescription
        }
    }

    /// The poll is the source of truth; an `up` stream only adds detail while it runs.
    func apply(_ s: Snapshot) {
        let before = phase
        snap = s
        if let st = s.status, st.stage == "download" || st.stage == "verify" { download = st.download }
        let next = Store.derive(snap: s, upRunning: up != nil, current: phase)
        if next != phase { phase = next }
        if before == .ready, next == .off, !userStopped {
            let why = "stopped by itself (idle or expiry)"
            notify("lobo stopped", why)
        }
        if next == .off || next == .ready { userStopped = false }
        if next == .booting, bootStart == nil { bootStart = s.pod?.started_at ?? Date() }
        if next == .booting, let st = s.status, let step = Step.from(upPhase: st.stage), stepAt[step] == nil, let t0 = bootStart {
            // Resumed boot (app started mid-boot): fill steps from the agent stage.
            for c in Step.allCases where c.index <= step.index && stepAt[c] == nil { stepAt[c] = Date().timeIntervalSince(t0) }
        }
        if next == .off || next == .ready, up == nil { bootStart = nil; stepAt = [:]; download = nil }
    }

    nonisolated static func derive(snap s: Snapshot, upRunning: Bool, current: Phase) -> Phase {
        if case .failed = current, s.down, !upRunning { return current } // keep the error until dismissed
        if current == .stopping, !s.down { return .stopping }
        if s.down { return upRunning ? .booting : .off }
        guard let st = s.status else { return .booting } // pod exists, agent not answering yet
        switch st.stage {
        case "ready": return .ready
        case "failed", "terminating":
            return upRunning ? .booting : .failed(st.stage_detail ?? st.stage) // `up` re-rents bad hosts itself
        default: return .booting
        }
    }

    func start() {
        guard let cli, up == nil else { return }
        // Always explicit: a missing --q6 would let a saved LOBO_MODEL=q6 win over the Q8 pick.
        let args = ["up", "--json", "--provider", provider, "--q6=\(model == "q6")"]
        phase = .booting
        bootStart = Date()
        stepAt = [:]
        download = nil
        lastDetail = "renting \(provider)…"
        logTail = []
        readyURL = nil
        do {
            up = try cli.stream(args, onLine: { [weak self] line in
                guard let ev = try? JSON.decoder.decode(UpEvent.self, from: line) else { return }
                self?.handle(ev)
            }, onExit: { [weak self] code, stderr in
                self?.upExited(code: code, stderr: stderr)
            })
        } catch {
            phase = .failed(error.localizedDescription)
        }
    }

    func handle(_ ev: UpEvent) {
        if let step = Step.from(upPhase: ev.phase), let t0 = bootStart, stepAt[step] == nil {
            stepAt[step] = Date().timeIntervalSince(t0)
        }
        if let d = ev.download { download = d }
        if let d = ev.detail, !d.isEmpty {
            lastDetail = d
            logTail = Array((logTail + [d]).suffix(6))
        }
        if let err = ev.err {
            logTail = Array((logTail + err.split(separator: "\n").map(String.init)).suffix(8))
        }
        if let r = ev.ready {
            readyURL = r.url
            phase = .ready
            notify("lobo ready", "\(r.url) · \(Fmt.duration(Double(r.elapsed_ns) / 1e9)) · $\(String(format: "%.2f", r.usd_per_h))/h")
        }
    }

    func upExited(code: Int32, stderr: String) {
        up = nil
        if code != 0 && phase != .ready && phase != .stopping {
            let msg = logTail.last(where: { !$0.isEmpty }) ?? stderr.split(separator: "\n").last.map(String.init) ?? "exit \(code)"
            phase = .failed(msg)
            notify("lobo boot failed", msg)
        }
        Task { await refresh() }
    }

    /// Stop = end the `up` run cleanly, then delete every lobo pod, then check again: a create request
    /// that was in flight when `up` stopped can land a pod after the first `down` listed nothing.
    func stop() {
        guard let cli else { return }
        userStopped = true
        phase = .stopping
        Task {
            if let p = up {
                p.interrupt() // SIGINT: the CLI cancels its context (SIGTERM would kill it mid-request)
                for _ in 0..<150 where up != nil { try? await Task.sleep(nanoseconds: 100_000_000) }
                if up != nil { p.terminate() }
            }
            do {
                let r = try await cli.run(["down", "--json"])
                guard r.ok else { phase = .failed("down: " + r.message); await refresh(); return }
                try? await Task.sleep(nanoseconds: 10_000_000_000)
                let r2 = try await cli.run(["down", "--json"])
                guard r2.ok else { phase = .failed("down: " + r2.message); await refresh(); return }
                phase = .off
            } catch {
                // Could not even run the CLI: keep the pod visible, never claim OFF.
                phase = .failed("down did not run: \(error.localizedDescription)")
            }
            await refresh()
        }
    }

    func dismiss() {
        phase = .off
        Task { await refresh() }
    }

    func copyAPIKey() {
        guard let cli else { return }
        Task {
            if let r = try? await cli.run(["config", "get", "LOBO_API_KEY"]), r.ok {
                Pasteboard.copy(String(decoding: r.out, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines))
            }
        }
    }

    var endpoint: String? {
        if let readyURL { return readyURL }
        guard let d = config?.values["LOBO_DOMAIN"], !d.isEmpty else { return nil }
        return "https://\(d)/v1"
    }

    /// $ spent on the current pod so far.
    var spent: Double? {
        guard let p = snap?.pod, let t = p.started_at else { return nil }
        return p.cost_per_hr * now.timeIntervalSince(t) / 3600
    }

    // MARK: - notifications

    /// UNUserNotificationCenter aborts outside a real .app bundle (swift run, xctest).
    private var canNotify: Bool { Bundle.main.bundleURL.pathExtension == "app" }

    private func requestNotifications() {
        guard canNotify else { return }
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound]) { _, _ in }
    }

    func notify(_ title: String, _ body: String) {
        guard canNotify else { return }
        let c = UNMutableNotificationContent()
        c.title = title
        c.body = body
        UNUserNotificationCenter.current().add(UNNotificationRequest(identifier: UUID().uuidString, content: c, trigger: nil))
    }
}

extension Step {
    var index: Int { Step.allCases.firstIndex(of: self)! }
}

enum Pasteboard {
    static func copy(_ s: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(s, forType: .string)
    }
}

enum Fmt {
    static func duration(_ s: Double) -> String {
        let t = max(0, Int(s.rounded()))
        return t >= 3600 ? String(format: "%d:%02d:%02d", t / 3600, t / 60 % 60, t % 60) : String(format: "%d:%02d", t / 60, t % 60)
    }

    static func gb(_ bytes: Int64) -> String { String(format: "%.1f", Double(bytes) / 1e9) }

    /// ▓▓▓▓░░░░ bar of `width` cells.
    static func bar(_ frac: Double, width: Int) -> String {
        let f = min(1, max(0, frac.isFinite ? frac : 0))
        let full = Int((f * Double(width)).rounded())
        return String(repeating: "▓", count: full) + String(repeating: "░", count: width - full)
    }
}
