fn main() {
    use lobocode_app::{icons, types::Phase};
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("output directory"));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, phase, progress) in [
        ("loading", Phase::Loading, 0.0),
        ("setup", Phase::NoConfig, 0.0),
        ("off", Phase::Off, 0.0),
        ("boot_0", Phase::Booting, 0.0),
        ("boot_50", Phase::Booting, 0.5),
        ("ready", Phase::Ready, 1.0),
        ("stop", Phase::Stopping, 0.0),
        (
            "fail",
            Phase::Failed {
                message: String::new(),
            },
            0.0,
        ),
    ] {
        let (data, w, h, _) = icons::tray_icon(&phase, progress);
        let p =
            tiny_skia::Pixmap::from_vec(data, tiny_skia::IntSize::from_wh(w, h).unwrap()).unwrap();
        p.save_png(dir.join(format!("tray_{name}.png"))).unwrap();
    }
    icons::app_icon_1024()
        .save_png(dir.join("icon_1024.png"))
        .unwrap();
}
