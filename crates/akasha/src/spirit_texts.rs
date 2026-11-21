use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpiritTextKind {
    Identity,
    Presence,
    Spirit,
}

impl SpiritTextKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Presence => "presence",
            Self::Spirit => "spirit",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "identity" => Some(Self::Identity),
            "presence" => Some(Self::Presence),
            "spirit" => Some(Self::Spirit),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpiritText {
    pub room: String,
    pub spirit: String,
    pub kind: SpiritTextKind,
    pub revision: i32,
    pub body: String,
    pub sha256: String,
    pub written_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum SpiritTextError {
    #[error("stale spirit text revision; current revision is {current}")]
    StaleRevision { current: i32 },
    #[error("invalid spirit text: {0}")]
    Invalid(&'static str),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Returns the latest revision, or `None` if the spirit has no text of this kind.
pub async fn current_spirit_text(
    pool: &PgPool,
    room: &str,
    spirit: &str,
    kind: SpiritTextKind,
) -> Result<Option<SpiritText>, SpiritTextError> {
    let row = sqlx::query(
        "SELECT revision, body, sha256, written_by, created_at FROM spirit_texts
         WHERE room = $1 AND spirit = $2 AND kind = $3 ORDER BY revision DESC LIMIT 1",
    )
    .bind(room)
    .bind(spirit)
    .bind(kind.as_str())
    .fetch_optional(pool)
    .await?;

    row.map(|row| {
        Ok(SpiritText {
            room: room.into(),
            spirit: spirit.into(),
            kind,
            revision: row.try_get("revision")?,
            body: row.try_get("body")?,
            sha256: row.try_get("sha256")?,
            written_by: row.try_get("written_by")?,
            created_at: row.try_get("created_at")?,
        })
    })
    .transpose()
}

/// Starts at expected revision zero. An identical body keeps the current revision and author.
pub async fn append_spirit_text(
    pool: &PgPool,
    room: &str,
    spirit: &str,
    kind: SpiritTextKind,
    body: &str,
    written_by: &str,
    expected_revision: i32,
) -> Result<SpiritText, SpiritTextError> {
    if !room.split('-').all(|part| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    }) {
        return Err(SpiritTextError::Invalid("room"));
    }
    if spirit.is_empty() || spirit.trim_matches(' ') != spirit {
        return Err(SpiritTextError::Invalid("spirit"));
    }
    if written_by.trim_matches(' ').is_empty() {
        return Err(SpiritTextError::Invalid("written_by"));
    }
    if body.is_empty() {
        return Err(SpiritTextError::Invalid("body"));
    }
    if body.contains('\0') {
        return Err(SpiritTextError::Invalid("body contains U+0000"));
    }

    let row = sqlx::query(
        r#"SELECT revision, body = $4 COLLATE "C" AS same, sha256, written_by, created_at
           FROM spirit_texts WHERE room = $1 AND spirit = $2 AND kind = $3
           ORDER BY revision DESC LIMIT 1"#,
    )
    .bind(room)
    .bind(spirit)
    .bind(kind.as_str())
    .bind(body)
    .fetch_optional(pool)
    .await?;
    let current = row
        .as_ref()
        .map(|row| row.try_get("revision"))
        .transpose()?
        .unwrap_or(0);

    if expected_revision != current {
        return Err(SpiritTextError::StaleRevision { current });
    }
    if let Some(row) = row {
        if row.try_get::<bool, _>("same")? {
            return Ok(SpiritText {
                room: room.into(),
                spirit: spirit.into(),
                kind,
                revision: current,
                body: body.into(),
                sha256: row.try_get("sha256")?,
                written_by: row.try_get("written_by")?,
                created_at: row.try_get("created_at")?,
            });
        }
    }

    let sha256 = format!("{:x}", Sha256::digest(body.as_bytes()));
    let inserted = sqlx::query_as::<_, (i32, DateTime<Utc>)>(
        "INSERT INTO spirit_texts (room, spirit, kind, revision, body, sha256, written_by)
         VALUES ($1, $2, $3, $4::integer + 1, $5, $6, $7) RETURNING revision, created_at",
    )
    .bind(room)
    .bind(spirit)
    .bind(kind.as_str())
    .bind(expected_revision)
    .bind(body)
    .bind(&sha256)
    .bind(written_by)
    .fetch_one(pool)
    .await;
    let (revision, created_at) = match inserted {
        Ok(row) => row,
        Err(sqlx::Error::Database(error)) if error.code().as_deref() == Some("23505") => {
            let current = sqlx::query_scalar(
                "SELECT revision FROM spirit_texts WHERE room = $1 AND spirit = $2 AND kind = $3
                 ORDER BY revision DESC LIMIT 1",
            )
            .bind(room)
            .bind(spirit)
            .bind(kind.as_str())
            .fetch_one(pool)
            .await?;

            return Err(SpiritTextError::StaleRevision { current });
        }
        Err(error) => return Err(error.into()),
    };

    Ok(SpiritText {
        room: room.into(),
        spirit: spirit.into(),
        kind,
        revision,
        body: body.into(),
        sha256,
        written_by: written_by.into(),
        created_at,
    })
}
