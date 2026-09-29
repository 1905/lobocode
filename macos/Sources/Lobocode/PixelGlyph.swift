import SwiftUI

/// Pixel glyph: rows of "#" (filled) and "." (empty), drawn as square cells with a small gap.
struct PixelGlyph: View {
    let rows: [String]
    let cell: CGFloat
    var fill: AnyShapeStyle = AnyShapeStyle(Theme.copper)

    var body: some View {
        Rectangle().fill(fill).mask(grid)
            .frame(width: CGFloat(rows[0].count) * cell, height: CGFloat(rows.count) * cell)
    }

    private var grid: some View {
        VStack(spacing: 0) {
            ForEach(rows.indices, id: \.self) { r in
                HStack(spacing: 0) {
                    ForEach(Array(rows[r].enumerated()), id: \.offset) { _, c in
                        Rectangle().fill(c == "#" ? Color.white : Color.clear)
                            .padding(cell * 0.05)
                            .frame(width: cell, height: cell)
                    }
                }
            }
        }
    }
}

enum Glyphs {
    static let L = ["#....", "#....", "#....", "#....", "#....", "#....", "#####"]
    static let C = [".####", "#....", "#....", "#....", "#....", "#....", ".####"]
}
