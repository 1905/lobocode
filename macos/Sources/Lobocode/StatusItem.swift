import AppKit
import SwiftUI

/// Menu bar item: a small coloured square glyph (state) + a short mono status text.
struct MenuLabel: View {
    @ObservedObject var store: Store

    var body: some View {
        HStack(spacing: 4) {
            Image(nsImage: StatusIcon.image(phase: store.phase, progress: store.bootProgress))
            Text(store.menuText).font(.system(size: 12, weight: .medium, design: .monospaced)).monospacedDigit()
        }
    }
}

extension Store {
    /// 0…1 while booting: steps done, with the download filling its share.
    var bootProgress: Double {
        guard phase == .booting else { return phase == .ready ? 1 : 0 }
        let steps = bootSteps
        let n = Double(steps.count)
        guard let cur = currentStep, let i = steps.firstIndex(of: cur) else { return 0 }
        var p = Double(i) / n
        if cur == .download, let d = download, d.total > 0 { p += Double(d.bytes) / Double(d.total) / n }
        return p
    }

    /// OFF · rent · 42% · load · 45 t/s · 27m · FAIL
    var menuText: String {
        switch phase {
        case .loading: return "lobo"
        case .noConfig: return "setup"
        case .off: return "off"
        case .stopping: return "stop"
        case .failed: return "FAIL"
        case .booting:
            if currentStep == .download, let d = download, d.total > 0 { return "\(Int(Double(d.bytes) / Double(d.total) * 100))%" }
            return currentStep?.label(local: isLocal) ?? "boot"
        case .ready:
            let st = snap?.status
            if (st?.llama?.requests_processing ?? 0) > 0, let t = st?.llama?.gen_tps, t > 0 { return "\(Int(t)) t/s" }
            if let k = st?.kill_in_s {
                let left = max(0, Double(k) - (snap?.at.map { now.timeIntervalSince($0) } ?? 0))
                return "\(Int((left / 60).rounded(.up)))m"
            }
            return "run"
        }
    }
}

enum StatusIcon {
    static func nsColor(_ hex: UInt32) -> NSColor {
        NSColor(srgbRed: CGFloat(hex >> 16 & 0xFF) / 255, green: CGFloat(hex >> 8 & 0xFF) / 255, blue: CGFloat(hex & 0xFF) / 255, alpha: 1)
    }

    /// 16×16 non-template image, so the state colour survives in the menu bar.
    static func image(phase: Phase, progress: Double) -> NSImage {
        let color: NSColor
        switch phase {
        case .ready: color = nsColor(0x2EE57A)
        case .booting, .stopping, .loading: color = nsColor(0x00C8E6)
        case .failed: color = nsColor(0xFF4D5E)
        case .noConfig: color = nsColor(0xFFB020)
        case .off: color = NSColor.secondaryLabelColor
        }
        let img = NSImage(size: NSSize(width: 16, height: 16), flipped: false) { _ in
            let outer = NSRect(x: 1.5, y: 1.5, width: 13, height: 13)
            let frame = NSBezierPath(roundedRect: outer, xRadius: 3, yRadius: 3)
            frame.lineWidth = 1.5
            color.setStroke()
            frame.stroke()
            let inner = outer.insetBy(dx: 3, dy: 3)
            switch phase {
            case .ready:
                color.setFill()
                NSBezierPath(roundedRect: inner, xRadius: 1, yRadius: 1).fill()
            case .booting, .stopping, .loading:
                // Fill from the bottom as the boot advances (at least a sliver so it never looks off).
                let h = inner.height * CGFloat(max(0.12, min(1, progress)))
                color.withAlphaComponent(0.9).setFill()
                NSBezierPath(rect: NSRect(x: inner.minX, y: inner.minY, width: inner.width, height: h)).fill()
            case .failed:
                color.setStroke()
                let x = NSBezierPath()
                x.move(to: NSPoint(x: inner.minX, y: inner.minY)); x.line(to: NSPoint(x: inner.maxX, y: inner.maxY))
                x.move(to: NSPoint(x: inner.minX, y: inner.maxY)); x.line(to: NSPoint(x: inner.maxX, y: inner.minY))
                x.lineWidth = 1.5
                x.stroke()
            case .noConfig:
                color.setFill()
                NSBezierPath(ovalIn: NSRect(x: inner.midX - 1.5, y: inner.midY - 1.5, width: 3, height: 3)).fill()
            case .off:
                break
            }
            return true
        }
        img.isTemplate = false
        return img
    }
}
