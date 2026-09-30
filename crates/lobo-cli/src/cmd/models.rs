use std::{io::Write, path::Path};
pub fn write_models(w: &mut dyn Write, weights: &Path, json: bool) -> anyhow::Result<()> {
    let listing = lobo_core::local::list(weights)?;
    if json {
        serde_json::to_writer(&mut *w, &listing)?;
        writeln!(w)?;
        return Ok(());
    }
    for m in &listing.models {
        writeln!(w, "{}", row(m))?;
    }
    writeln!(
        w,
        "weights {} ({:.1} GB free)",
        listing.weights,
        listing.free_bytes as f64 / 1e9
    )?;
    Ok(())
}
fn row(m: &lobo_proto::ModelState) -> String {
    let (state, verified) = if m.on_disk == m.size {
        (
            "on disk".into(),
            if m.verified {
                "verified"
            } else {
                "not verified"
            },
        )
    } else if m.on_disk > m.size {
        ("oversize".into(), "")
    } else if m.on_disk > 0 {
        (format!("partial {}%", m.on_disk * 100 / m.size), "")
    } else {
        ("missing".into(), "")
    };
    format!(
        "{:<4} {:6.1} GB  {:<12} {}",
        m.id,
        m.size as f64 / 1e9,
        state,
        verified
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn models_row_format() {
        for (on_disk, verified, state, note) in [
            (0, false, "missing", ""),
            (500_000_000, false, "partial 50%", ""),
            (1_000_000_001, false, "oversize", ""),
            (1_000_000_000, false, "on disk", "not verified"),
            (1_000_000_000, true, "on disk", "verified"),
        ] {
            let m = lobo_proto::ModelState {
                id: "q8".into(),
                size: 1_000_000_000,
                on_disk,
                verified,
                ..Default::default()
            };
            assert_eq!(row(&m), format!("q8      1.0 GB  {state:<12} {note}"));
        }
    }
}
