//! The receipt a durable write's file backup leaves behind.
//!
//! A write commits to PostgreSQL first; the dump that follows is a second,
//! weaker durability. Its outcome is still a fact a spirit must be able to
//! name afterwards: which dump, with which checksum, made by which tool, or
//! which mechanical seam refused and how long it took to find out.

use crate::error::DomainError;

/// The longest one-line failure detail a receipt carries.
pub const MAX_BACKUP_DETAIL_BYTES: usize = 512;

/// Why a backup produced no dump. One variant per mechanical seam; the
/// variant names the seam, never the message, so it can be an Insula error
/// class as it stands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackupFailureCode {
    /// `pg_dump` resolved nowhere: not under `PG_BIN_DIR`, not inside WSL,
    /// not on `PATH`.
    PgDumpNotFound,
    /// `pg_restore` (the dump validity check) resolved nowhere.
    PgRestoreNotFound,
    /// The database URL, keep count, or backup directory is unusable.
    Configuration,
    /// The Athanor state root is unresolved, so there is no default dump directory.
    StateRoot,
    /// Reading, writing, renaming, or spawning failed at the operating system.
    Io,
    /// `pg_dump` or `pg_restore` ran and exited non-zero.
    Command,
    /// The manifest could not be produced, or the schema lineage is unknown.
    Manifest,
}

impl BackupFailureCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PgDumpNotFound => "pg_dump_not_found",
            Self::PgRestoreNotFound => "pg_restore_not_found",
            Self::Configuration => "backup_error.config",
            Self::StateRoot => "backup_error.state",
            Self::Io => "backup_error.io",
            Self::Command => "backup_error.command",
            Self::Manifest => "backup_error.manifest",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [
            Self::PgDumpNotFound,
            Self::PgRestoreNotFound,
            Self::Configuration,
            Self::StateRoot,
            Self::Io,
            Self::Command,
            Self::Manifest,
        ]
        .into_iter()
        .find(|code| code.as_str() == value)
    }
}

/// A dump that exists on disk with a verifiable checksum.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackupReceipt {
    dump_path: String,
    sha256: String,
    bytes: u64,
    elapsed_ms: u64,
    tool: String,
}

impl BackupReceipt {
    pub fn new(
        dump_path: String,
        sha256: String,
        bytes: u64,
        elapsed_ms: u64,
        tool: String,
    ) -> Result<Self, DomainError> {
        if dump_path.trim().is_empty() {
            return Err(DomainError::InvalidBackupReceipt {
                field: "dump_path".into(),
                message: "must not be empty".into(),
            });
        }
        if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(DomainError::InvalidBackupReceipt {
                field: "sha256".into(),
                message: "must be 64 hexadecimal characters".into(),
            });
        }
        if tool.trim().is_empty() {
            return Err(DomainError::InvalidBackupReceipt {
                field: "tool".into(),
                message: "must name the pg_dump that ran".into(),
            });
        }
        Ok(Self {
            dump_path,
            sha256: sha256.to_ascii_lowercase(),
            bytes,
            elapsed_ms,
            tool,
        })
    }

    pub fn dump_path(&self) -> &str {
        &self.dump_path
    }
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }
    pub const fn elapsed_ms(&self) -> u64 {
        self.elapsed_ms
    }
    /// Which `pg_dump` ran: `pg_bin_dir:pg_dump`, `wsl:pg_dump`, `path:pg_dump`.
    pub fn tool(&self) -> &str {
        &self.tool
    }
}

/// A backup that produced no dump. The PostgreSQL row it followed is durable
/// regardless; this only records that the second copy is missing and why.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackupFailure {
    code: BackupFailureCode,
    detail: String,
    elapsed_ms: u64,
    tool: Option<String>,
}

impl BackupFailure {
    /// `detail` is flattened to one line, stripped of any URL credentials,
    /// and bounded. `tool` names the `pg_dump` that ran when one resolved.
    pub fn new(
        code: BackupFailureCode,
        detail: impl AsRef<str>,
        elapsed_ms: u64,
        tool: Option<String>,
    ) -> Self {
        Self {
            code,
            detail: one_line_detail(detail.as_ref()),
            elapsed_ms,
            tool: tool.filter(|tool| !tool.trim().is_empty()),
        }
    }

    pub const fn code(&self) -> BackupFailureCode {
        self.code
    }
    pub fn detail(&self) -> &str {
        &self.detail
    }
    pub const fn elapsed_ms(&self) -> u64 {
        self.elapsed_ms
    }
    pub fn tool(&self) -> Option<&str> {
        self.tool.as_deref()
    }
}

/// What a write's backup came to. `Skipped` is the write's own choice; the
/// other two are the backup's answer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BackupOutcome {
    Skipped,
    Ok(BackupReceipt),
    Failed(BackupFailure),
}

impl BackupOutcome {
    pub const fn status(&self) -> &'static str {
        match self {
            Self::Skipped => "skipped",
            Self::Ok(_) => "ok",
            Self::Failed(_) => "failed",
        }
    }

    pub const fn receipt(&self) -> Option<&BackupReceipt> {
        match self {
            Self::Ok(receipt) => Some(receipt),
            _ => None,
        }
    }

    pub const fn failure(&self) -> Option<&BackupFailure> {
        match self {
            Self::Failed(failure) => Some(failure),
            _ => None,
        }
    }

    /// The one warning line a failed backup adds to its write's receipt.
    pub fn warning(&self) -> Option<String> {
        self.failure().map(|failure| {
            format!(
                "backup failed after PostgreSQL commit ({}): {}",
                failure.code().as_str(),
                failure.detail()
            )
        })
    }
}

/// Collapses whitespace runs to one space, drops `user:password@` from any
/// URL, and bounds the result. A receipt is read in tool output and stored
/// in Insula-adjacent rows; neither may carry a credential or a stack.
fn one_line_detail(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len().min(MAX_BACKUP_DETAIL_BYTES));
    let mut pending_space = false;
    for word in raw.split_whitespace() {
        if pending_space {
            out.push(' ');
        }
        out.push_str(&redact_url_credentials(word));
        pending_space = true;
    }
    if out.len() > MAX_BACKUP_DETAIL_BYTES {
        let mut cut = MAX_BACKUP_DETAIL_BYTES;
        while !out.is_char_boundary(cut) {
            cut -= 1;
        }
        out.truncate(cut);
        out.push('…');
    }
    out
}

fn redact_url_credentials(word: &str) -> String {
    let Some(scheme_end) = word.find("://") else {
        return word.to_owned();
    };
    let after = scheme_end + 3;
    let Some(at) = word[after..].find('@') else {
        return word.to_owned();
    };
    // A `/` before the `@` means the `@` belongs to a path, not to userinfo.
    if word[after..after + at].contains('/') {
        return word.to_owned();
    }
    format!("{}[redacted]{}", &word[..after], &word[after + at..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha() -> String {
        "a".repeat(64)
    }

    #[test]
    fn receipt_requires_a_dump_path_a_real_sha256_and_a_tool() {
        assert!(matches!(
            BackupReceipt::new(" ".into(), sha(), 1, 1, "path:pg_dump".into()),
            Err(DomainError::InvalidBackupReceipt { field, .. }) if field == "dump_path"
        ));
        assert!(matches!(
            BackupReceipt::new("x.dump".into(), "abc".into(), 1, 1, "path:pg_dump".into()),
            Err(DomainError::InvalidBackupReceipt { field, .. }) if field == "sha256"
        ));
        assert!(matches!(
            BackupReceipt::new("x.dump".into(), sha(), 1, 1, String::new()),
            Err(DomainError::InvalidBackupReceipt { field, .. }) if field == "tool"
        ));
        let receipt = BackupReceipt::new(
            "C:/state/backups/db-1.dump".into(),
            "A".repeat(64),
            42,
            1500,
            "wsl:pg_dump".into(),
        )
        .unwrap();
        assert_eq!(receipt.sha256(), sha());
        assert_eq!(receipt.bytes(), 42);
        assert_eq!(receipt.elapsed_ms(), 1500);
        assert_eq!(receipt.tool(), "wsl:pg_dump");
        let outcome = BackupOutcome::Ok(receipt);
        assert_eq!(outcome.status(), "ok");
        assert!(outcome.warning().is_none());
        assert!(outcome.failure().is_none());
    }

    #[test]
    fn failure_detail_is_one_bounded_line_without_credentials() {
        let failure = BackupFailure::new(
            BackupFailureCode::Command,
            "pg_dump: error:\n  connection to postgres://sol:hunter2@127.0.0.1:5432/db failed\n\tfe_sendauth",
            20,
            Some("wsl:pg_dump".into()),
        );
        assert_eq!(
            failure.detail(),
            "pg_dump: error: connection to postgres://[redacted]@127.0.0.1:5432/db failed fe_sendauth"
        );
        assert_eq!(failure.tool(), Some("wsl:pg_dump"));
        let long = BackupFailure::new(BackupFailureCode::Io, "x".repeat(2000), 0, None);
        assert!(long.detail().len() <= MAX_BACKUP_DETAIL_BYTES + '…'.len_utf8());
        assert!(long.detail().ends_with('…'));
        assert_eq!(long.tool(), None);
        let outcome = BackupOutcome::Failed(failure);
        assert_eq!(outcome.status(), "failed");
        assert_eq!(
            outcome.warning().as_deref(),
            Some(
                "backup failed after PostgreSQL commit (backup_error.command): pg_dump: error: connection to postgres://[redacted]@127.0.0.1:5432/db failed fe_sendauth"
            )
        );
    }

    #[test]
    fn a_path_with_an_at_sign_is_not_a_credential() {
        assert_eq!(
            redact_url_credentials("https://host/room@kodo"),
            "https://host/room@kodo"
        );
        assert_eq!(redact_url_credentials("plain@word"), "plain@word");
    }

    #[test]
    fn codes_are_mechanical_and_round_trip() {
        for code in [
            BackupFailureCode::PgDumpNotFound,
            BackupFailureCode::PgRestoreNotFound,
            BackupFailureCode::Configuration,
            BackupFailureCode::StateRoot,
            BackupFailureCode::Io,
            BackupFailureCode::Command,
            BackupFailureCode::Manifest,
        ] {
            let name = code.as_str();
            assert!(
                name.bytes().enumerate().all(|(index, byte)| byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || (index > 0 && matches!(byte, b'_' | b'.'))),
                "{name} is not a mechanical name"
            );
            assert_eq!(BackupFailureCode::parse(name), Some(code));
        }
        assert_eq!(BackupFailureCode::parse("io"), None);
        assert_eq!(BackupOutcome::Skipped.status(), "skipped");
    }
}
