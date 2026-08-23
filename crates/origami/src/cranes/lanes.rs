//! Lanes: which current a crane flies. `boat.ready` predates addressing
//! and carries none; addressed lanes name a recipient kind and key.
//!
//! Concern: the mapping between a recipient (kind plus key) and the one
//! NATS subject that recipient owns, in both directions. The subject
//! vocabulary a lane maps onto lives in [`crate::cranes::broker`]; the
//! payload that rides a lane lives in [`crate::cranes::envelope`].
//!
//! Door: [`Lane`], [`RecipientKind`]. Extracted from
//! house-delivery/src/model.rs; the envelope tests pin the routing.

use crate::cranes::broker::{BOAT_READY_SUBJECT, CRANE_SUBJECT_PREFIX};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecipientKind {
    Worker,
    Familiar,
    Room,
    Reviewer,
}

impl RecipientKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Worker => "worker",
            Self::Familiar => "familiar",
            Self::Room => "room",
            Self::Reviewer => "reviewer",
        }
    }
}

impl fmt::Display for RecipientKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for RecipientKind {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "worker" => Ok(Self::Worker),
            "familiar" => Ok(Self::Familiar),
            "room" => Ok(Self::Room),
            "reviewer" => Ok(Self::Reviewer),
            other => bail!("unknown recipient kind {other}"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Lane {
    BoatReady,
    Addressed {
        recipient_kind: RecipientKind,
        recipient_key: String,
    },
}

impl Lane {
    pub fn subject(&self) -> String {
        match self {
            Self::BoatReady => BOAT_READY_SUBJECT.to_owned(),
            Self::Addressed {
                recipient_kind,
                recipient_key,
            } => format!("{CRANE_SUBJECT_PREFIX}{recipient_kind}.{recipient_key}"),
        }
    }

    pub fn from_subject(subject: &str) -> Option<Self> {
        if subject == BOAT_READY_SUBJECT {
            return Some(Self::BoatReady);
        }
        let (kind, key) = subject
            .strip_prefix(CRANE_SUBJECT_PREFIX)?
            .split_once('.')?;
        if !is_recipient_key(key) {
            return None;
        }
        Some(Self::Addressed {
            recipient_kind: kind.parse().ok()?,
            recipient_key: key.to_owned(),
        })
    }
}

pub(crate) fn is_recipient_key(value: &str) -> bool {
    if value.is_empty() || value.len() > 64 {
        return false;
    }
    let mut bytes = value.bytes();
    let first = bytes.next().expect("non-empty recipient key");
    (first.is_ascii_lowercase() || first.is_ascii_digit())
        && bytes.all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
        })
}
