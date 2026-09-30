use chrono::{DateTime, Utc};
use lobo_proto::{GoTime, ReadyInfo};
use serde::Serialize;
use std::{fs::OpenOptions, io::Write, path::Path};

#[derive(Serialize)]
pub struct BootLine<'a> {
    pub at: GoTime,
    pub conns: i64,
    pub ready: &'a ReadyInfo,
    pub source: &'a str,
}

pub fn report_boot(
    w: &mut dyn Write,
    r: Option<&ReadyInfo>,
    source: &str,
    conns: i64,
    log: &Path,
    now: DateTime<Utc>,
) -> std::io::Result<()> {
    let Some(r) = r else {
        return Ok(());
    };
    let Some(t) = &r.timings else {
        return Ok(());
    };
    let host_net = if r.host_download_mbps > 0 {
        format!("{} Mbps", r.host_download_mbps)
    } else {
        "n/a".into()
    };
    let rows = [
        (
            "rent → container start",
            r.rent_s,
            format!(
                "{} {} ({}), attempt {}, host net {host_net}",
                r.provider, r.pod_id, r.detail, r.attempts
            ),
        ),
        ("bootstrap apt", t.bootstrap_apt_s, String::new()),
        ("bootstrap release zip", t.bootstrap_zip_s, String::new()),
        ("tunnel", t.tunnel_s, String::new()),
        ("gpu check", t.gpu_check_s, String::new()),
        (
            "model download",
            t.download_s,
            format!(
                "{:.0} MB/s, {} conns, {}",
                t.download_mbps, t.download_conns, t.download_source
            ),
        ),
        ("sha256 verify", t.verify_s, String::new()),
        ("load into VRAM", t.load_s, String::new()),
        (
            "total (incl. replaced pods)",
            r.elapsed_ns as f64 / 1e9,
            String::new(),
        ),
    ]
    .map(|(label, seconds, extra)| [format!("  {label}"), format!("{seconds:6.1}s"), extra]);
    let widths: Vec<usize> = (0..3)
        .map(|i| rows.iter().map(|r| r[i].chars().count()).max().unwrap_or(0) + 2)
        .collect();
    writeln!(w, "\nboot timings:")?;
    for row in rows {
        for (i, value) in row.iter().enumerate() {
            write!(
                w,
                "{value}{}",
                " ".repeat(widths[i] - value.chars().count())
            )?;
        }
        writeln!(w)?;
    }
    // As in the Go CLI, failure to append a local timing log does not fail a boot.
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log) {
        let line = BootLine {
            at: GoTime::from_utc(now),
            conns,
            ready: r,
            source,
        };
        if let Ok(mut bytes) = serde_json::to_vec(&line) {
            bytes.push(b'\n');
            let _ = file.write_all(&bytes);
        }
    }
    Ok(())
}
