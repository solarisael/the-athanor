//! The operator axis: which human a memory belongs to and how far up the
//! organisation it may travel. The Host names the operator; callers only pick
//! the scope.

use std::fmt;

use crate::error::DomainError;
use crate::room::is_safe_key;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct OperatorKey(String);

impl OperatorKey {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        if !is_safe_key(&value) {
            return Err(DomainError::InvalidOperatorKey(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OperatorKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// How far a memory travels from its writer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryScope {
    /// Every reader in the House.
    Company,
    /// The writer and readers above the writer in the writer's unit.
    Up,
    /// The writer only.
    Private,
}

impl MemoryScope {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "company" => Ok(Self::Company),
            "up" => Ok(Self::Up),
            "private" => Ok(Self::Private),
            other => Err(DomainError::InvalidMemoryScope(other.into())),
        }
    }

    /// A memory without an operator predates the axis and stays company-wide;
    /// a memory with one travels up unless the caller says otherwise.
    pub const fn default_for(operator: Option<&OperatorKey>) -> Self {
        match operator {
            Some(_) => Self::Up,
            None => Self::Company,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Company => "company",
            Self::Up => "up",
            Self::Private => "private",
        }
    }
}
