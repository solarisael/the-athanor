use crate::config::{AppError, EMBED_DIMENSION};
use house_core::{
    ClusterExecution, ClusterFreshnessPolicy, ClusterMaintenanceOutcome, ClusterMaintenanceRequest,
    ClusterMaintenanceStatus as DomainClusterMaintenanceStatus,
    ClusterStaleness as DomainClusterStaleness, ClusterSummary,
};
use serde::Serialize;
use sqlx::{PgPool, Row};

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ClusterStaleness {
    pub built_at: Option<chrono::DateTime<chrono::Utc>>,
    pub clusters: i64,
    pub chunks_total: i64,
    pub chunks_since_build: i64,
    pub fraction_unseen: f64,
}

pub fn cluster_is_stale(staleness: &ClusterStaleness, now: chrono::DateTime<chrono::Utc>) -> bool {
    let Some(built_at) = staleness.built_at else {
        return true;
    };
    let count = staleness.chunks_since_build >= 250;
    let fraction = staleness.chunks_total > 0
        && (staleness.chunks_since_build as f64 / staleness.chunks_total as f64) >= 0.05;
    let age =
        now.signed_duration_since(built_at).num_days() >= 7 && staleness.chunks_since_build > 0;
    count || fraction || age
}

fn unit(v: &[f32]) -> Vec<f32> {
    let n = v
        .iter()
        .map(|x| (*x as f64) * (*x as f64))
        .sum::<f64>()
        .sqrt();
    if n == 0.0 {
        return vec![0.0; v.len()];
    }
    v.iter().map(|x| (*x as f64 / n) as f32).collect()
}
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
pub type ClusterMembers = Vec<(i64, f64)>;
pub type ClusterGroup = (Vec<f32>, ClusterMembers);

/// Deterministic spherical k-means (seed 42 is represented by stable farthest
/// initialization; no RNG means equal inputs always produce equal output).
pub fn spherical_kmeans(input: &[(i64, Vec<f32>)], requested_k: usize) -> Vec<ClusterGroup> {
    if input.is_empty() {
        return Vec::new();
    }
    let points: Vec<(i64, Vec<f32>)> = input.iter().map(|(id, v)| (*id, unit(v))).collect();
    let k = requested_k.max(1).min(points.len());
    let mut centers = vec![points[0].1.clone()];
    while centers.len() < k {
        let (idx, _) = points
            .iter()
            .enumerate()
            .map(|(i, (_, v))| {
                (
                    i,
                    centers.iter().map(|c| cosine(v, c)).fold(-1.0f32, f32::max),
                )
            })
            .min_by(|a, b| {
                a.1.partial_cmp(&b.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.0.cmp(&b.0))
            })
            .unwrap();
        centers.push(points[idx].1.clone());
    }
    let mut assignment = vec![usize::MAX; points.len()];
    for _ in 0..32 {
        let next: Vec<usize> = points
            .iter()
            .map(|(_, v)| {
                centers
                    .iter()
                    .enumerate()
                    .max_by(|(ia, a), (ib, b)| {
                        cosine(v, a)
                            .partial_cmp(&cosine(v, b))
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then_with(|| ia.cmp(ib))
                    })
                    .map(|(i, _)| i)
                    .unwrap()
            })
            .collect();
        if next == assignment {
            break;
        }
        assignment = next;
        for (cluster_index, center) in centers.iter_mut().enumerate().take(k) {
            let mut sum = vec![0.0f32; points[0].1.len()];
            for (_, (_, vector)) in points
                .iter()
                .enumerate()
                .filter(|(point_index, _)| assignment[*point_index] == cluster_index)
            {
                for (dimension, value) in vector.iter().enumerate() {
                    sum[dimension] += value;
                }
            }
            if sum.iter().any(|value| *value != 0.0) {
                *center = unit(&sum);
            }
        }
    }
    let mut out = centers
        .into_iter()
        .map(|c| (c, Vec::new()))
        .collect::<Vec<_>>();
    for (i, (id, v)) in points.iter().enumerate() {
        let c = assignment[i];
        let distance = 1.0 - cosine(v, &out[c].0);
        out[c].1.push((*id, distance as f64));
    }
    out
}

pub async fn cluster_staleness(
    pool: &PgPool,
    room: Option<&str>,
) -> Result<ClusterStaleness, AppError> {
    let built: (Option<chrono::DateTime<chrono::Utc>>, i64) =
        sqlx::query_as("SELECT max(created_at), count(*) FROM memory_clusters")
            .fetch_one(pool)
            .await?;
    let scope = room.map(|_| " AND m.room = $1").unwrap_or("");
    let sql = format!(
        "SELECT count(*) FROM memory_chunks c JOIN memories m ON m.id=c.memory_id WHERE c.body_embedding IS NOT NULL AND m.archived_at IS NULL AND m.superseded_by IS NULL{scope}"
    );
    let total: i64 = if let Some(r) = room {
        sqlx::query_scalar(&sql).bind(r).fetch_one(pool).await?
    } else {
        sqlx::query_scalar(&sql).fetch_one(pool).await?
    };
    let since: i64 = if let Some(at) = built.0 {
        if let Some(room) = room {
            sqlx::query_scalar("SELECT count(*) FROM memory_chunks c JOIN memories m ON m.id=c.memory_id WHERE c.body_embedding IS NOT NULL AND m.archived_at IS NULL AND m.superseded_by IS NULL AND c.embedded_at > $1 AND m.room = $2").bind(at).bind(room).fetch_one(pool).await?
        } else {
            sqlx::query_scalar("SELECT count(*) FROM memory_chunks c JOIN memories m ON m.id=c.memory_id WHERE c.body_embedding IS NOT NULL AND m.archived_at IS NULL AND m.superseded_by IS NULL AND c.embedded_at > $1").bind(at).fetch_one(pool).await?
        }
    } else {
        total
    };
    Ok(ClusterStaleness {
        built_at: built.0,
        clusters: built.1,
        chunks_total: total,
        chunks_since_build: since,
        fraction_unseen: if total == 0 {
            0.0
        } else {
            since as f64 / total as f64
        },
    })
}

fn maintenance_status(
    info: ClusterStaleness,
    stale: bool,
) -> Result<DomainClusterMaintenanceStatus, AppError> {
    let reason = if info.built_at.is_none() {
        "never_built"
    } else if stale {
        "stale"
    } else {
        "fresh"
    };
    Ok(DomainClusterMaintenanceStatus {
        stale,
        reason: reason.into(),
        staleness: DomainClusterStaleness::new(
            info.built_at.map(|value| value.to_rfc3339()),
            info.clusters as u64,
            info.chunks_total as u64,
            info.chunks_since_build as u64,
            info.fraction_unseen,
        )
        .map_err(|error| AppError::Invalid(error.to_string()))?,
    })
}

async fn cluster_summaries(pool: &PgPool) -> Result<Vec<ClusterSummary>, AppError> {
    let rows = sqlx::query(
        "SELECT id,COALESCE(label, 'cluster') AS label,member_count,accepted FROM memory_clusters ORDER BY id",
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            ClusterSummary::new(
                row.try_get("id")?,
                row.try_get::<String, _>("label")?,
                row.try_get::<i32, _>("member_count")? as u64,
                row.try_get("accepted")?,
            )
            .map_err(|error| AppError::Invalid(error.to_string()))
        })
        .collect()
}

pub async fn cluster_maintenance(
    pool: &PgPool,
    command: ClusterMaintenanceRequest,
) -> Result<ClusterMaintenanceOutcome, AppError> {
    let stale_info = cluster_staleness(pool, None).await?;
    let stale = cluster_is_stale(&stale_info, chrono::Utc::now());
    let status = maintenance_status(stale_info, stale)?;

    match command {
        ClusterMaintenanceRequest::Check { .. } => Ok(ClusterMaintenanceOutcome::Checked {
            status,
            clusters: cluster_summaries(pool).await?,
        }),
        ClusterMaintenanceRequest::Rebuild {
            execution: ClusterExecution::DryRun,
            ..
        } => Ok(ClusterMaintenanceOutcome::DryRun { status }),
        ClusterMaintenanceRequest::Rebuild {
            freshness: ClusterFreshnessPolicy::IfStale,
            ..
        } if !stale => Ok(ClusterMaintenanceOutcome::SkippedFresh {
            status,
            clusters: cluster_summaries(pool).await?,
        }),
        ClusterMaintenanceRequest::Rebuild { k, .. } => {
            let mut tx = pool.begin().await?;
            sqlx::query(
                "SELECT pg_advisory_xact_lock(hashtextextended('solarisael.cluster_maintenance', 42))",
            )
            .execute(&mut *tx)
            .await?;
            let rows = sqlx::query("SELECT c.id,c.body_embedding::text FROM memory_chunks c JOIN memories m ON m.id=c.memory_id WHERE c.body_embedding IS NOT NULL AND m.archived_at IS NULL AND m.superseded_by IS NULL ORDER BY c.id").fetch_all(&mut *tx).await?;
            let mut points = Vec::new();
            for row in rows {
                let text: String = row.try_get("body_embedding")?;
                let vector = text
                    .trim_matches(|character| character == '[' || character == ']')
                    .split(',')
                    .filter_map(|value| value.trim().parse::<f32>().ok())
                    .collect::<Vec<_>>();
                if vector.len() != EMBED_DIMENSION {
                    return Err(AppError::Config(
                        "cluster embedding dimension is not vector(2048)".into(),
                    ));
                }
                points.push((row.try_get::<i64, _>("id")?, vector));
            }
            let groups = spherical_kmeans(&points, k as usize);
            sqlx::query("DELETE FROM memory_clusters")
                .execute(&mut *tx)
                .await?;
            let mut clusters = Vec::with_capacity(groups.len());
            for (center, members) in &groups {
                let center_text = format!(
                    "[{}]",
                    center
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(",")
                );
                let cluster_id: i64 = sqlx::query_scalar("INSERT INTO memory_clusters (label,centroid,member_count,accepted) VALUES ($1,$2::vector,$3,FALSE) RETURNING id").bind("cluster").bind(center_text).bind(members.len() as i32).fetch_one(&mut *tx).await?;
                for (chunk_id, distance) in members {
                    sqlx::query("INSERT INTO memory_cluster_members (cluster_id,chunk_id,distance_to_centroid) VALUES ($1,$2,$3)").bind(cluster_id).bind(chunk_id).bind(distance).execute(&mut *tx).await?;
                }
                clusters.push(
                    ClusterSummary::new(cluster_id, "cluster", members.len() as u64, false)
                        .map_err(|error| AppError::Invalid(error.to_string()))?,
                );
            }
            tx.commit().await?;
            Ok(ClusterMaintenanceOutcome::Rebuilt { status, clusters })
        }
    }
}

pub(crate) async fn cluster_resonance(
    pool: &PgPool,
    vector_text: &str,
    rooms: &[String],
) -> Result<serde_json::Value, AppError> {
    let rows = sqlx::query(
        "SELECT mc.id,mc.label,COUNT(mm.chunk_id)::bigint AS member_count,
                (1-(mc.centroid <=> $1::vector))::double precision AS activation
         FROM memory_clusters mc
         JOIN memory_cluster_members mm ON mm.cluster_id=mc.id
         JOIN memory_chunks c ON c.id=mm.chunk_id
         JOIN memories m ON m.id=c.memory_id
         WHERE mc.centroid IS NOT NULL
           AND m.room=ANY($2::text[])
           AND m.archived_at IS NULL
           AND m.superseded_by IS NULL
           AND COALESCE(m.type,'') <> $3
           AND (mc.label IS NULL OR mc.label NOT ILIKE 'paper boat%')
         GROUP BY mc.id,mc.label,mc.centroid
         ORDER BY activation DESC
         LIMIT 8",
    )
    .bind(vector_text)
    .bind(rooms)
    .bind(origami::boats::MEMORY_KIND)
    .fetch_all(pool)
    .await?;
    let mut profile = Vec::new();
    let mut hot = Vec::new();
    for (index, r) in rows.iter().enumerate() {
        let id: i64 = r.try_get("id")?;
        let label: Option<String> = r.try_get("label")?;
        profile.push(serde_json::json!({"cluster_id":id,"label":label.clone().unwrap_or_default(),"member_count":r.try_get::<i64,_>("member_count")?,"activation":r.try_get::<f64,_>("activation")?.clamp(0.0,1.0)}));
        if index < 3 {
            let chunks = sqlx::query(
                "SELECT m.source_path,c.heading_path,
                        (1-(c.body_embedding <=> $1::vector))::double precision AS sim
                 FROM memory_cluster_members mm
                 JOIN memory_chunks c ON c.id=mm.chunk_id
                 JOIN memories m ON m.id=c.memory_id
                 WHERE mm.cluster_id=$2
                   AND m.room=ANY($3::text[])
                   AND m.archived_at IS NULL
                   AND m.superseded_by IS NULL
                   AND COALESCE(m.type,'') <> $4
                 ORDER BY sim DESC
                 LIMIT 2",
            )
            .bind(vector_text)
            .bind(id)
            .bind(rooms)
            .bind(origami::boats::MEMORY_KIND)
            .fetch_all(pool)
            .await?;
            let pointers = chunks.into_iter().map(|c| serde_json::json!({"source_path":c.try_get::<String,_>("source_path").unwrap_or_default(),"heading_path":c.try_get::<Option<String>,_>("heading_path").ok().flatten(),"sim":c.try_get::<f64,_>("sim").unwrap_or(0.0).clamp(-1.0,1.0)})).collect::<Vec<_>>();
            if !pointers.is_empty() {
                hot.push(serde_json::json!({"cluster_id":id,"label":label,"chunks":pointers}));
            }
        }
    }
    Ok(serde_json::json!({"profile":profile,"hot":hot}))
}

#[cfg(test)]
mod cluster_tests {
    use super::*;
    #[test]
    fn stale_policy_boundaries_and_never_built() {
        let now = chrono::Utc::now();
        assert!(cluster_is_stale(
            &ClusterStaleness {
                built_at: None,
                clusters: 0,
                chunks_total: 0,
                chunks_since_build: 0,
                fraction_unseen: 0.0
            },
            now
        ));
        assert!(!cluster_is_stale(
            &ClusterStaleness {
                built_at: Some(now),
                clusters: 1,
                chunks_total: 100,
                chunks_since_build: 4,
                fraction_unseen: 0.04
            },
            now
        ));
        assert!(cluster_is_stale(
            &ClusterStaleness {
                built_at: Some(now),
                clusters: 1,
                chunks_total: 100,
                chunks_since_build: 5,
                fraction_unseen: 0.05
            },
            now
        ));
        assert!(cluster_is_stale(
            &ClusterStaleness {
                built_at: Some(now - chrono::Duration::days(8)),
                clusters: 1,
                chunks_total: 1000,
                chunks_since_build: 1,
                fraction_unseen: 0.001
            },
            now
        ));
    }
    #[test]
    fn kmeans_is_deterministic_and_safe_for_small_inputs() {
        let a = vec![
            (1, vec![1.0, 0.0]),
            (2, vec![0.9, 0.1]),
            (3, vec![0.0, 1.0]),
        ];
        let x = spherical_kmeans(&a, 8);
        let y = spherical_kmeans(&a, 8);
        assert_eq!(x, y);
        assert_eq!(x.len(), 3);
        assert_eq!(x.iter().map(|(_, m)| m.len()).sum::<usize>(), 3);
        assert!(spherical_kmeans(&[], 8).is_empty());
    }
}
