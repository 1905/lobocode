use std::collections::BTreeMap;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Config(String),
    #[error("config: {}",fmt_bad(.0))]
    Defaults(BTreeMap<String, String>),
    #[error("provider: no gpu capacity: {0}")]
    NoCapacity(String),
    #[error("provider: instance not found")]
    NotFound,
    #[error("vast: account has no credit (top up at https://cloud.vast.ai/billing/)")]
    NoCredit,
    #[error("vast: offer rejected: {0}")]
    Rejected(String),
    #[error("{0}")]
    AlreadyRunning(String),
    #[error("{0}")]
    Api(String),
    #[error("{0}")]
    CreateRejected(String),
    #[error("unresolved create on {provider}, boot {boot_id}: {detail}")]
    UnresolvedCreate {
        provider: String,
        boot_id: String,
        detail: String,
    },
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Agent(#[from] lobo_agent::Error),
    #[error("{0}")]
    Local(String),
    #[error("{0}")]
    Release(String),
    #[error("cancelled")]
    Cancelled,
    #[error("{}",join_lines(.0))]
    Multi(Vec<Error>),
    #[error("{0}")]
    Other(String),
}
pub type Result<T> = std::result::Result<T, Error>;
fn fmt_bad(bad: &BTreeMap<String, String>) -> String {
    bad.iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect::<Vec<_>>()
        .join("; ")
}
fn join_lines(errors: &[Error]) -> String {
    errors
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}
impl Error {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Config(_) | Self::Defaults(_) => "config",
            Self::NoCapacity(_) => "no_capacity",
            Self::NotFound => "not_found",
            Self::NoCredit => "no_credit",
            Self::Rejected(_) | Self::CreateRejected(_) => "rejected",
            Self::AlreadyRunning(_) => "already_running",
            Self::Api(_) => "provider_api",
            Self::UnresolvedCreate { .. } => "unresolved_create",
            Self::Http(_) => "network",
            Self::Io(_) => "io",
            Self::Json(_) => "json",
            Self::Agent(_) => "agent",
            Self::Local(_) => "local",
            Self::Release(_) => "release",
            Self::Cancelled => "cancelled",
            Self::Multi(_) => "multi",
            Self::Other(_) => "other",
        }
    }
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound)
    }
    pub fn is_no_capacity(&self) -> bool {
        matches!(self, Self::NoCapacity(_))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn kind_table() {
        for (e, k) in [
            (Error::Config("x".into()), "config"),
            (Error::Defaults(BTreeMap::new()), "config"),
            (Error::NoCapacity("x".into()), "no_capacity"),
            (Error::NotFound, "not_found"),
            (Error::NoCredit, "no_credit"),
            (Error::Rejected("x".into()), "rejected"),
            (Error::CreateRejected("x".into()), "rejected"),
            (Error::AlreadyRunning("x".into()), "already_running"),
            (Error::Api("x".into()), "provider_api"),
            (
                Error::UnresolvedCreate {
                    provider: "runpod".into(),
                    boot_id: "b".into(),
                    detail: "unknown".into(),
                },
                "unresolved_create",
            ),
            (Error::Io(std::io::Error::other("x")), "io"),
            (
                Error::Json(serde_json::from_str::<String>("{").unwrap_err()),
                "json",
            ),
            (Error::Agent(lobo_agent::Error::Cancelled), "agent"),
            (Error::Local("x".into()), "local"),
            (Error::Release("x".into()), "release"),
            (Error::Cancelled, "cancelled"),
            (Error::Multi(vec![]), "multi"),
            (Error::Other("x".into()), "other"),
        ] {
            assert_eq!(e.kind(), k);
        }
        let e = reqwest::Client::new().get("bad").build().unwrap_err();
        assert_eq!(Error::Http(e).kind(), "network");
    }
    #[test]
    fn defaults_display_sorted() {
        assert_eq!(
            Error::Defaults(
                [
                    ("LOBO_MODEL".into(), "x".into()),
                    ("LOBO_CTX".into(), "y".into())
                ]
                .into()
            )
            .to_string(),
            "config: LOBO_CTX: y; LOBO_MODEL: x"
        );
    }
    #[test]
    fn multi_joins_with_newline() {
        assert_eq!(
            Error::Multi(vec![Error::Other("a".into()), Error::Other("b".into())]).to_string(),
            "a\nb"
        );
    }
    #[test]
    fn no_capacity_display_contains_go_text() {
        assert!(
            Error::NoCapacity("none".into())
                .to_string()
                .contains("no gpu capacity")
        );
    }
}
