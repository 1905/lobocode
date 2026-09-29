import XCTest
@testable import Lobocode

final class LobocodeTests: XCTestCase {
    func fixture(_ name: String) throws -> Data {
        let url = try XCTUnwrap(Bundle.module.url(forResource: "Fixtures/\(name)", withExtension: nil))
        return try Data(contentsOf: url)
    }

    func testDecodeReadyStatus() throws {
        let s = try JSON.decoder.decode(Snapshot.self, from: fixture("status_ready.json"))
        XCTAssertEqual(s.pod?.provider, "runpod")
        XCTAssertEqual(s.status?.stage, "ready")
        XCTAssertNotNil(s.status?.gpu)
        XCTAssertNotNil(s.status?.llama)
        XCTAssertNotNil(s.pod?.started_at)
        XCTAssertFalse(s.down)
    }

    func testDecodeOffStatus() throws {
        let s = try JSON.decoder.decode(Snapshot.self, from: fixture("status_off.json"))
        XCTAssertTrue(s.down)
        XCTAssertNil(s.pod)
        XCTAssertNotNil(s.at)
    }

    func testDecodeUpEvents() throws {
        let lines = String(decoding: try fixture("up_events.jsonl"), as: UTF8.self).split(separator: "\n")
        let evs = try lines.map { try JSON.decoder.decode(UpEvent.self, from: Data($0.utf8)) }
        XCTAssertEqual(evs.map(\.phase), ["create", "image", "tunnel", "gpu", "download", "load", "ready", "failed"])
        XCTAssertEqual(evs[4].download?.mbps ?? 0, 713.2, accuracy: 0.01)
        XCTAssertEqual(evs[6].ready?.url, "https://lobo.example.com/v1")
        XCTAssertEqual(evs[7].err, "gave up: 4 pods in a row landed on bad hosts (all deleted)")
    }

    func testNanosecondDates() {
        XCTAssertNotNil(JSON.parseDate("2026-09-25T14:45:45.395971328Z"))
        XCTAssertNotNil(JSON.parseDate("2026-09-26T01:04:57.038014+08:00"))
        XCTAssertNotNil(JSON.parseDate("2026-09-26T02:32:59Z"))
    }

    func snap(down: Bool, stage: String? = nil) -> Snapshot {
        Snapshot(pod: down ? nil : Pod(provider: "runpod", id: "p", status: "RUNNING", detail: nil, cost_per_hr: 0.69, started_at: nil, host_download_mbps: nil),
                 version: nil, status: stage.map { AgentStatus(stage: $0) }, down: down, at: Date())
    }

    func testDerive() {
        XCTAssertEqual(Store.derive(snap: snap(down: true), upRunning: false, current: .loading), .off)
        XCTAssertEqual(Store.derive(snap: snap(down: true), upRunning: true, current: .booting), .booting)       // renting, nothing listed yet
        XCTAssertEqual(Store.derive(snap: snap(down: false), upRunning: false, current: .off), .booting)        // pod up, agent silent
        XCTAssertEqual(Store.derive(snap: snap(down: false, stage: "download"), upRunning: false, current: .off), .booting)
        XCTAssertEqual(Store.derive(snap: snap(down: false, stage: "ready"), upRunning: false, current: .booting), .ready)
        XCTAssertEqual(Store.derive(snap: snap(down: false, stage: "failed"), upRunning: true, current: .booting), .booting) // up re-rents
        XCTAssertEqual(Store.derive(snap: snap(down: false, stage: "failed"), upRunning: false, current: .booting), .failed("failed"))
        XCTAssertEqual(Store.derive(snap: snap(down: true), upRunning: false, current: .failed("x")), .failed("x")) // kept until dismissed
        XCTAssertEqual(Store.derive(snap: snap(down: false, stage: "ready"), upRunning: false, current: .stopping), .stopping)
    }

    @MainActor
    func testUpEventsDriveSteps() {
        let s = Store(cli: nil)
        s.phase = .booting
        s.bootStart = Date()
        s.handle(UpEvent(phase: "create", detail: "vast 1"))
        s.handle(UpEvent(phase: "download", download: Download(bytes: 50, total: 100, mbps: 200)))
        XCTAssertEqual(s.currentStep, .download)
        XCTAssertEqual(s.menuText, "50%")
        XCTAssertEqual(s.bootProgress, (Double(Step.download.index) + 0.5) / Double(Step.allCases.count), accuracy: 0.001)
        s.handle(UpEvent(phase: "ready", ready: ReadyInfo(pod_id: "1", provider: "vast", detail: "", attempts: 1, url: "https://x/v1", version: "v", usd_per_h: 0.7, elapsed_ns: 1)))
        XCTAssertEqual(s.phase, .ready)
        XCTAssertEqual(s.endpoint, "https://x/v1")
    }

    func testDecodeModels() throws {
        let m = try JSON.decoder.decode(ModelsInfo.self, from: fixture("models.json"))
        XCTAssertEqual(m.weights, "/Volumes/Extreme/_lobocode")
        XCTAssertEqual(m.free_bytes, 958_902_697_984)
        XCTAssertEqual(m.models.map(\.id), ["q6", "q8"])
        XCTAssertEqual(m.model("q6")?.state, .onDisk)
        XCTAssertEqual(Fmt.gb(m.model("q6")!.size), "22.1")
        XCTAssertEqual(Fmt.gb(m.model("q8")!.size), "28.6")
        XCTAssertEqual(m.runtime, ModelsInfo.Runtime(version: "b11118", present: false))
        XCTAssertEqual(LocalModel(id: "q8", file: "f", size: 100, on_disk: 43, verified: false).state, .partial(0.43))
        XCTAssertEqual(LocalModel(id: "q8", file: "f", size: 100, on_disk: 0, verified: false).state, .missing)
    }

    func testLocalSteps() {
        XCTAssertEqual(Step.steps(local: true).map { $0.label(local: true) }, ["start", "metal", "model", "load", "ready"])
        XCTAssertEqual(Step.steps(local: false), Step.allCases)
        XCTAssertEqual(Step.from(upPhase: "create"), .rent)
        XCTAssertEqual(Step.from(upPhase: "gpu"), .gpu)
        XCTAssertEqual(Step.from(upPhase: "verify"), .download)
        XCTAssertEqual(Step.from(upPhase: "load"), .load)
        XCTAssertEqual(Step.from(upPhase: "ready"), .ready)
        XCTAssertEqual(Step.gpu.label(local: false), "gpu")
    }

    @MainActor
    func testLocalBoot() {
        let s = Store(cli: nil)
        s.target = .local
        s.model = "q6"
        XCTAssertEqual(s.upArgs, ["up", "--json", "--provider", "local", "--q6=true"])
        s.model = "q8"
        XCTAssertEqual(s.upArgs, ["up", "--json", "--provider", "local", "--q6=false"])
        s.target = .cloud
        s.provider = "vast"
        XCTAssertEqual(s.upArgs, ["up", "--json", "--provider", "vast", "--q6=false"])

        s.target = .local
        s.phase = .booting
        s.bootStart = Date()
        s.handle(UpEvent(phase: "create", detail: "llama.cpp b11118 11 MB"))
        s.handle(UpEvent(phase: "gpu"))
        XCTAssertEqual(s.currentStep, .gpu)
        XCTAssertEqual(s.menuText, "metal")
        XCTAssertEqual(s.bootProgress, 0.2, accuracy: 0.001) // 1 of 5 local steps
        s.handle(UpEvent(phase: "verify"))
        XCTAssertEqual(s.currentStep, .download)
        XCTAssertEqual(s.upPhase, "verify")
        XCTAssertNil(s.endpoint.flatMap { $0.hasPrefix("https") ? $0 : nil })
        s.handle(UpEvent(phase: "ready", ready: ReadyInfo(pod_id: "local", provider: "local", detail: "", attempts: 1, url: "http://127.0.0.1:8931/v1", version: "v", usd_per_h: 0, elapsed_ns: 1)))
        XCTAssertEqual(s.phase, .ready)
        XCTAssertEqual(s.endpoint, "http://127.0.0.1:8931/v1")
    }

    @MainActor
    func testLocalEndpointWithoutReadyEvent() {
        let s = Store(cli: nil)
        s.config = ConfigShow(path: "p", exists: true, values: ["LOBO_DOMAIN": "lobo.x.cc", "LOBO_LOCAL_PORT": "9000"], set: [:])
        s.snap = Snapshot(pod: Pod(provider: "local", id: "local", status: "running", detail: "this Mac, q6", cost_per_hr: 0, started_at: nil, host_download_mbps: nil),
                          version: nil, status: nil, down: false, at: Date())
        XCTAssertTrue(s.isLocal)
        XCTAssertEqual(s.endpoint, "http://127.0.0.1:9000/v1")
        s.snap = nil
        XCTAssertEqual(s.endpoint, "https://lobo.x.cc/v1") // target cloud, no pod
    }

    func testDefaultTarget() throws {
        let models = try JSON.decoder.decode(ModelsInfo.self, from: fixture("models.json"))
        var empty = models
        empty.models = empty.models.map { var m = $0; m.on_disk = 0; return m }
        let keyed = ConfigShow(path: "p", exists: true, values: [:], set: ["RUNPOD_API_KEY": true])
        let bare = ConfigShow(path: "p", exists: true, values: [:], set: [:])
        let wantsLocal = ConfigShow(path: "p", exists: true, values: ["LOBO_PROVIDER": "local"], set: ["RUNPOD_API_KEY": true])
        XCTAssertEqual(Store.defaultTarget(saved: nil, config: wantsLocal, models: nil, localSupported: true), .local)
        XCTAssertEqual(Store.defaultTarget(saved: nil, config: bare, models: models, localSupported: true), .local)
        XCTAssertEqual(Store.defaultTarget(saved: nil, config: bare, models: empty, localSupported: true), .cloud)
        XCTAssertEqual(Store.defaultTarget(saved: nil, config: keyed, models: models, localSupported: true), .cloud)
        XCTAssertEqual(Store.defaultTarget(saved: .cloud, config: wantsLocal, models: models, localSupported: true), .cloud)
        XCTAssertEqual(Store.defaultTarget(saved: .local, config: keyed, models: nil, localSupported: true), .local)
        XCTAssertEqual(Store.defaultTarget(saved: .local, config: wantsLocal, models: models, localSupported: false), .cloud)
    }

    @MainActor
    func testTargetPersists() throws {
        let suite = "lobocode.tests.\(UUID().uuidString)"
        let d = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { d.removePersistentDomain(forName: suite) }
        let a = Store(cli: nil, defaults: d)
        XCTAssertNil(a.savedTarget)
        a.choose(.local)
        XCTAssertEqual(Store(cli: nil, defaults: d).target, .local)
        XCTAssertEqual(Store(cli: nil).target, .cloud) // previews never read the user's defaults
    }

    /// A fake `lobo` that logs each subcommand and answers from the fixtures.
    func fakeCLI() throws -> (CLI, URL) {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lobocode-fake-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let config = #"{"path":"p","exists":true,"values":{},"set":{"RUNPOD_API_KEY":true,"LOBO_DOMAIN":true,"LOBO_API_KEY":true,"CF_TUNNEL_TOKEN":true,"LOBO_BUCKET_URL":true}}"#
        try Data(config.utf8).write(to: dir.appendingPathComponent("config.json"))
        try fixture("models.json").write(to: dir.appendingPathComponent("models.json"))
        try fixture("status_off.json").write(to: dir.appendingPathComponent("status.json"))
        let log = dir.appendingPathComponent("calls.log")
        let script = """
        #!/bin/sh
        echo "$1" >> '\(log.path)'
        cd '\(dir.path)'
        case "$1" in config) cat config.json;; models) cat models.json;; status) cat status.json;; esac
        """
        let bin = dir.appendingPathComponent("lobo")
        try Data(script.utf8).write(to: bin)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: bin.path)
        return (CLI(binary: bin, configOverride: nil), log)
    }

    @MainActor
    func testModelsNotListedOnPlainRefresh() async throws {
        try XCTSkipUnless(ConfigShow.localSupported)
        let (cli, log) = try fakeCLI()
        let suite = "lobocode.tests.\(UUID().uuidString)"
        let d = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { d.removePersistentDomain(forName: suite) }
        func calls() -> [String] { ((try? String(contentsOf: log, encoding: .utf8)) ?? "").split(separator: "\n").map(String.init) }

        let s = Store(cli: cli, defaults: d)
        for _ in 0..<100 where calls().count < 3 { try await Task.sleep(nanoseconds: 50_000_000) }
        XCTAssertEqual(calls(), ["config", "models", "status"]) // start: models listed once
        XCTAssertEqual(s.phase, .off)
        XCTAssertNotNil(s.models)

        await s.refresh() // a plain poll tick
        XCTAssertEqual(calls().suffix(from: 3), ["status"])
        await s.refresh(models: true) // panel open, after down, after a failed boot
        XCTAssertEqual(calls().suffix(from: 4), ["status", "models"])
        await s.loadConfig() // settings open / save
        XCTAssertEqual(calls().suffix(from: 6), ["config", "models"])
    }

    func testSettingsChanges() {
        var f = SettingsView.Fields()
        f.plain = ["LOBO_DOMAIN": "lobo.x.cc", "LOBO_MIN_MBPS": "150", "LOBO_CTX": ""]
        f.secrets = ["RUNPOD_API_KEY": "", "VASTAI_API_KEY": "-", "CF_TUNNEL_TOKEN": " new-token "]
        let set = SettingsView.changes(f, current: ["LOBO_DOMAIN": "lobo.x.cc", "LOBO_MIN_MBPS": "100", "LOBO_CTX": "", "VASTAI_API_KEY": "3f9c…c0de"])
        XCTAssertEqual(set, ["LOBO_MIN_MBPS": "150", "VASTAI_API_KEY": "", "CF_TUNNEL_TOKEN": "new-token"])
        XCTAssertNil(SettingsView.validate(set))
        XCTAssertNotNil(SettingsView.validate(["LOBO_CTX": "100"]))
        XCTAssertNotNil(SettingsView.validate(["LOBO_DOMAIN": "https://x"]))
        XCTAssertNil(SettingsView.validate(["LOBO_CTX": "0"]))
    }

    func testSettingsLocal() {
        XCTAssertEqual(Store.defaultLocalPort, 8931) // the CLI's defaults
        XCTAssertEqual(Store.localPortRange, 1024...65534)
        XCTAssertEqual(SettingsView.validate(["LOBO_LOCAL_PORT": "80"]), "LOBO_LOCAL_PORT: whole number 1024-65534, or empty")
        XCTAssertNil(SettingsView.validate(["LOBO_LOCAL_PORT": ""]))
        XCTAssertNil(SettingsView.validate(["LOBO_LOCAL_PORT": "1024"]))
        XCTAssertNil(SettingsView.validate(["LOBO_LOCAL_PORT": "8931"]))
        XCTAssertNil(SettingsView.validate(["LOBO_LOCAL_PORT": "65534"]))
        for bad in ["1023", "65535", "0", "-1", "89.31", "port", " 8931"] {
            XCTAssertNotNil(SettingsView.validate(["LOBO_LOCAL_PORT": bad]), bad)
        }
        var f = SettingsView.Fields()
        f.plain = ["LOBO_WEIGHTS_DIR": " /Volumes/Extreme/_lobocode ", "LOBO_LOCAL_PORT": "8931"]
        XCTAssertEqual(SettingsView.changes(f, current: ["LOBO_LOCAL_PORT": "8931"]), ["LOBO_WEIGHTS_DIR": "/Volumes/Extreme/_lobocode"])
        XCTAssertNotNil(SettingsView.freeBytes("/tmp/lobocode-no-such-dir/weights")) // measured on the nearest existing parent
    }

    func testReadyNeedsAPIKey() {
        let keys = ["RUNPOD_API_KEY", "LOBO_DOMAIN", "CF_TUNNEL_TOKEN", "LOBO_BUCKET_URL"]
        var set = Dictionary(uniqueKeysWithValues: keys.map { ($0, true) })
        XCTAssertFalse(ConfigShow(path: "p", exists: true, values: [:], set: set).ready) // the CLI refuses without LOBO_API_KEY
        set["LOBO_API_KEY"] = true
        XCTAssertTrue(ConfigShow(path: "p", exists: true, values: [:], set: set).ready)
        XCTAssertTrue(ConfigShow(path: "p", exists: true, values: [:], set: set).cloudReady)
        // Local-only: an API key and no cloud keys is usable on Apple Silicon, never cloud-ready.
        let local = ConfigShow(path: "p", exists: true, values: [:], set: ["LOBO_API_KEY": true])
        XCTAssertFalse(local.cloudReady)
        XCTAssertEqual(local.ready, ConfigShow.localSupported)
    }

    func testFormat() {
        XCTAssertEqual(Fmt.duration(72), "1:12")
        XCTAssertEqual(Fmt.duration(8342), "2:19:02")
        XCTAssertEqual(Fmt.bar(0.5, width: 4), "▓▓░░")
        XCTAssertEqual(Fmt.bar(.nan, width: 2), "░░")
        XCTAssertEqual(Mask.newAPIKey().count, 51)
    }
}

extension AgentStatus {
    init(stage: String) {
        self.init(stage: stage, stage_detail: nil, download: nil, uptime_s: nil, idle_s: nil, kill_in_s: nil, kill_reason: nil,
                  expires_at: nil, gpu: nil, llama: nil, model: nil, ctx: nil)
    }
}

extension UpEvent {
    init(phase: String, detail: String? = nil, download: Download? = nil, ready: ReadyInfo? = nil) {
        self.init(phase: phase, detail: detail, download: download, ready: ready, done: nil, err: nil)
    }
}
