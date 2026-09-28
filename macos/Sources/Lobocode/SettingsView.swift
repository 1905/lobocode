import AppKit
import Security
import SwiftUI

/// Edits the same config file as `lobo config`, through `lobo config set` (never writes the file itself).
struct SettingsView: View {
    @ObservedObject var store: Store
    var rendering = false // PNG render: ImageRenderer can't draw a ScrollView or AppKit text fields
    @State private var f = Fields()
    @State private var status: (String, Color)?
    @State private var saving = false

    static let secrets = ["RUNPOD_API_KEY", "VASTAI_API_KEY", "CF_TUNNEL_TOKEN"]

    struct Fields: Equatable {
        var secrets: [String: String] = [:]     // typed; empty = keep, "-" = remove
        var plain: [String: String] = [:]
        var newAPIKey: String?                   // generated, not saved yet
    }

    var body: some View {
        Group {
            if rendering { form } else { ScrollView { form } }
        }
        .frame(width: 520, height: rendering ? nil : 640)
        .background(Theme.bg)
        .environment(\.colorScheme, .dark)
        .onAppear { Task { await store.loadConfig(); load() } }
    }

    private var form: some View {
            VStack(alignment: .leading, spacing: 18) {
                HStack(alignment: .bottom) {
                    Logo()
                    Spacer()
                    Text("config").font(Theme.mono(10)).foregroundColor(Theme.dim)
                }
                RasterBar(active: saving)
                fileBox
                section("providers  (one is enough)") {
                    secret("RUNPOD_API_KEY", "runpod key")
                    secret("VASTAI_API_KEY", "vast key")
                }
                section("access") {
                    plain("LOBO_DOMAIN", "domain", "lobo.example.com")
                    apiKeyRow
                    secret("CF_TUNNEL_TOKEN", "tunnel token")
                    plain("LOBO_BUCKET_URL", "bucket url", "https://pub-….r2.dev")
                }
                section("defaults for lobo up  (empty = built-in)") {
                    if (store.config?.providers.count ?? 0) > 1 {
                        picker("LOBO_PROVIDER", "provider", ["runpod", "vast"], def: "runpod")
                    }
                    picker("LOBO_MODEL", "model", ["q8", "q6"], def: "q8")
                    plain("LOBO_MIN_MBPS", "min MB/s", "100")
                    plain("LOBO_CTX", "context", "65536")
                    plain("LOBO_IDLE_MIN", "idle min", "30")
                    plain("LOBO_MAX_HOURS", "max hours", "12")
                    picker("LOBO_CLOUD", "runpod cloud", ["community", "secure"], def: "community")
                    plain("LOBO_VAST_MAX_DPH", "vast max $/h", "1.20")
                    plain("LOBO_POD_IMAGE", "pod image", "ghcr.io/1905/lobocode@sha256:…")
                }
                HStack {
                    if let (msg, color) = status { Text(msg).font(Theme.mono(10)).foregroundColor(color) }
                    Spacer()
                    Button("REVERT") { load() }.buttonStyle(BracketButtonStyle(color: Theme.dim))
                    Button("SAVE") { save() }.buttonStyle(BracketButtonStyle(color: Theme.green))
                        .keyboardShortcut("s", modifiers: .command)
                        .disabled(saving)
                }
            }
            .padding(22)
    }

    private var fileBox: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(store.config?.path ?? "~/.config/lobo/config.env").font(Theme.mono(11)).foregroundColor(Theme.text).textSelection(.enabled)
            Text("plain KEY=value lines, shared with the lobo CLI. edit it by hand any time: this window only touches the keys it shows.")
                .font(Theme.mono(10)).foregroundColor(Theme.dim).fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 14) {
                Button("reveal in finder") { if let p = store.config?.path { NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: p)]) } }
                Button("open in editor") { if let p = store.config?.path { NSWorkspace.shared.open(URL(fileURLWithPath: p)) } }
                    .disabled(!(store.config?.exists ?? false))
            }.buttonStyle(LinkButtonStyle(color: Theme.cyan))
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 6).fill(Theme.card))
        .overlay(RoundedRectangle(cornerRadius: 6).stroke(Theme.line, lineWidth: 1))
    }

    private func section<C: View>(_ title: String, @ViewBuilder _ c: () -> C) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("// \(title)").font(Theme.mono(11, .bold)).foregroundStyle(Theme.copper)
            c()
        }
    }

    private func label(_ s: String) -> some View {
        Text(s).font(Theme.mono(11)).foregroundColor(Theme.dim).frame(width: 110, alignment: .leading)
    }

    private func field<F: View>(_ f: F) -> some View {
        f.textFieldStyle(.plain)
            .font(Theme.mono(12))
            .foregroundColor(Theme.text)
            .padding(.vertical, 5).padding(.horizontal, 8)
            .background(RoundedRectangle(cornerRadius: 4).fill(Theme.card))
            .overlay(RoundedRectangle(cornerRadius: 4).stroke(Theme.line, lineWidth: 1))
    }

    private func plain(_ key: String, _ name: String, _ placeholder: String) -> some View {
        HStack {
            label(name)
            input(TextField(placeholder, text: Binding(get: { f.plain[key] ?? "" }, set: { f.plain[key] = $0 })),
                  value: f.plain[key] ?? "", placeholder: placeholder)
        }
    }

    /// The editable control, or in render mode the text it would show (ImageRenderer draws AppKit fields as blocks).
    @ViewBuilder private func input<E: View>(_ editable: E, value: String, placeholder: String) -> some View {
        if rendering {
            field(Text(value.isEmpty ? placeholder : value).foregroundColor(value.isEmpty ? Theme.faint : Theme.text)
                .lineLimit(1).frame(maxWidth: .infinity, alignment: .leading))
        } else {
            field(editable)
        }
    }

    private func secret(_ key: String, _ name: String) -> some View {
        let now = store.config?.values[key] ?? ""
        let hint = now.isEmpty ? "not set" : "\(now)  (empty = keep, - = remove)"
        return HStack {
            label(name)
            input(SecureField(hint, text: Binding(get: { f.secrets[key] ?? "" }, set: { f.secrets[key] = $0 })),
                  value: "", placeholder: hint)
        }
    }

    private var apiKeyRow: some View {
        HStack {
            label("api key")
            Text(f.newAPIKey.map { Mask.mask($0) + "  (new, unsaved)" } ?? (store.config?.values["LOBO_API_KEY"] ?? "not set"))
                .font(Theme.mono(12)).foregroundColor(f.newAPIKey == nil ? Theme.text : Theme.amber)
            Spacer()
            Button("generate") { f.newAPIKey = Mask.newAPIKey() }.buttonStyle(LinkButtonStyle(color: Theme.cyan))
        }
    }

    private func picker(_ key: String, _ name: String, _ options: [String], def: String) -> some View {
        HStack {
            label(name)
            ForEach(options, id: \.self) { o in
                let cur = (f.plain[key] ?? "").isEmpty ? def : f.plain[key]!
                Button { f.plain[key] = o } label: {
                    Text(o == cur ? "[\(o)]" : " \(o) ").font(Theme.mono(12)).foregroundColor(o == cur ? Theme.green : Theme.dim)
                }.buttonStyle(.plain)
            }
            Spacer()
        }
    }

    private static let plainKeys = ["LOBO_DOMAIN", "LOBO_BUCKET_URL", "LOBO_PROVIDER", "LOBO_MODEL", "LOBO_MIN_MBPS", "LOBO_CTX",
                                    "LOBO_IDLE_MIN", "LOBO_MAX_HOURS", "LOBO_CLOUD", "LOBO_VAST_MAX_DPH", "LOBO_POD_IMAGE"]

    private func load() {
        var n = Fields()
        for k in SettingsView.plainKeys { n.plain[k] = store.config?.values[k] ?? "" }
        f = n
        status = nil
    }

    /// Only changed keys go to `lobo config set`.
    static func changes(_ f: Fields, current: [String: String]) -> [String: String] {
        var set: [String: String] = [:]
        for (k, v) in f.plain where v.trimmingCharacters(in: .whitespaces) != (current[k] ?? "") {
            set[k] = v.trimmingCharacters(in: .whitespaces)
        }
        for (k, v) in f.secrets {
            let t = v.trimmingCharacters(in: .whitespacesAndNewlines)
            if t == "-" { set[k] = "" } else if !t.isEmpty { set[k] = t }
        }
        if let k = f.newAPIKey { set["LOBO_API_KEY"] = k }
        return set
    }

    static func validate(_ set: [String: String]) -> String? {
        let ints: [String: Int] = ["LOBO_MIN_MBPS": 1, "LOBO_CTX": 512, "LOBO_IDLE_MIN": 1, "LOBO_MAX_HOURS": 1]
        for (k, min) in ints {
            if let v = set[k], !v.isEmpty, v != "0", (Int(v) ?? -1) < min { return "\(k): whole number ≥ \(min), or empty" }
        }
        if let v = set["LOBO_VAST_MAX_DPH"], !v.isEmpty, (Double(v) ?? 0) <= 0 { return "LOBO_VAST_MAX_DPH: a price like 1.20" }
        if let v = set["LOBO_DOMAIN"], v.contains("/") || v.contains(" ") { return "LOBO_DOMAIN: bare hostname, no https://" }
        return nil
    }

    private func save() {
        if !(store.config?.has("LOBO_API_KEY") ?? false), f.newAPIKey == nil {
            f.newAPIKey = Mask.newAPIKey() // lobo can't run without one; first save creates it
        }
        let set = SettingsView.changes(f, current: store.config?.values ?? [:])
        guard !set.isEmpty else { status = ("nothing changed", Theme.dim); return }
        if let e = SettingsView.validate(set) { status = (e, Theme.red); return }
        guard let cli = store.cli else { return }
        saving = true
        Task {
            // Secrets go through stdin as JSON, never argv.
            let json = try? JSONSerialization.data(withJSONObject: set)
            let r = try? await cli.run(["config", "set", "--stdin"], stdin: json)
            saving = false
            if let r, r.ok {
                status = ("saved \(set.count) key\(set.count == 1 ? "" : "s")", Theme.green)
                await store.loadConfig()
                load()
                status = ("saved \(set.count) key\(set.count == 1 ? "" : "s")", Theme.green)
                await store.refresh()
            } else {
                status = ("save failed: \(r?.message ?? "lobo CLI not found")", Theme.red)
            }
        }
    }
}

enum Mask {
    static func mask(_ s: String) -> String { s.count < 12 ? "••••" : "\(s.prefix(4))…\(s.suffix(4))" }

    static func newAPIKey() -> String {
        var b = [UInt8](repeating: 0, count: 24)
        _ = SecRandomCopyBytes(kSecRandomDefault, b.count, &b)
        return "sk-" + b.map { String(format: "%02x", $0) }.joined()
    }
}
