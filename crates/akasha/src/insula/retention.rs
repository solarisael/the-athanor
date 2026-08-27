use chrono::{DateTime, SecondsFormat, Timelike, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::binding::is_house;
use super::error::{InsulaError, bad};
use super::hash::{hf, hp, hs};
use super::lock::lock;
use super::vitals::VITALS;

// enough: retention sweeps are one receipt per House per cutoff minute, so a
// hundred newest-first rows reach far past the configured raw window; upgrade
// path is a keyset cursor on (created_at, receipt_id), not a bigger cap.
pub const INSULA_MAX_RETENTION_ROWS: u32 = 100;
const RETENTION: &str = "insula.retention.raw_delete";
const RETENTION_READ: &str = "insula.retention.receipts";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RetentionStatus {
    Deleted,
    Replayed,
    Noop,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RetentionReceipt {
    pub receipt_id: Option<String>,
    pub receipt_kind: String,
    pub receipt_version: i16,
    pub status: RetentionStatus,
    pub house_id: String,
    pub sweep_version: i16,
    pub sweep_key: String,
    pub retention_days: i16,
    pub swept_through: DateTime<Utc>,
    pub window_start: Option<DateTime<Utc>>,
    pub window_end: Option<DateTime<Utc>>,
    pub event_count: i64,
    pub writer_count: i64,
    pub duplicate_count_sum: i64,
    pub drop_count_sum: i64,
    pub coverage_version: Option<i16>,
    pub coverage_hash: Option<String>,
    pub rollup_query_name: String,
    pub rollup_query_version: i16,
    pub rollup_watermark: Option<DateTime<Utc>>,
}
// A persisted sweep receipt as read back, joined to the per-writer tombstone
// summary that proves the delete. Both relations are mechanical counts and
// hashes only, so the read is body free by construction.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RetentionReceiptRow {
    pub receipt_id: String,
    pub receipt_kind: String,
    pub receipt_version: i16,
    pub house_id: String,
    pub sweep_version: i16,
    pub sweep_key: String,
    pub retention_days: i16,
    pub swept_through: DateTime<Utc>,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub event_count: i64,
    pub writer_count: i64,
    pub duplicate_count_sum: i64,
    pub drop_count_sum: i64,
    pub coverage_version: i16,
    pub coverage_hash: String,
    pub rollup_query_name: String,
    pub rollup_query_version: i16,
    pub rollup_watermark: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub tombstone_writer_count: i64,
    pub tombstone_event_count: i64,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RetentionReadResult {
    pub query_name: String,
    pub query_version: i16,
    pub rows: Vec<RetentionReceiptRow>,
}

pub async fn query_retention(
    pool: &PgPool,
    house_id: &str,
    limit: u32,
) -> Result<RetentionReadResult, InsulaError> {
    if !is_house(house_id) {
        return Err(bad("houseId", "invalid_house"));
    }
    if limit == 0 || limit > INSULA_MAX_RETENTION_ROWS {
        return Err(bad("limit", "out_of_range"));
    }
    // The newest receipts are selected first, then each is joined to its own
    // tombstone summary: the aggregate never scans tombstones for receipts the
    // caller will not see.
    let rs=sqlx::query("WITH recent AS(SELECT * FROM insula.retention_receipts WHERE house_id=$1 ORDER BY created_at DESC,receipt_id DESC LIMIT $2)SELECT r.receipt_id::text receipt_id,r.receipt_kind,r.receipt_version,r.house_id,r.sweep_version,r.sweep_key,r.retention_days,r.swept_through,r.window_start,r.window_end,r.event_count,r.writer_count,r.duplicate_count_sum,r.drop_count_sum,r.coverage_version,r.coverage_hash,r.rollup_query_name,r.rollup_query_version,r.rollup_watermark,r.created_at,t.tombstone_writer_count,t.tombstone_event_count FROM recent r LEFT JOIN LATERAL(SELECT COUNT(*)::bigint tombstone_writer_count,COALESCE(SUM(event_count),0)::bigint tombstone_event_count FROM insula.log_tombstones s WHERE s.receipt_id=r.receipt_id AND s.house_id=r.house_id)t ON TRUE ORDER BY r.created_at DESC,r.receipt_id DESC").bind(house_id).bind(i64::from(limit)).fetch_all(pool).await?;
    let rows = rs
        .into_iter()
        .map(|r| {
            Ok(RetentionReceiptRow {
                receipt_id: r.try_get("receipt_id")?,
                receipt_kind: r.try_get("receipt_kind")?,
                receipt_version: r.try_get("receipt_version")?,
                house_id: r.try_get("house_id")?,
                sweep_version: r.try_get("sweep_version")?,
                sweep_key: r.try_get("sweep_key")?,
                retention_days: r.try_get("retention_days")?,
                swept_through: r.try_get("swept_through")?,
                window_start: r.try_get("window_start")?,
                window_end: r.try_get("window_end")?,
                event_count: r.try_get("event_count")?,
                writer_count: r.try_get("writer_count")?,
                duplicate_count_sum: r.try_get("duplicate_count_sum")?,
                drop_count_sum: r.try_get("drop_count_sum")?,
                coverage_version: r.try_get("coverage_version")?,
                coverage_hash: r.try_get("coverage_hash")?,
                rollup_query_name: r.try_get("rollup_query_name")?,
                rollup_query_version: r.try_get("rollup_query_version")?,
                rollup_watermark: r.try_get("rollup_watermark")?,
                created_at: r.try_get("created_at")?,
                tombstone_writer_count: r.try_get("tombstone_writer_count")?,
                tombstone_event_count: r.try_get("tombstone_event_count")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(RetentionReadResult {
        query_name: RETENTION_READ.into(),
        query_version: 1,
        rows,
    })
}

fn sweep(h: &str, c: DateTime<Utc>, d: i16) -> String {
    let mut x = hs("insula.retention.sweep.v1");
    hp(&mut x, "house", h);
    hp(
        &mut x,
        "cutoff",
        &c.to_rfc3339_opts(SecondsFormat::Nanos, true),
    );
    hp(&mut x, "days", &d.to_string());
    hf(x)
}
fn rid(k: &str) -> String {
    let d = Sha256::digest(k);
    let mut b = [0; 16];
    b.copy_from_slice(&d[..16]);
    b[6] = (b[6] & 15) | 80;
    b[8] = (b[8] & 63) | 128;
    Uuid::from_bytes(b).to_string()
}
pub async fn run_retention(
    pool: &PgPool,
    house_id: &str,
    cutoff: DateTime<Utc>,
    days: i16,
) -> Result<RetentionReceipt, InsulaError> {
    if !is_house(house_id) || days <= 0 || cutoff > Utc::now() {
        return Err(bad("retention", "invalid_request"));
    }
    let cutoff = cutoff
        .with_second(0)
        .and_then(|value| value.with_nanosecond(0))
        .ok_or_else(|| bad("retention", "invalid_cutoff"))?;
    let key = sweep(house_id, cutoff, days);
    let id = rid(&key);
    let mut tx = pool.begin().await?;
    lock(&mut tx, house_id, true).await?;
    let select = "SELECT receipt_id::text receipt_id,receipt_kind,receipt_version,house_id,sweep_version,sweep_key,retention_days,swept_through,window_start,window_end,event_count,writer_count,duplicate_count_sum,drop_count_sum,coverage_version,coverage_hash,rollup_query_name,rollup_query_version,rollup_watermark FROM insula.retention_receipts WHERE house_id=$1 AND sweep_version=1 AND sweep_key=$2 FOR UPDATE";
    if let Some(r) = sqlx::query(select)
        .bind(house_id)
        .bind(&key)
        .fetch_optional(&mut *tx)
        .await?
    {
        let out = RetentionReceipt {
            receipt_id: Some(r.try_get("receipt_id")?),
            receipt_kind: r.try_get("receipt_kind")?,
            receipt_version: r.try_get("receipt_version")?,
            status: RetentionStatus::Replayed,
            house_id: r.try_get("house_id")?,
            sweep_version: r.try_get("sweep_version")?,
            sweep_key: r.try_get("sweep_key")?,
            retention_days: r.try_get("retention_days")?,
            swept_through: r.try_get("swept_through")?,
            window_start: Some(r.try_get("window_start")?),
            window_end: Some(r.try_get("window_end")?),
            event_count: r.try_get("event_count")?,
            writer_count: r.try_get("writer_count")?,
            duplicate_count_sum: r.try_get("duplicate_count_sum")?,
            drop_count_sum: r.try_get("drop_count_sum")?,
            coverage_version: Some(r.try_get("coverage_version")?),
            coverage_hash: Some(r.try_get("coverage_hash")?),
            rollup_query_name: r.try_get("rollup_query_name")?,
            rollup_query_version: r.try_get("rollup_query_version")?,
            rollup_watermark: Some(r.try_get("rollup_watermark")?),
        };
        tx.commit().await?;
        return Ok(out);
    }
    let a=sqlx::query("SELECT MIN(observed_at) ws,MAX(observed_at) we,COUNT(*)::bigint n,COUNT(DISTINCT writer_id)::bigint writers,COALESCE(SUM(duplicate_count),0)::bigint duplicates,COALESCE(SUM(drop_count),0)::bigint drops,MAX(ingested_at) watermark,encode(digest(convert_to(string_agg(event_id::text||':'||semantic_hash,E'\\n' ORDER BY event_id),'UTF8'),'sha256'),'hex') coverage FROM insula.log WHERE house_id=$1 AND expires_at<$2").bind(house_id).bind(cutoff).fetch_one(&mut *tx).await?;
    let n: i64 = a.try_get("n")?;
    if n == 0 {
        tx.commit().await?;
        return Ok(RetentionReceipt {
            receipt_id: None,
            receipt_kind: RETENTION.into(),
            receipt_version: 1,
            status: RetentionStatus::Noop,
            house_id: house_id.into(),
            sweep_version: 1,
            sweep_key: key,
            retention_days: days,
            swept_through: cutoff,
            window_start: None,
            window_end: None,
            event_count: 0,
            writer_count: 0,
            duplicate_count_sum: 0,
            drop_count_sum: 0,
            coverage_version: None,
            coverage_hash: None,
            rollup_query_name: VITALS.into(),
            rollup_query_version: 1,
            rollup_watermark: None,
        });
    }
    let ws: DateTime<Utc> = a.try_get("ws")?;
    let we: DateTime<Utc> = a.try_get("we")?;
    let writers: i64 = a.try_get("writers")?;
    let duplicates: i64 = a.try_get("duplicates")?;
    let drops: i64 = a.try_get("drops")?;
    let watermark: DateTime<Utc> = a.try_get("watermark")?;
    let coverage: String = a.try_get("coverage")?;
    // Exact observed_at + 14-day expiry, a minute-truncated cutoff, and a
    // strict `< cutoff` predicate make every selected source minute complete:
    // the boundary minute is wholly retained and every prior minute is wholly
    // eligible. Comparing the selected source groups to whole Vitals rows is
    // therefore exact rather than a subset comparison.
    let missing:i64=sqlx::query_scalar("WITH s AS(SELECT date_trunc('minute',observed_at AT TIME ZONE 'UTC')AT TIME ZONE 'UTC' AS minute,house_id,room,spirit,component,layer,operation,phase,outcome_class,COUNT(*)::bigint n,COALESCE(SUM(duration_us),0)::bigint duration_sum,MAX(duration_us) duration_max,SUM(bytes_in)::bigint bytes_in_sum,SUM(bytes_out)::bigint bytes_out_sum,SUM(tokens_in)::bigint tokens_in_sum,SUM(tokens_out)::bigint tokens_out_sum,SUM(drop_count)::bigint drops,MIN(writer_sequence) first_sequence,MAX(writer_sequence) last_sequence,MIN(observed_at) first_observed,MAX(observed_at) last_observed,encode(digest(convert_to(string_agg(event_id::text||':'||semantic_hash,E'\\n' ORDER BY event_id),'UTF8'),'sha256'),'hex') coverage FROM insula.log WHERE house_id=$1 AND expires_at<$2 GROUP BY 1,2,3,4,5,6,7,8,9)SELECT COUNT(*)::bigint FROM s LEFT JOIN insula.vitals_minute v ON v.query_name='insula.vitals.minute'AND v.query_version=1 AND v.minute=s.minute AND v.house_id=s.house_id AND v.room=s.room AND v.spirit=s.spirit AND v.component=s.component AND v.layer=s.layer AND v.operation=s.operation AND v.phase=s.phase AND v.outcome_class=s.outcome_class WHERE v.event_count IS NULL OR v.event_count<>s.n OR v.duration_us_sum<>s.duration_sum OR v.duration_us_max IS DISTINCT FROM s.duration_max OR v.bytes_in_sum<>s.bytes_in_sum OR v.bytes_out_sum<>s.bytes_out_sum OR v.tokens_in_sum<>s.tokens_in_sum OR v.tokens_out_sum<>s.tokens_out_sum OR v.drop_count_sum<>s.drops OR v.source_first_sequence<>s.first_sequence OR v.source_last_sequence<>s.last_sequence OR v.source_first_observed_at<>s.first_observed OR v.source_last_observed_at<>s.last_observed OR v.source_coverage_hash<>s.coverage").bind(house_id).bind(cutoff).fetch_one(&mut *tx).await?;
    if missing != 0 {
        return Err(InsulaError::Invariant(
            "retention refused: Vitals coverage incomplete",
        ));
    }
    sqlx::query("INSERT INTO insula.retention_receipts(receipt_id,receipt_kind,receipt_version,house_id,sweep_version,sweep_key,retention_days,swept_through,window_start,window_end,event_count,writer_count,duplicate_count_sum,drop_count_sum,coverage_version,coverage_hash,rollup_query_name,rollup_query_version,rollup_watermark)VALUES($1::uuid,'insula.retention.raw_delete',1,$2,1,$3,$4,$5,$6,$7,$8,$9,$10,$11,1,$12,'insula.vitals.minute',1,$13)").bind(&id).bind(house_id).bind(&key).bind(days).bind(cutoff).bind(ws).bind(we).bind(n).bind(writers).bind(duplicates).bind(drops).bind(&coverage).bind(watermark).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO insula.log_tombstones(tombstone_id,receipt_id,receipt_kind,house_id,writer_id,first_writer_sequence,last_writer_sequence,first_observed_at,last_observed_at,event_count,room_count,spirit_count,session_count,duplicate_count_sum,drop_count_sum,coverage_version,coverage_hash)SELECT gen_random_uuid(),$1::uuid,'insula.retention.raw_delete',house_id,writer_id,MIN(writer_sequence),MAX(writer_sequence),MIN(observed_at),MAX(observed_at),COUNT(*)::bigint,COUNT(DISTINCT room)::bigint,COUNT(DISTINCT spirit)::bigint,COUNT(DISTINCT session_id)::bigint,COALESCE(SUM(duplicate_count),0)::bigint,COALESCE(SUM(drop_count),0)::bigint,1,encode(digest(convert_to(string_agg(event_id::text||':'||semantic_hash,E'\\n' ORDER BY event_id),'UTF8'),'sha256'),'hex')FROM insula.log WHERE house_id=$2 AND expires_at<$3 GROUP BY house_id,writer_id").bind(&id).bind(house_id).bind(cutoff).execute(&mut *tx).await?;
    let proof:i64=sqlx::query_scalar("SELECT COALESCE(SUM(event_count),0)::bigint FROM insula.log_tombstones WHERE receipt_id=$1::uuid AND house_id=$2").bind(&id).bind(house_id).fetch_one(&mut *tx).await?;
    if proof != n {
        return Err(InsulaError::Invariant("tombstone coverage mismatch"));
    }
    let deleted = sqlx::query("DELETE FROM insula.log WHERE house_id=$1 AND expires_at<$2")
        .bind(house_id)
        .bind(cutoff)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if deleted != n as u64 {
        return Err(InsulaError::Invariant("retention delete count changed"));
    }
    tx.commit().await?;
    Ok(RetentionReceipt {
        receipt_id: Some(id),
        receipt_kind: RETENTION.into(),
        receipt_version: 1,
        status: RetentionStatus::Deleted,
        house_id: house_id.into(),
        sweep_version: 1,
        sweep_key: key,
        retention_days: days,
        swept_through: cutoff,
        window_start: Some(ws),
        window_end: Some(we),
        event_count: n,
        writer_count: writers,
        duplicate_count_sum: duplicates,
        drop_count_sum: drops,
        coverage_version: Some(1),
        coverage_hash: Some(coverage),
        rollup_query_name: VITALS.into(),
        rollup_query_version: 1,
        rollup_watermark: Some(watermark),
    })
}
