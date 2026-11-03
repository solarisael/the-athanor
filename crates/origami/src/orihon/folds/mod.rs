//! Immutable, non-authoritative folds of exact Orihon source snapshots.

mod file;
mod pg;

use super::error::{Error, invalid, refusal};
use super::prepare;
use hearth::orihon::Record;
use serde::{Deserialize, Serialize, Serializer, ser::SerializeSeq};
use sha2::{Digest, Sha256};

pub use file::{append_file, list_file, read_file};
pub use pg::{append_pg, list_pg, read_pg};

const MAX_FOLDS: usize = 200_000;

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FoldRef {
    pub level: i16,
    pub index: i64,
    pub source_digest: String,
    pub recipe_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SourceRef {
    pub seq: i64,
    pub content_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Fold {
    pub room: String,
    pub level: i16,
    pub index: i64,
    pub source_digest: String,
    pub recipe_digest: String,
    pub sources: Vec<SourceRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<FoldRef>,
    pub text: String,
    pub model: Option<String>,
    pub tries: u8,
    pub built_at: String,
}

impl Fold {
    pub fn reference(&self) -> FoldRef {
        FoldRef {
            level: self.level,
            index: self.index,
            source_digest: self.source_digest.clone(),
            recipe_digest: self.recipe_digest.clone(),
        }
    }
}

pub fn children_digest(children: &[FoldRef]) -> String {
    hex::encode(Sha256::digest(
        serde_json::to_vec(children).expect("fold refs JSON"),
    ))
}

fn sort_folds(folds: &mut [Fold]) {
    folds.sort_by(|a, b| {
        (a.level, a.index, &a.source_digest, &a.recipe_digest).cmp(&(
            b.level,
            b.index,
            &b.source_digest,
            &b.recipe_digest,
        ))
    });
}

pub fn source_digest(records: &[Record]) -> String {
    digest_refs(
        records
            .iter()
            .map(|record| (record.seq, record.content_digest.as_str())),
    )
}

fn refs_digest(sources: &[SourceRef]) -> String {
    digest_refs(
        sources
            .iter()
            .map(|source| (source.seq, source.content_digest.as_str())),
    )
}

fn digest_refs<'a>(sources: impl ExactSizeIterator<Item = (i64, &'a str)>) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Ref<'a> {
        seq: i64,
        content_digest: &'a str,
    }

    let mut bytes = Vec::new();
    let mut serializer = serde_json::Serializer::new(&mut bytes);
    let mut sequence = serializer
        .serialize_seq(Some(sources.len()))
        .expect("JSON sequence");

    for (seq, content_digest) in sources {
        sequence
            .serialize_element(&Ref {
                seq,
                content_digest,
            })
            .expect("source ref JSON");
    }
    sequence.end().expect("source refs JSON");

    hex::encode(Sha256::digest(bytes))
}

fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn identity(room: &str, level: i16, index: i64, source: &str, recipe: &str) -> Result<(), Error> {
    prepare::identity(room, "room")?;

    if !(0..=62).contains(&level) || index < 0 {
        return Err(invalid(
            "fold level must be 0 through 62 and index nonnegative",
        ));
    }
    if level >= 1 {
        let width = 1_i64 << (level - 1);
        if index
            .checked_mul(width)
            .and_then(|start| start.checked_add(width - 1))
            .is_none()
        {
            return Err(invalid("fold page interval overflows"));
        }
    }
    if !is_digest(source) || !is_digest(recipe) {
        return Err(invalid("fold digests must be lowercase SHA-256 hex"));
    }

    Ok(())
}

fn validate(fold: &Fold) -> Result<(), Error> {
    identity(
        &fold.room,
        fold.level,
        fold.index,
        &fold.source_digest,
        &fold.recipe_digest,
    )?;

    if fold.level >= 2 {
        if !fold.sources.is_empty() || fold.children.len() != 2 {
            return Err(invalid(
                "a branch requires exactly two children and no raw sources",
            ));
        }
        let left = fold
            .index
            .checked_mul(2)
            .ok_or_else(|| invalid("child index overflows"))?;
        for (offset, child) in fold.children.iter().enumerate() {
            identity(
                &fold.room,
                child.level,
                child.index,
                &child.source_digest,
                &child.recipe_digest,
            )?;
            if child.level != fold.level - 1 || child.index != left + offset as i64 {
                return Err(invalid("branch child geometry does not match its parent"));
            }
        }
        if children_digest(&fold.children) != fold.source_digest {
            return Err(invalid(
                "sourceDigest does not match the ordered child refs",
            ));
        }
    } else {
        if !fold.children.is_empty() {
            return Err(invalid("message and page folds cannot have children"));
        }
        if fold.sources.is_empty() || fold.sources.len() > 4096 {
            return Err(invalid("a fold requires 1 through 4096 source refs"));
        }
        let mut previous = None;

        for source in &fold.sources {
            if source.seq < 0
                || previous.is_some_and(|seq| source.seq <= seq)
                || !is_digest(&source.content_digest)
            {
                return Err(invalid(
                    "fold sources must have sorted unique nonnegative seq and valid digests",
                ));
            }
            previous = Some(source.seq);
        }

        if fold.level == 0 && (fold.sources.len() != 1 || fold.sources[0].seq != fold.index) {
            return Err(invalid(
                "a message fold requires exactly its indexed source",
            ));
        }
        if refs_digest(&fold.sources) != fold.source_digest {
            return Err(invalid(
                "sourceDigest does not match the ordered source refs",
            ));
        }
    }
    if fold.text.is_empty() || fold.text.len() > 512 || fold.text.contains('\0') {
        return Err(invalid(
            "fold text must contain 1 through 512 UTF-8 bytes and no NUL",
        ));
    }
    match (&fold.model, fold.tries) {
        (None, 0) => {}
        (Some(model), 1..=5) => prepare::identity(model, "model")?,
        _ => {
            return Err(invalid(
                "model is absent exactly when tries is zero; model tries must be 1 through 5",
            ));
        }
    }
    if fold.built_at.contains('\0') {
        return Err(invalid("builtAt cannot contain NUL"));
    }

    Ok(())
}

fn replay(stored: Fold, proposed: &Fold) -> Result<Fold, Error> {
    if stored.sources != proposed.sources
        || stored.children != proposed.children
        || stored.text != proposed.text
        || stored.model != proposed.model
        || stored.tries != proposed.tries
    {
        return Err(refusal(
            "idempotency_reuse",
            "fold identity was reused with different content",
        ));
    }

    Ok(stored)
}
