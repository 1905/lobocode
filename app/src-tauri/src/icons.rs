use crate::types::Phase;
use tiny_skia::{FillRule, Paint, Path, PathBuilder, Pixmap, Rect, Stroke, Transform};
fn paint(hex: u32, alpha: u8) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8, alpha);
    p
}
fn rounded(x: f32, y: f32, w: f32, h: f32, r: f32) -> Path {
    let k = r * 0.552_284_8;
    let mut b = PathBuilder::new();
    b.move_to(x + r, y);
    b.line_to(x + w - r, y);
    b.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    b.line_to(x + w, y + h - r);
    b.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    b.line_to(x + r, y + h);
    b.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    b.line_to(x, y + r);
    b.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    b.close();
    b.finish().unwrap()
}
pub fn tray_icon(phase: &Phase, progress: f64) -> (Vec<u8>, u32, u32, bool) {
    let mut pix = Pixmap::new(32, 32).unwrap();
    let color = match phase {
        Phase::Ready => 0x2EE57A,
        Phase::Failed { .. } => 0xFF4D5E,
        Phase::NoConfig => 0xFFB020,
        Phase::Off => 0,
        _ => 0x00C8E6,
    };
    let p = paint(color, 255);
    let stroke = Stroke {
        width: 3.0,
        ..Default::default()
    };
    pix.stroke_path(
        &rounded(3.0, 3.0, 26.0, 26.0, 6.0),
        &p,
        &stroke,
        Transform::identity(),
        None,
    );
    match phase {
        Phase::Ready => pix.fill_path(
            &rounded(9.0, 9.0, 14.0, 14.0, 2.0),
            &p,
            FillRule::Winding,
            Transform::identity(),
            None,
        ),
        Phase::Booting | Phase::Stopping | Phase::Loading => {
            let f = if progress.is_finite() {
                progress.clamp(0.12, 1.0)
            } else {
                0.12
            } as f32;
            let h = 14.0 * f;
            pix.fill_rect(
                Rect::from_xywh(9.0, 23.0 - h, 14.0, h).unwrap(),
                &paint(color, 230),
                Transform::identity(),
                None,
            );
        }
        Phase::Failed { .. } => {
            let mut b = PathBuilder::new();
            b.move_to(9.0, 9.0);
            b.line_to(23.0, 23.0);
            b.move_to(9.0, 23.0);
            b.line_to(23.0, 9.0);
            pix.stroke_path(
                &b.finish().unwrap(),
                &p,
                &stroke,
                Transform::identity(),
                None,
            );
        }
        Phase::NoConfig => {
            let mut b = PathBuilder::new();
            b.push_circle(16.0, 16.0, 3.0);
            pix.fill_path(
                &b.finish().unwrap(),
                &p,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
        Phase::Off => {}
    }
    (pix.take(), 32, 32, *phase == Phase::Off)
}
pub fn app_icon_1024() -> Pixmap {
    let mut pix = Pixmap::new(1024, 1024).unwrap();
    let tile = rounded(100.0, 100.0, 824.0, 824.0, 185.0);
    pix.fill_path(
        &tile,
        &paint(0x0B0D10, 255),
        FillRule::Winding,
        Transform::identity(),
        None,
    );
    pix.stroke_path(
        &tile,
        &paint(0x1F252D, 255),
        &Stroke {
            width: 10.0,
            ..Default::default()
        },
        Transform::identity(),
        None,
    );
    for (x, color, glyph) in [
        (
            232.0,
            0x00E5FF,
            [
                "#....", "#....", "#....", "#....", "#....", "#....", "#####",
            ],
        ),
        (
            532.0,
            0xFF2BD6,
            [
                ".####", "#....", "#....", "#....", "#....", "#....", ".####",
            ],
        ),
    ] {
        for (y, row) in glyph.iter().enumerate() {
            for (col, ch) in row.chars().enumerate() {
                if ch == '#' {
                    pix.fill_rect(
                        Rect::from_xywh(
                            x + col as f32 * 52.0 + 2.6,
                            330.0 + y as f32 * 52.0 + 2.6,
                            46.8,
                            46.8,
                        )
                        .unwrap(),
                        &paint(color, 255),
                        Transform::identity(),
                        None,
                    );
                }
            }
        }
    }
    pix
}
#[cfg(test)]
mod tests {
    use super::*;
    fn pixel(phase: Phase, progress: f64, x: usize, y: usize) -> [u8; 4] {
        let (data, _, _, _) = tray_icon(&phase, progress);
        data[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4]
            .try_into()
            .unwrap()
    }
    #[test]
    fn phases_draw_expected_pixels() {
        assert_eq!(pixel(Phase::Ready, 0.0, 16, 16), [46, 229, 122, 255]);
        assert_eq!(
            pixel(
                Phase::Failed {
                    message: String::new()
                },
                0.0,
                16,
                16
            ),
            [255, 77, 94, 255]
        );
        assert_eq!(pixel(Phase::NoConfig, 0.0, 16, 16), [255, 176, 32, 255]);
        assert_eq!(pixel(Phase::NoConfig, 0.0, 9, 9)[3], 0);
        assert!(tray_icon(&Phase::Off, 0.0).3);
        assert_eq!(pixel(Phase::Off, 0.0, 16, 16)[3], 0);
        assert_eq!(pixel(Phase::Off, 0.0, 16, 3), [0, 0, 0, 255]);
        assert_eq!(pixel(Phase::Booting, 0.0, 16, 10)[3], 0);
        assert!(pixel(Phase::Booting, 0.0, 16, 22)[3] > 0);
        assert!(pixel(Phase::Booting, 1.0, 16, 10)[3] > 0);
    }
    #[test]
    fn app_glyph_and_transparency() {
        let p = app_icon_1024();
        assert_eq!(p.pixel(10, 10).unwrap().alpha(), 0);
        assert_eq!(p.pixel(512, 110).unwrap().demultiply().red(), 11);
        assert_eq!(p.pixel(258, 356).unwrap().demultiply().blue(), 255);
        assert_eq!(p.pixel(610, 356).unwrap().demultiply().red(), 255);
        assert_eq!(p.pixel(310, 356).unwrap().demultiply().red(), 11);
    }
}
