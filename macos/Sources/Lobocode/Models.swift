import Foundation

// Mirrors of the lobo CLI JSON (`status --json`, `up --json`, `config show --json`, `models --json`). Every field the
// app does not need is left out; every field the CLI may omit is optional.

struct Pod: Decodable, Equatable {
    var provider: String
    var id: String
    var status: String
    var detail: String?
    var cost_per_hr: Double
    var started_at: Date?
    var host_download_mbps: Int?
}

struct Download: Decodable, Equatable {
    var bytes: Int64
    var total: Int64
    var mbps: Double
    var verifying: Bool?
    var source: String?
}

struct GPU: Decodable, Equatable {
    var name: String
    var vram_used_mb: Int
    var vram_total_mb: Int
    var util_pct: Int
}

struct Llama: Decodable, Equatable {
    var requests_processing: Int
    var requests_deferred: Int
    var prompt_tokens_total: Int
    var gen_tokens_total: Int
    var prompt_tps: Double
    var gen_tps: Double
}

struct AgentStatus: Decodable, Equatable {
    var stage: String
    var stage_detail: String?
    var download: Download?
    var uptime_s: Int?
    var idle_s: Int?
    var kill_in_s: Int?
    var kill_reason: String?
    var expires_at: Date?
    var gpu: GPU?
    var llama: Llama?
    var model: String?
    var ctx: Int?
}

struct Release: Decodable, Equatable {
    var version: String
    var git_sha: String?
}

/// `lobo status --json`
struct Snapshot: Decodable, Equatable {
    var pod: Pod?
    var version: Release?
    var status: AgentStatus?
    var down: Bool
    var at: Date?
}

struct ReadyInfo: Decodable, Equatable {
    var pod_id: String
    var provider: String
    var detail: String
    var attempts: Int
    var url: String
    var version: String
    var usd_per_h: Double
    var elapsed_ns: Int64
}

/// One line of `lobo up --json`.
struct UpEvent: Decodable, Equatable {
    var phase: String
    var detail: String?
    var download: Download?
    var ready: ReadyInfo?
    var done: Bool?
    var err: String?
}

/// `lobo config show --json`: values are masked for secrets; `set` says which keys have a value.
struct ConfigShow: Decodable, Equatable {
    var path: String
    var exists: Bool
    var values: [String: String]
    var set: [String: Bool]

    func has(_ key: String) -> Bool { set[key] ?? false }
    var providers: [String] {
        var p: [String] = []
        if has("RUNPOD_API_KEY") { p.append("runpod") }
        if has("VASTAI_API_KEY") { p.append("vast") }
        return p
    }
    var defaultProvider: String {
        let want = values["LOBO_PROVIDER"] ?? ""
        if providers.contains(want) { return want }
        return providers.first ?? "runpod"
    }
    var defaultModel: String { values["LOBO_MODEL"] == "q6" ? "q6" : "q8" }
    /// Every key `lobo up` on runpod/vast needs (the CLI's RequireCloud + LOBO_API_KEY).
    var cloudReady: Bool {
        exists && !providers.isEmpty && ["LOBO_DOMAIN", "LOBO_API_KEY", "CF_TUNNEL_TOKEN", "LOBO_BUCKET_URL"].allSatisfy(has)
    }
    /// Usable at all: the cloud keys, or on Apple Silicon just LOBO_API_KEY (local needs no cloud key).
    var ready: Bool { cloudReady || (ConfigShow.localSupported && exists && has("LOBO_API_KEY")) }

    /// llama.cpp local mode runs on Apple Silicon only (the CLI's local.Supported).
    static var localSupported: Bool {
        #if arch(arm64)
        return true
        #else
        return false
        #endif
    }
}

/// `lobo models --json`: catalog models in the local weights folder.
struct ModelsInfo: Decodable, Equatable {
    var weights: String
    var free_bytes: Int64
    var models: [LocalModel]
    var runtime: Runtime?

    struct Runtime: Decodable, Equatable {
        var version: String
        var present: Bool
    }

    func model(_ id: String) -> LocalModel? { models.first { $0.id == id } }
}

struct LocalModel: Decodable, Equatable, Identifiable {
    var id: String
    var file: String
    var size: Int64
    var on_disk: Int64
    var verified: Bool

    enum State: Equatable { case onDisk, partial(Double), missing }

    var state: State {
        if size > 0, on_disk >= size { return .onDisk }
        if on_disk > 0, size > 0 { return .partial(Double(on_disk) / Double(size)) }
        return .missing
    }
}

enum JSON {
    /// Go writes RFC 3339 with or without fractional seconds (and up to nanoseconds).
    static let decoder: JSONDecoder = {
        let d = JSONDecoder()
        d.dateDecodingStrategy = .custom { dec in
            let s = try dec.singleValueContainer().decode(String.self)
            if let t = parseDate(s) { return t }
            throw DecodingError.dataCorrupted(.init(codingPath: dec.codingPath, debugDescription: "bad date \(s)"))
        }
        return d
    }()

    static func parseDate(_ s: String) -> Date? {
        // Trim fractional seconds to milliseconds: ISO8601DateFormatter can't read nanoseconds.
        var str = s
        if let dot = str.firstIndex(of: "."), let end = str[dot...].firstIndex(where: { $0 == "Z" || $0 == "+" || $0 == "-" }) {
            let frac = str[str.index(after: dot)..<end].prefix(3)
            str = String(str[..<dot]) + "." + frac.padding(toLength: 3, withPad: "0", startingAt: 0) + String(str[end...])
        }
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let d = f.date(from: str) { return d }
        f.formatOptions = [.withInternetDateTime]
        return f.date(from: str)
    }
}
