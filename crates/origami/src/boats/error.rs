
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum BoatError {
    /// The request or the row cannot become a boat. The message is
    /// caller-facing and stable.
    Invalid(String),
    /// PostgreSQL refused or failed. The original error rides along.
    Database(sqlx::Error),
}

pub type BoatResult<T> = Result<T, BoatError>;

impl fmt::Display for BoatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => formatter.write_str(message),
            Self::Database(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for BoatError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Invalid(_) => None,
            Self::Database(error) => Some(error),
        }
    }
}

impl From<sqlx::Error> for BoatError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}
