use std::time::Duration;

/// Parse Go's positive duration syntax without rounding through floating point.
pub fn parse_go_duration(input: &str) -> Result<Duration, String> {
    let bad = || format!("invalid duration {input:?}");
    let mut s = input.strip_prefix('+').unwrap_or(input);
    if s == "0" {
        return Ok(Duration::ZERO);
    }
    if s.is_empty() || s.starts_with('-') {
        return Err(bad());
    }
    let mut ns: u128 = 0;
    while !s.is_empty() {
        let n = s
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .ok_or_else(bad)?;
        let (number, rest) = s.split_at(n);
        let (unit, scale) = [
            ("ns", 1u128),
            ("us", 1_000),
            ("µs", 1_000),
            ("μs", 1_000),
            ("ms", 1_000_000),
            ("s", 1_000_000_000),
            ("m", 60_000_000_000),
            ("h", 3_600_000_000_000),
        ]
        .into_iter()
        .find(|(u, _)| rest.starts_with(u))
        .ok_or_else(bad)?;
        let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
        if whole.is_empty() && fraction.is_empty() || !fraction.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(bad());
        }
        let whole = if whole.is_empty() {
            0
        } else {
            whole.parse::<u128>().map_err(|_| bad())?
        };
        // More than 18 decimal places cannot contribute a nanosecond, even for hours.
        let fraction = &fraction[..fraction.len().min(18)];
        let sub = if fraction.is_empty() {
            0
        } else {
            fraction.parse::<u128>().map_err(|_| bad())? * scale / 10u128.pow(fraction.len() as u32)
        };
        ns = ns
            .checked_add(whole.checked_mul(scale).ok_or_else(bad)?)
            .and_then(|v| v.checked_add(sub))
            .ok_or_else(bad)?;
        if ns > i64::MAX as u128 {
            return Err(bad());
        }
        s = &rest[unit.len()..];
    }
    Ok(Duration::from_nanos(ns as u64))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn go_duration_syntax() {
        for (s, nanos) in [
            ("12h", 43_200_000_000_000),
            ("1h30m", 5_400_000_000_000),
            ("1.5h", 5_400_000_000_000),
            ("90m", 5_400_000_000_000),
            ("300ms", 300_000_000),
            ("0", 0),
            ("2h45m30.5s", 9_930_500_000_000),
            (".5s", 500_000_000),
            ("1ns", 1),
            ("1µs", 1000),
            ("+1μs", 1000),
            ("1.000000001s", 1_000_000_001),
        ] {
            assert_eq!(parse_go_duration(s).unwrap().as_nanos(), nanos, "{s}");
        }
        for s in ["", "12", "1d", "-1h", "h", "1..2s", ".s", "2562048h", "1h2"] {
            assert!(parse_go_duration(s).is_err(), "{s}");
        }
    }
}
