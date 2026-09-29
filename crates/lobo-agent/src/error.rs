#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Permanent(String),
    #[error("context canceled")]
    Cancelled,
    #[error("{0}")]
    Msg(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Error {
    pub fn is_permanent(&self) -> bool {
        matches!(self, Self::Permanent(_))
    }

    pub fn msg(s: impl Into<String>) -> Self {
        Self::Msg(s.into())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
