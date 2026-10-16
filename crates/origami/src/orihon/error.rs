use std::{
    fmt, io,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub enum Error {
    Invalid(String),
    Refusal { code: &'static str, message: String },
    Database(sqlx::Error),
    Io { path: PathBuf, source: io::Error },
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => write!(formatter, "invalid Orihon request: {message}"),
            Self::Refusal { code, message } => {
                write!(formatter, "Orihon refusal ({code}): {message}")
            }
            Self::Database(_) => formatter.write_str("Orihon database operation failed"),
            Self::Io { path, source } => {
                write!(formatter, "Orihon I/O at {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(source) => Some(source),
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<sqlx::Error> for Error {
    fn from(source: sqlx::Error) -> Self {
        Self::Database(source)
    }
}

pub(super) fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}

pub(super) fn refusal(code: &'static str, message: impl Into<String>) -> Error {
    Error::Refusal {
        code,
        message: message.into(),
    }
}

pub(super) fn io(path: &Path, source: io::Error) -> Error {
    Error::Io {
        path: path.to_owned(),
        source,
    }
}

pub(super) fn malformed(path: &Path, line: usize) -> Error {
    refusal(
        "malformed_book",
        format!(
            "{} at line {line}; existing bytes remain unchanged",
            path.display()
        ),
    )
}
