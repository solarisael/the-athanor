use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

use super::binding::{TrustedBinding, is_house};
use super::error::{InsulaError, bad};
use super::event::{ObservationEvent, ObservationPhase, OutcomeClass};

pub const INSULA_MAX_VITALS_ROWS: u32 = 5_000;
pub(super) const VITALS: &str = "insula.vitals.minute";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VitalsQuery {
    pub house_id: String,
    pub room: Option<String>,
    pub spirit: Option<String>,
    pub component: Option<String>,
    pub layer: Option<String>,
    pub operation: Option<String>,
    pub phase: Option<ObservationPhase>,
    pub outcome_class: Option<OutcomeClass>,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub limit: u32,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VitalsRow {
    pub minute: DateTime<Utc>,
    pub house_id: String,
    pub room: String,
    pub spirit: String,
    pub component: String,
    pub layer: String,
    pub operation: String,
    pub phase: String,
    pub outcome_class: String,
    pub event_count: i64,
    pub duration_us_sum: i64,
    pub duration_us_max: Option<i64>,
    pub bytes_in_sum: i64,
    pub bytes_out_sum: i64,
    pub tokens_in_sum: i64,
    pub tokens_out_sum: i64,
    pub drop_count_sum: i64,
    pub source_first_sequence: i64,
    pub source_last_sequence: i64,
    pub source_first_observed_at: DateTime<Utc>,
    pub source_last_observed_at: DateTime<Utc>,
    pub source_coverage_hash: String,
    pub updated_at: DateTime<Utc>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VitalsResult {
    pub query_name: String,
    pub query_version: i16,
    pub rows: Vec<VitalsRow>,
}

pub(super) async fn vitals(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    b: &TrustedBinding,
    e: &ObservationEvent,
) -> Result<(), InsulaError> {
    sqlx::query(
        r#"INSERT INTO insula.vitals_minute(
               query_name,query_version,minute,house_id,room,spirit,component,layer,operation,
               phase,outcome_class,event_count,duration_us_sum,duration_us_max,bytes_in_sum,
               bytes_out_sum,tokens_in_sum,tokens_out_sum,drop_count_sum,source_first_sequence,
               source_last_sequence,source_first_observed_at,source_last_observed_at,
               source_coverage_hash
           )
           VALUES(
               'insula.vitals.minute',1,
               date_trunc('minute',$1::timestamptz AT TIME ZONE 'UTC') AT TIME ZONE 'UTC',
               $2,$3,$4,$5,$6,$7,$8,$9,1,COALESCE($10,0),$10,$11,$12,$13,$14,$15,$16,$16,$1,$1,$17
           )
           ON CONFLICT(
               query_name,query_version,minute,house_id,room,spirit,component,layer,operation,
               phase,outcome_class
           )
           DO UPDATE SET
               event_count=insula.vitals_minute.event_count+1,
               duration_us_sum=insula.vitals_minute.duration_us_sum+EXCLUDED.duration_us_sum,
               duration_us_max=GREATEST(insula.vitals_minute.duration_us_max,EXCLUDED.duration_us_max),
               bytes_in_sum=insula.vitals_minute.bytes_in_sum+EXCLUDED.bytes_in_sum,
               bytes_out_sum=insula.vitals_minute.bytes_out_sum+EXCLUDED.bytes_out_sum,
               tokens_in_sum=insula.vitals_minute.tokens_in_sum+EXCLUDED.tokens_in_sum,
               tokens_out_sum=insula.vitals_minute.tokens_out_sum+EXCLUDED.tokens_out_sum,
               drop_count_sum=insula.vitals_minute.drop_count_sum+EXCLUDED.drop_count_sum,
               source_first_sequence=LEAST(
                   insula.vitals_minute.source_first_sequence,EXCLUDED.source_first_sequence
               ),
               source_last_sequence=GREATEST(
                   insula.vitals_minute.source_last_sequence,EXCLUDED.source_last_sequence
               ),
               source_first_observed_at=LEAST(
                   insula.vitals_minute.source_first_observed_at,EXCLUDED.source_first_observed_at
               ),
               source_last_observed_at=GREATEST(
                   insula.vitals_minute.source_last_observed_at,EXCLUDED.source_last_observed_at
               ),
               updated_at=NOW()"#,
    )
    .bind(e.observed_at)
    .bind(&b.house_id)
    .bind(&b.room)
    .bind(&b.spirit)
    .bind(&e.component)
    .bind(&e.layer)
    .bind(&e.operation)
    .bind(e.phase.as_str())
    .bind(e.outcome_class.as_str())
    .bind(e.duration_us)
    .bind(e.bytes_in)
    .bind(e.bytes_out)
    .bind(e.tokens_in)
    .bind(e.tokens_out)
    .bind(e.drop_count)
    .bind(e.writer_sequence)
    .bind(&e.semantic_hash)
    .execute(&mut **tx)
    .await?;

    // Coverage is a canonical set hash, not an arrival-order hash chain.
    sqlx::query(
        r#"UPDATE insula.vitals_minute AS v
           SET source_coverage_hash=source.coverage_hash,updated_at=NOW()
           FROM (
               SELECT encode(
                   digest(
                       convert_to(
                           string_agg(event_id::text||':'||semantic_hash,E'\n' ORDER BY event_id),
                           'UTF8'
                       ),
                       'sha256'
                   ),
                   'hex'
               ) AS coverage_hash
               FROM insula.log
               WHERE house_id=$2 AND room=$3 AND spirit=$4 AND component=$5 AND layer=$6
                 AND operation=$7 AND phase=$8 AND outcome_class=$9
                 AND date_trunc('minute',observed_at AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'
                     = date_trunc('minute',$1::timestamptz AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'
           ) AS source
           WHERE v.query_name='insula.vitals.minute' AND v.query_version=1
             AND v.minute=date_trunc('minute',$1::timestamptz AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'
             AND v.house_id=$2 AND v.room=$3 AND v.spirit=$4 AND v.component=$5
             AND v.layer=$6 AND v.operation=$7 AND v.phase=$8 AND v.outcome_class=$9"#,
    )
    .bind(e.observed_at)
    .bind(&b.house_id)
    .bind(&b.room)
    .bind(&b.spirit)
    .bind(&e.component)
    .bind(&e.layer)
    .bind(&e.operation)
    .bind(e.phase.as_str())
    .bind(e.outcome_class.as_str())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn query_vitals(pool: &PgPool, q: &VitalsQuery) -> Result<VitalsResult, InsulaError> {
    if !is_house(&q.house_id)
        || q.end <= q.start
        || q.end - q.start > Duration::days(366)
        || q.limit == 0
        || q.limit > INSULA_MAX_VITALS_ROWS
    {
        return Err(bad("query", "out_of_range"));
    }
    let p = q.phase.map(ObservationPhase::as_str);
    let o = q.outcome_class.map(OutcomeClass::as_str);
    let rs=sqlx::query("SELECT minute,house_id,room,spirit,component,layer,operation,phase,outcome_class,event_count,duration_us_sum,duration_us_max,bytes_in_sum,bytes_out_sum,tokens_in_sum,tokens_out_sum,drop_count_sum,source_first_sequence,source_last_sequence,source_first_observed_at,source_last_observed_at,source_coverage_hash,updated_at FROM insula.vitals_minute WHERE query_name='insula.vitals.minute'AND query_version=1 AND house_id=$1 AND minute>=$2 AND minute<$3 AND($4::text IS NULL OR room=$4)AND($5::text IS NULL OR spirit=$5)AND($6::text IS NULL OR component=$6)AND($7::text IS NULL OR layer=$7)AND($8::text IS NULL OR operation=$8)AND($9::text IS NULL OR phase=$9)AND($10::text IS NULL OR outcome_class=$10)ORDER BY minute,room,spirit LIMIT $11").bind(&q.house_id).bind(q.start).bind(q.end).bind(&q.room).bind(&q.spirit).bind(&q.component).bind(&q.layer).bind(&q.operation).bind(p).bind(o).bind(i64::from(q.limit)).fetch_all(pool).await?;
    let rows = rs
        .into_iter()
        .map(|r| {
            Ok(VitalsRow {
                minute: r.try_get("minute")?,
                house_id: r.try_get("house_id")?,
                room: r.try_get("room")?,
                spirit: r.try_get("spirit")?,
                component: r.try_get("component")?,
                layer: r.try_get("layer")?,
                operation: r.try_get("operation")?,
                phase: r.try_get("phase")?,
                outcome_class: r.try_get("outcome_class")?,
                event_count: r.try_get("event_count")?,
                duration_us_sum: r.try_get("duration_us_sum")?,
                duration_us_max: r.try_get("duration_us_max")?,
                bytes_in_sum: r.try_get("bytes_in_sum")?,
                bytes_out_sum: r.try_get("bytes_out_sum")?,
                tokens_in_sum: r.try_get("tokens_in_sum")?,
                tokens_out_sum: r.try_get("tokens_out_sum")?,
                drop_count_sum: r.try_get("drop_count_sum")?,
                source_first_sequence: r.try_get("source_first_sequence")?,
                source_last_sequence: r.try_get("source_last_sequence")?,
                source_first_observed_at: r.try_get("source_first_observed_at")?,
                source_last_observed_at: r.try_get("source_last_observed_at")?,
                source_coverage_hash: r.try_get("source_coverage_hash")?,
                updated_at: r.try_get("updated_at")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(VitalsResult {
        query_name: VITALS.into(),
        query_version: 1,
        rows,
    })
}
