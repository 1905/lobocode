pub fn duration(secs: f64) -> String {
    let t = if secs.is_finite() {
        secs.round().max(0.0) as u64
    } else {
        0
    };
    if t >= 3600 {
        format!("{}:{:02}:{:02}", t / 3600, t / 60 % 60, t % 60)
    } else {
        format!("{}:{:02}", t / 60, t % 60)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn shared_duration_cases() {
        let v: serde_json::Value =
            serde_json::from_str(include_str!("../../ui/src/fixtures/time_cases.json")).unwrap();
        for case in v["duration"].as_array().unwrap() {
            assert_eq!(
                super::duration(case[0].as_f64().unwrap()),
                case[1].as_str().unwrap()
            );
        }
    }
}
