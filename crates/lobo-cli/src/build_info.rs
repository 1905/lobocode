pub const VERSION: &str = match option_env!("LOBO_VERSION") {
    Some(s) => s,
    None => "dev",
};
pub const COMMIT: &str = match option_env!("LOBO_COMMIT") {
    Some(s) => s,
    None => "none",
};
pub const DATE: &str = match option_env!("LOBO_DATE") {
    Some(s) => s,
    None => "unknown",
};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn build_values() {
        assert_eq!(VERSION, option_env!("LOBO_VERSION").unwrap_or("dev"));
        assert_eq!(COMMIT, option_env!("LOBO_COMMIT").unwrap_or("none"));
        assert_eq!(DATE, option_env!("LOBO_DATE").unwrap_or("unknown"));
    }
}
