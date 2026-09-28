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

    func testReadyNeedsAPIKey() {
        let keys = ["RUNPOD_API_KEY", "LOBO_DOMAIN", "CF_TUNNEL_TOKEN", "LOBO_BUCKET_URL"]
        var set = Dictionary(uniqueKeysWithValues: keys.map { ($0, true) })
        XCTAssertFalse(ConfigShow(path: "p", exists: true, values: [:], set: set).ready) // the CLI refuses without LOBO_API_KEY
        set["LOBO_API_KEY"] = true
        XCTAssertTrue(ConfigShow(path: "p", exists: true, values: [:], set: set).ready)
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
