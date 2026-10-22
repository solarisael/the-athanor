use super::{Fold, identity, validate};
use crate::orihon::Error;
use serde_json::json;
use sqlx::{PgPool, Row, postgres::PgRow};

fn from_row(row: PgRow) -> Result<Fold, Error> {
    let sources = serde_json::from_value(row.try_get("sources")?)
        .map_err(|_| super::invalid("stored fold sources are malformed"))?;
    let tries: i16 = row.try_get("tries")?;
    let fold = Fold {
        room: row.try_get("room")?,
        level: row.try_get("level")?,
        index: row.try_get("fold_index")?,
        source_digest: row.try_get("source_digest")?,
        recipe_digest: row.try_get("recipe_digest")?,
        sources,
        text: row.try_get("text")?,
        model: row.try_get("model")?,
        tries: u8::try_from(tries).map_err(|_| super::invalid("stored fold tries are invalid"))?,
        built_at: row.try_get("built_at")?,
    };
    validate(&fold)?;

    Ok(fold)
}

pub async fn read_pg(
    pool: &PgPool,
    room: &str,
    level: i16,
    index: i64,
    source_digest: &str,
    recipe_digest: &str,
) -> Result<Option<Fold>, Error> {
    identity(room, level, index, source_digest, recipe_digest)?;
    let row = sqlx::query(
        "SELECT * FROM orihon_folds WHERE room=$1 AND level=$2 AND fold_index=$3
         AND source_digest=$4 AND recipe_digest=$5",
    )
    .bind(room)
    .bind(level)
    .bind(index)
    .bind(source_digest)
    .bind(recipe_digest)
    .fetch_optional(pool)
    .await?;

    row.map(from_row).transpose()
}

pub async fn append_pg(pool: &PgPool, fold: &Fold) -> Result<Fold, Error> {
    validate(fold)?;
    let row = sqlx::query(
        "SELECT * FROM orihon_fold_append(jsonb_populate_record(NULL::orihon_folds,$1))",
    )
    .bind(json!({
        "room": fold.room,
        "level": fold.level,
        "fold_index": fold.index,
        "source_digest": fold.source_digest,
        "recipe_digest": fold.recipe_digest,
        "sources": fold.sources,
        "text": fold.text,
        "model": fold.model,
        "tries": fold.tries,
        "built_at": fold.built_at,
    }))
    .fetch_one(pool)
    .await?;

    from_row(row)
}
