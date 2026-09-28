import AppKit
import SwiftUI

/// Renders every panel state (and the menu bar item) to PNGs from fixed data. `Lobocode --render <dir>`.
@MainActor
enum Renderer {
    static func run(to dir: URL) {
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        for (name, store) in samples() {
            write(PanelView(store: store), dir.appendingPathComponent("panel_\(name).png"))
            write(MenuLabelPreview(store: store), dir.appendingPathComponent("menubar_\(name).png"))
        }
        let s = Store(cli: nil)
        s.config = sampleConfig
        write(SettingsView(store: s, scroll: false), dir.appendingPathComponent("settings.png"))
        write(AppIconView(), dir.appendingPathComponent("icon_1024.png"), scale: 1)
        print("rendered to \(dir.path)")
    }

    private static func write<V: View>(_ v: V, _ url: URL, scale: CGFloat = 2) {
        let r = ImageRenderer(content: v)
        r.scale = scale
        guard let img = r.nsImage, let tiff = img.tiffRepresentation, let rep = NSBitmapImageRep(data: tiff),
              let png = rep.representation(using: .png, properties: [:]) else { return }
        try? png.write(to: url)
    }

    static let sampleConfig = ConfigShow(path: "/Users/you/.config/lobo/config.env", exists: true,
        values: ["RUNPOD_API_KEY": "rpa_…86az", "VASTAI_API_KEY": "5547…ff6b", "LOBO_DOMAIN": "lobo.example.com", "LOBO_API_KEY": "sk-2…fb35",
                 "CF_TUNNEL_TOKEN": "eyJh…fQ==", "LOBO_BUCKET_URL": "https://pub-8046….r2.dev", "LOBO_MIN_MBPS": "100"],
        set: ["RUNPOD_API_KEY": true, "VASTAI_API_KEY": true, "LOBO_DOMAIN": true, "LOBO_API_KEY": true, "CF_TUNNEL_TOKEN": true,
              "LOBO_BUCKET_URL": true, "LOBO_MIN_MBPS": true])

    static func samples() -> [(String, Store)] {
        let now = Date()
        func base(_ phase: Phase) -> Store {
            let s = Store(cli: nil)
            s.config = sampleConfig
            s.phase = phase
            s.now = now
            s.provider = "vast"
            return s
        }
        let off = base(.off)
        off.snap = Snapshot(pod: nil, version: nil, status: nil, down: true, at: now)

        let boot = base(.booting)
        boot.bootStart = now.addingTimeInterval(-72)
        boot.stepAt = [.rent: 8, .container: 41, .tunnel: 43, .gpu: 44, .download: 45]
        boot.download = Download(bytes: 12_400_000_000, total: 28_595_762_272, mbps: 713, verifying: nil, source: nil)
        boot.lastDetail = "vast 26461230, offer 51401937, 18877 Mbps down, California, US, $0.73/h"
        boot.snap = Snapshot(pod: Pod(provider: "vast", id: "26461230", status: "running", detail: "offer 51401937, 18877 Mbps down, California, US",
                                      cost_per_hr: 0.73, started_at: now.addingTimeInterval(-72), host_download_mbps: 18877),
                             version: Release(version: "2026.09.25-10", git_sha: "4b513b3"), status: nil, down: false, at: now)

        let ready = base(.ready)
        ready.snap = Snapshot(pod: Pod(provider: "runpod", id: "rysv8058qqhsqc", status: "RUNNING", detail: "SECURE, ≥5000 Mbps",
                                       cost_per_hr: 0.69, started_at: now.addingTimeInterval(-8342), host_download_mbps: 5000),
                              version: Release(version: "2026.09.25-10", git_sha: "4b513b3"),
                              status: AgentStatus(stage: "ready", stage_detail: nil, download: nil, uptime_s: 8342, idle_s: 180, kill_in_s: 1634,
                                                  kill_reason: "idle", expires_at: nil,
                                                  gpu: GPU(name: "NVIDIA GeForce RTX 5090", vram_used_mb: 29316, vram_total_mb: 32607, util_pct: 87),
                                                  llama: Llama(requests_processing: 1, requests_deferred: 0, prompt_tokens_total: 857350,
                                                               gen_tokens_total: 115498, prompt_tps: 503.9, gen_tps: 45.1),
                                                  model: "q8", ctx: 65536),
                              down: false, at: now)

        let fail = base(.failed("gave up: 4 pods in a row landed on bad hosts (all deleted)"))
        fail.logTail = ["image: download: host: download too slow: 41.2 MB/s after 20s from https://acc.r2.cloudflarestorage.com (min 100)",
                        "create: bad host, renting another pod (4/4)", "vast 26461301, offer 51399812, 9129 Mbps down, Quebec, CA"]

        let setup = base(.noConfig)
        setup.config = ConfigShow(path: "/Users/you/.config/lobo/config.env", exists: false, values: [:], set: [:])

        return [("off", off), ("boot", boot), ("ready", ready), ("fail", fail), ("setup", setup)]
    }
}

/// The menu bar item on a dark strip, for the renders.
struct MenuLabelPreview: View {
    @ObservedObject var store: Store
    var body: some View {
        MenuLabel(store: store)
            .foregroundColor(.white)
            .padding(.horizontal, 8).padding(.vertical, 3)
            .background(Color(hex: 0x2A2A2E))
            .environment(\.colorScheme, .dark)
    }
}

/// 1024 px app icon: dark tile, gradient block logo, a thin raster line, green status square.
struct AppIconView: View {
    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 185, style: .continuous).fill(Theme.bg)
            RoundedRectangle(cornerRadius: 185, style: .continuous).stroke(Theme.line, lineWidth: 10)
            Scanlines().clipShape(RoundedRectangle(cornerRadius: 185, style: .continuous)).opacity(2)
            VStack(spacing: 36) {
                Text(Logo.art)
                    .font(.system(size: 23, weight: .bold, design: .monospaced))
                    .lineSpacing(-3)
                    .foregroundStyle(Theme.copper)
                    .fixedSize()
                Rectangle().fill(Theme.copper).frame(width: 460, height: 8).opacity(0.8)
                HStack(spacing: 18) {
                    RoundedRectangle(cornerRadius: 8).fill(Theme.green).frame(width: 46, height: 46)
                        .shadow(color: Theme.green.opacity(0.7), radius: 20)
                    Text("RUN").font(.system(size: 52, weight: .bold, design: .monospaced)).foregroundColor(Theme.green)
                }
            }
        }
        .frame(width: 824, height: 824)
        .frame(width: 1024, height: 1024) // Apple icon grid: 824 px tile, transparent margin
    }
}
