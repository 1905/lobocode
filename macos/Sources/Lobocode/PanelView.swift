import SwiftUI

/// The menu bar panel.
struct PanelView: View {
    @ObservedObject var store: Store
    var openSettings: () -> Void = {}

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Header(store: store)
            VStack(alignment: .leading, spacing: 12) {
                content
                if let w = store.warning {
                    Text("! \(w)").font(Theme.mono(10)).foregroundColor(Theme.amber).lineLimit(2)
                }
            }
            .padding(14)
            Footer(store: store, openSettings: openSettings)
        }
        .frame(width: 340)
        .background(Theme.bg)
        .environment(\.colorScheme, .dark)
    }

    @ViewBuilder private var content: some View {
        switch store.phase {
        case .loading:
            HStack(spacing: 4) { Text("scanning providers").foregroundColor(Theme.dim); Cursor() }.font(Theme.mono(12))
        case .noConfig:
            SetupCard(store: store, openSettings: openSettings)
        case .off:
            StartCard(store: store, openSettings: openSettings)
        case .booting:
            BootLog(store: store)
        case .ready:
            ReadyCard(store: store)
        case .stopping:
            HStack(spacing: 6) {
                Text("[ .. ]").foregroundColor(Theme.cyan)
                Text(store.isLocal ? "stopping llama.cpp" : "deleting pod on \(store.snap?.pod?.provider ?? "provider")").foregroundColor(Theme.text)
                Cursor()
            }.font(Theme.mono(12))
        case .failed(let msg):
            FailCard(store: store, message: msg)
        }
    }
}

struct Header: View {
    @ObservedObject var store: Store

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Logo()
            RasterBar(active: store.phase == .booting || store.phase == .stopping || store.phase == .loading)
            HStack(spacing: 6) {
                Text("sys:").font(Theme.mono(11)).foregroundColor(Theme.dim)
                GlitchText(text: store.phase.word, color: Theme.color(for: store.phase))
                Spacer(minLength: 4)
                Text(store.snap?.version?.version ?? "").font(Theme.mono(9)).foregroundColor(Theme.faint).lineLimit(1).truncationMode(.middle)
            }
            if !detail.isEmpty {
                Text(detail).font(Theme.mono(10)).foregroundColor(Theme.dim).lineLimit(2).fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.horizontal, 14)
        .padding(.top, 14)
        .padding(.bottom, 10)
        .background(ZStack { Theme.card; Scanlines() })
        .overlay(Rectangle().fill(Theme.line).frame(height: 1), alignment: .bottom)
    }

    private var detail: String {
        guard let p = store.snap?.pod else {
            if store.phase == .booting { return store.isLocal ? "starting local" : "renting \(store.provider)" }
            guard store.phase == .off else { return "" }
            return store.isLocal ? "local · $0" : "no pod · $0.00/h"
        }
        var parts = [p.provider]
        if let d = p.detail, !d.isEmpty { parts.append(d) }
        parts.append(p.provider == "local" ? "$0" : String(format: "$%.2f/h", p.cost_per_hr))
        return parts.joined(separator: " · ")
    }
}

struct SetupCard: View {
    @ObservedObject var store: Store
    var openSettings: () -> Void
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("no usable config yet").font(Theme.mono(12, .bold)).foregroundColor(Theme.amber)
            Text(store.config?.path ?? "~/.config/lobo/config.env").font(Theme.mono(10)).foregroundColor(Theme.dim)
                .textSelection(.enabled)
            Text(ConfigShow.localSupported ? "needs an api key. cloud also needs a provider key, domain, tunnel token and bucket URL."
                                           : "needs a provider key (RunPod or Vast), domain, tunnel token and bucket URL.")
                .font(Theme.mono(10)).foregroundColor(Theme.dim).fixedSize(horizontal: false, vertical: true)
            Button("SETUP", action: openSettings).buttonStyle(BracketButtonStyle(color: Theme.amber, wide: true))
        }
    }
}

struct StartCard: View {
    @ObservedObject var store: Store
    var openSettings: () -> Void = {}

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if ConfigShow.localSupported {
                BracketPicker(label: "target", options: Target.allCases.map(\.rawValue),
                              value: Binding(get: { store.target.rawValue }, set: { store.choose(Target(rawValue: $0) ?? .cloud) }))
            }
            if store.target == .local {
                LocalStart(store: store)
            } else {
                cloud
            }
        }
    }

    @ViewBuilder private var cloud: some View {
        if !(store.config?.cloudReady ?? false) {
            row("cloud", "no keys")
            Button("SETUP", action: openSettings)
                .buttonStyle(BracketButtonStyle(color: Theme.amber, wide: true))
                .padding(.top, 4)
        } else {
            if (store.config?.providers.count ?? 0) > 1 {
                BracketPicker(label: "provider", options: store.config?.providers ?? [], value: $store.provider)
            } else {
                row("provider", store.provider)
            }
            BracketPicker(label: "model", options: ["q8", "q6"], value: $store.model)
            row("limits", limits).font(Theme.mono(11))
            Button("START", action: store.start)
                .buttonStyle(BracketButtonStyle(color: Theme.green, wide: true))
                .keyboardShortcut(.defaultAction)
                .padding(.top, 4)
        }
    }

    private var limits: String {
        let v = store.config?.values ?? [:]
        let mbps = v["LOBO_MIN_MBPS"].flatMap { $0.isEmpty ? nil : $0 } ?? "100"
        let idle = v["LOBO_IDLE_MIN"].flatMap { $0.isEmpty || $0 == "0" ? nil : $0 } ?? "30"
        let maxH = v["LOBO_MAX_HOURS"].flatMap { $0.isEmpty || $0 == "0" ? nil : $0 } ?? "12"
        return "≥\(mbps)MB/s idle \(idle)m max \(maxH)h"
    }

    private func row(_ k: String, _ v: String) -> some View {
        HStack(spacing: 6) {
            Text("> \(k)").foregroundColor(Theme.dim).frame(width: 84, alignment: .leading)
            Text(v).foregroundColor(Theme.text)
            Spacer()
        }.font(Theme.mono(12))
    }
}

/// Local off panel: one selectable row per catalog model, the weights folder, START.
struct LocalStart: View {
    @ObservedObject var store: Store

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let m = store.models {
                ForEach(m.models) { modelRow($0) }
                Text("\(m.weights) · \(Fmt.gb(m.free_bytes)) GB free")
                    .font(Theme.mono(10)).foregroundColor(Theme.dim).lineLimit(1).truncationMode(.middle)
            } else {
                BracketPicker(label: "model", options: ["q8", "q6"], value: $store.model)
            }
            Button("START", action: store.start)
                .buttonStyle(BracketButtonStyle(color: Theme.green, wide: true))
                .keyboardShortcut(.defaultAction)
                .padding(.top, 4)
        }
    }

    /// `[q6]  22.1 GB  ✓ on disk` / ` q8   28.6 GB  ↓ download` / `partial 43%`
    private func modelRow(_ m: LocalModel) -> some View {
        let on = m.id == store.model
        return Button { store.model = m.id } label: {
            HStack(spacing: 10) {
                Text(on ? "[\(m.id)]" : " \(m.id) ").foregroundColor(on ? Theme.green : Theme.dim)
                Text("\(Fmt.gb(m.size)) GB").foregroundColor(on ? Theme.text : Theme.dim)
                state(m.state)
                Spacer()
            }
            .font(Theme.mono(12))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    @ViewBuilder private func state(_ s: LocalModel.State) -> some View {
        switch s {
        case .onDisk: Text("✓ on disk").foregroundColor(Theme.text)
        case .partial(let f): Text("partial \(min(99, Int((f * 100).rounded())))%").foregroundColor(Theme.cyan)
        case .missing: Text("↓ download").foregroundColor(Theme.dim)
        }
    }
}

struct BootLog: View {
    @ObservedObject var store: Store

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            ForEach(store.bootSteps) { step in stepRow(step) }
            Text(store.lastDetail)
                .font(Theme.mono(9)).foregroundColor(Theme.faint).lineLimit(2).truncationMode(.middle)
                .padding(.top, 2)
            HStack {
                Text("T+\(Fmt.duration(elapsed))").font(Theme.mono(11, .bold)).foregroundColor(Theme.cyan)
                Spacer()
                Button("ABORT", action: store.stop).buttonStyle(BracketButtonStyle(color: Theme.red))
            }
            .padding(.top, 4)
        }
    }

    private var elapsed: Double { store.bootStart.map { store.now.timeIntervalSince($0) } ?? 0 }

    @ViewBuilder private func stepRow(_ step: Step) -> some View {
        let current = store.currentStep == step
        let done = store.stepAt[step] != nil && !current
        VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 8) {
                Text(done ? "[ OK ]" : current ? "[ >> ]" : "[ .. ]")
                    .foregroundColor(done ? Theme.green : current ? Theme.cyan : Theme.faint)
                Text(step.label(local: store.isLocal))
                    .foregroundColor(done || current ? Theme.text : Theme.faint)
                if current { Cursor() }
                Spacer(minLength: 0)
                if let t = store.stepAt[step] {
                    Text(Fmt.duration(t)).foregroundColor(Theme.dim)
                }
            }
            if step == .download, current {
                if let d = store.download, d.total > 0 {
                    downloadLine(d)
                } else if store.upPhase == "verify" || store.snap?.status?.stage == "verify" {
                    Text("verify").foregroundColor(Theme.text).font(Theme.mono(10)).padding(.leading, 56)
                }
            }
        }
        .font(Theme.mono(11))
    }

    /// `       ▓▓▓▓▓▓░░░░░░░░ 43%  12.4/28.6 GB  713 MB/s  0:23`
    private func downloadLine(_ d: Download) -> some View {
        let frac = Double(d.bytes) / Double(d.total)
        let eta = d.mbps > 0 ? Double(d.total - d.bytes) / (d.mbps * 1e6) : 0
        return HStack(spacing: 6) {
            Text(Fmt.bar(frac, width: 14)).foregroundStyle(Theme.copper)
            if d.verifying == true {
                Text("verify sha256").foregroundColor(Theme.text)
            } else {
                Text("\(Fmt.gb(d.bytes))/\(Fmt.gb(d.total))G").foregroundColor(Theme.text)
                // Cloud: below the min MB/s the pod gets re-rented (money). Local has no minimum.
                Text("\(Int(d.mbps))MB/s").foregroundColor(store.isLocal ? Theme.text : d.mbps >= 100 ? Theme.green : Theme.amber)
                if eta > 0 { Text(Fmt.duration(eta)).foregroundColor(Theme.dim) }
            }
        }
        .font(Theme.mono(10))
        .lineLimit(1)
        .padding(.leading, 56)
    }
}

struct ReadyCard: View {
    @ObservedObject var store: Store
    @State private var copied: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            valueRow("endpoint", store.endpoint ?? "?", copy: { Pasteboard.copy(store.endpoint ?? "") }, id: "url")
            valueRow("api key", store.config?.values["LOBO_API_KEY"] ?? "?", copy: store.copyAPIKey, id: "key")
            HStack(spacing: 8) {
                tile("gen", store.snap?.status?.llama?.gen_tps, "tok/s")
                tile("prompt", store.snap?.status?.llama?.prompt_tps, "tok/s")
            }
            if let g = store.snap?.status?.gpu {
                // Local: used = llama RSS, total = system RAM, so it is memory, not vram.
                HStack(spacing: 6) {
                    Text(local ? "memory" : "vram").foregroundColor(Theme.dim).frame(width: 52, alignment: .leading)
                    Text(Fmt.bar(Double(g.vram_used_mb) / Double(max(1, g.vram_total_mb)), width: 12)).foregroundStyle(Theme.copper)
                    Text(String(format: "%.1f/%.1f GB", Double(g.vram_used_mb) / 1024, Double(g.vram_total_mb) / 1024)).foregroundColor(Theme.text)
                    Spacer()
                    if !local { Text("gpu \(g.util_pct)%").foregroundColor(g.util_pct > 0 ? Theme.green : Theme.dim) }
                }.font(Theme.mono(10))
            }
            HStack(spacing: 0) {
                if let k = killIn {
                    Text(local ? "idle-stop " : "idle-kill ").foregroundColor(Theme.dim)
                    Text(Fmt.duration(k)).foregroundColor(!local && k < 300 ? Theme.amber : Theme.text)
                }
                Text("  ·  T+\(Fmt.duration(uptime))").foregroundColor(Theme.dim)
                Spacer()
                if local {
                    Text("local · $0").foregroundColor(Theme.dim)
                } else if let s = store.spent {
                    Text(String(format: "$%.2f", s)).foregroundColor(Theme.text)
                }
            }.font(Theme.mono(10))
            Button("STOP", action: store.stop).buttonStyle(BracketButtonStyle(color: Theme.red, wide: true))
        }
    }

    private var local: Bool { store.isLocal }

    private var sinceSnap: Double { store.snap?.at.map { store.now.timeIntervalSince($0) } ?? 0 }

    private var killIn: Double? {
        guard let st = store.snap?.status, let k = st.kill_in_s else { return nil }
        if (st.llama?.requests_processing ?? 0) > 0 { return Double(k) } // busy: the timer is not running down
        return max(0, Double(k) - sinceSnap)
    }

    private var uptime: Double {
        guard let t = store.snap?.pod?.started_at else { return 0 }
        return store.now.timeIntervalSince(t)
    }

    private func valueRow(_ k: String, _ v: String, copy: @escaping () -> Void, id: String) -> some View {
        HStack(spacing: 6) {
            Text(k).foregroundColor(Theme.dim).frame(width: 64, alignment: .leading)
            Text(v).foregroundColor(Theme.text).lineLimit(1).truncationMode(.middle).textSelection(.enabled)
            Spacer(minLength: 4)
            Button(copied == id ? "copied" : "copy") {
                copy()
                copied = id
                DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) { if copied == id { copied = nil } }
            }
            .buttonStyle(LinkButtonStyle(color: copied == id ? Theme.green : Theme.cyan))
        }
        .font(Theme.mono(11))
    }

    private func tile(_ label: String, _ v: Double?, _ unit: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label).font(Theme.mono(9)).foregroundColor(Theme.dim)
            HStack(alignment: .firstTextBaseline, spacing: 4) {
                Text(v.map { $0 >= 100 ? String(format: "%.0f", $0) : String(format: "%.1f", $0) } ?? "—")
                    .font(Theme.mono(22, .bold)).foregroundColor(Theme.green)
                    .shadow(color: Theme.green.opacity(0.35), radius: 6)
                Text(unit).font(Theme.mono(9)).foregroundColor(Theme.dim)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(10)
        .background(RoundedRectangle(cornerRadius: 6).fill(Theme.card))
        .overlay(RoundedRectangle(cornerRadius: 6).stroke(Theme.line, lineWidth: 1))
    }
}

struct FailCard: View {
    @ObservedObject var store: Store
    let message: String
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .top, spacing: 8) {
                Text("[FAIL]").foregroundColor(Theme.red).font(Theme.mono(11, .bold))
                Text(message).foregroundColor(Theme.text).font(Theme.mono(11)).fixedSize(horizontal: false, vertical: true)
            }
            if !store.logTail.isEmpty {
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(Array(store.logTail.suffix(5).enumerated()), id: \.offset) { _, l in
                        Text(l).font(Theme.mono(9)).foregroundColor(Theme.faint).lineLimit(1).truncationMode(.middle)
                    }
                }
                .padding(8)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(RoundedRectangle(cornerRadius: 4).fill(Theme.card))
            }
            HStack {
                if let pod = store.snap?.pod {
                    // A pod is still there (and billing): the only useful action is to delete it.
                    Button("STOP \(pod.provider)", action: store.stop).buttonStyle(BracketButtonStyle(color: Theme.red))
                } else {
                    Button("RETRY", action: store.start).buttonStyle(BracketButtonStyle(color: Theme.green))
                }
                Button("DISMISS", action: store.dismiss).buttonStyle(BracketButtonStyle(color: Theme.dim))
            }
        }
    }
}

struct Footer: View {
    @ObservedObject var store: Store
    var openSettings: () -> Void

    var body: some View {
        HStack(spacing: 14) {
            Button("settings ⌘,", action: openSettings).keyboardShortcut(",", modifiers: .command)
            Button("config") {
                if let p = store.config?.path { NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: p)]) }
            }
            Spacer()
            Button("quit ⌘q") { NSApp.terminate(nil) }.keyboardShortcut("q", modifiers: .command)
        }
        .buttonStyle(LinkButtonStyle())
        .padding(.horizontal, 14)
        .padding(.vertical, 9)
        .background(Theme.card)
        .overlay(Rectangle().fill(Theme.line).frame(height: 1), alignment: .top)
    }
}
