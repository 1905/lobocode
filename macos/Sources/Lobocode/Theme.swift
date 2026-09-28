import SwiftUI

/// "Crack intro, but grown up": one dark surface, one mono face, phosphor green + one copper gradient.
enum Theme {
    static let bg = Color(hex: 0x0B0D10)
    static let card = Color(hex: 0x12161B)
    static let line = Color(hex: 0x1F252D)
    static let text = Color(hex: 0xD6DEE8)
    static let dim = Color(hex: 0x6B7685)
    static let faint = Color(hex: 0x3A424D)
    static let green = Color(hex: 0x39FF88)
    static let cyan = Color(hex: 0x00E5FF)
    static let magenta = Color(hex: 0xFF2BD6)
    static let amber = Color(hex: 0xFFB020)
    static let red = Color(hex: 0xFF4D5E)
    static let copper = LinearGradient(colors: [cyan, Color(hex: 0x8A7BFF), magenta], startPoint: .leading, endPoint: .trailing)

    static func mono(_ size: CGFloat, _ weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight, design: .monospaced)
    }

    static func color(for phase: Phase) -> Color {
        switch phase {
        case .ready: return green
        case .booting, .loading, .stopping: return cyan
        case .failed: return red
        case .noConfig: return amber
        case .off: return dim
        }
    }
}

extension Color {
    init(hex: UInt32, alpha: Double = 1) {
        self.init(.sRGB, red: Double(hex >> 16 & 0xFF) / 255, green: Double(hex >> 8 & 0xFF) / 255, blue: Double(hex & 0xFF) / 255, opacity: alpha)
    }
}

/// Block-letter logo, gradient filled.
struct Logo: View {
    static let art = """
    █     ▄▀▀▀▄ █▀▀▀▄ ▄▀▀▀▄ ▄▀▀▀▀ ▄▀▀▀▄ █▀▀▀▄ █▀▀▀▀
    █     █   █ █▀▀▀▄ █   █ █     █   █ █   █ █▀▀▀
    ▀▀▀▀   ▀▀▀  ▀▀▀▀   ▀▀▀   ▀▀▀▀  ▀▀▀  ▀▀▀▀  ▀▀▀▀▀
    """
    var body: some View {
        Text(Logo.art)
            .font(.system(size: 9, weight: .bold, design: .monospaced))
            .lineSpacing(-1)
            .foregroundStyle(Theme.copper)
            .fixedSize()
    }
}

/// 2 px copper bar; a bright slice sweeps across while `active` (static otherwise and with Reduce Motion).
struct RasterBar: View {
    var active: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        GeometryReader { g in
            ZStack(alignment: .leading) {
                Rectangle().fill(Theme.copper).opacity(0.35)
                if active && !reduceMotion {
                    TimelineView(.animation) { t in
                        let x = CGFloat(t.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 1.6) / 1.6)
                        Rectangle()
                            .fill(LinearGradient(colors: [.clear, .white.opacity(0.9), .clear], startPoint: .leading, endPoint: .trailing))
                            .frame(width: g.size.width * 0.25)
                            .offset(x: x * g.size.width * 1.25 - g.size.width * 0.25)
                    }
                }
            }
        }
        .frame(height: 2)
        .clipped()
    }
}

/// Faint scanlines, header only.
struct Scanlines: View {
    var body: some View {
        Canvas { ctx, size in
            var y: CGFloat = 0
            while y < size.height {
                ctx.fill(Path(CGRect(x: 0, y: y, width: size.width, height: 1)), with: .color(.white.opacity(0.03)))
                y += 3
            }
        }
        .allowsHitTesting(false)
    }
}

/// Text that scrambles for ~250 ms when its value changes, then settles.
struct GlitchText: View {
    let text: String
    var font: Font = Theme.mono(11, .bold)
    var color: Color = Theme.text
    @State private var shown: String = ""
    @State private var gen = 0
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    private static let noise = Array("!<>-_\\/[]{}=+*^?#%&01")

    var body: some View {
        Text(shown.isEmpty ? text : shown)
            .font(font)
            .foregroundColor(color)
            .onChange(of: text) { new in scramble(to: new) }
    }

    private func scramble(to target: String) {
        guard !reduceMotion else { shown = target; return }
        gen += 1
        let mine = gen
        let chars = Array(target)
        for frame in 0...8 {
            DispatchQueue.main.asyncAfter(deadline: .now() + Double(frame) * 0.03) {
                guard mine == gen else { return }
                let settled = Int(Double(chars.count) * Double(frame) / 8)
                shown = String(chars.enumerated().map { i, c in i < settled || c == " " ? c : GlitchText.noise.randomElement()! })
                if frame == 8 { shown = target }
            }
        }
    }
}

/// Blinking block cursor.
struct Cursor: View {
    var color: Color = Theme.cyan
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    var body: some View {
        TimelineView(.periodic(from: .now, by: 0.5)) { t in
            let on = reduceMotion || Int(t.date.timeIntervalSinceReferenceDate * 2) % 2 == 0
            Text("█").font(Theme.mono(10)).foregroundColor(color.opacity(on ? 1 : 0))
        }
    }
}

/// `[ START ]` style button: mono caps, 1 px border, fills on hover.
struct BracketButtonStyle: ButtonStyle {
    var color: Color = Theme.green
    var wide = false
    func makeBody(configuration: Configuration) -> some View {
        Styled(configuration: configuration, color: color, wide: wide)
    }

    struct Styled: View {
        let configuration: Configuration
        let color: Color
        let wide: Bool
        @State private var hover = false
        @Environment(\.isEnabled) private var enabled

        var body: some View {
            HStack(spacing: 0) {
                Text("[ ")
                configuration.label
                Text(" ]")
            }
            .font(Theme.mono(12, .bold))
            .foregroundColor(enabled ? (hover ? Theme.bg : color) : Theme.faint)
            .frame(maxWidth: wide ? .infinity : nil)
            .padding(.vertical, 7)
            .padding(.horizontal, 10)
            .background(RoundedRectangle(cornerRadius: 4).fill(hover && enabled ? color : color.opacity(configuration.isPressed ? 0.25 : 0.06)))
            .overlay(RoundedRectangle(cornerRadius: 4).stroke(enabled ? color.opacity(0.7) : Theme.faint, lineWidth: 1))
            .shadow(color: hover && enabled ? color.opacity(0.45) : .clear, radius: 8)
            .contentShape(Rectangle())
            .onHover { hover = $0 }
            .animation(.easeOut(duration: 0.12), value: hover)
        }
    }
}

/// Small text link in the footer and next to values.
struct LinkButtonStyle: ButtonStyle {
    var color: Color = Theme.dim
    func makeBody(configuration: Configuration) -> some View {
        Styled(configuration: configuration, color: color)
    }

    struct Styled: View {
        let configuration: Configuration
        let color: Color
        @State private var hover = false
        var body: some View {
            configuration.label
                .font(Theme.mono(10))
                .foregroundColor(hover ? Theme.text : color)
                .onHover { hover = $0 }
        }
    }
}

/// `label [a] b` option picker.
struct BracketPicker: View {
    let label: String
    let options: [String]
    @Binding var value: String

    var body: some View {
        HStack(spacing: 6) {
            Text("> \(label)").foregroundColor(Theme.dim).frame(width: 84, alignment: .leading)
            ForEach(options, id: \.self) { o in
                Button { value = o } label: {
                    Text(o == value ? "[\(o)]" : " \(o) ")
                        .foregroundColor(o == value ? Theme.green : Theme.dim)
                }
                .buttonStyle(.plain)
            }
            Spacer()
        }
        .font(Theme.mono(12))
    }
}
