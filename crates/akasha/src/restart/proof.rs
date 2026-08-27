//! Attempt-scoped proof that a restart verifier is the keeper-launched child.
//!
//! A room capability proves which room is speaking; every process in that room
//! can read it, including the predecessor. This module supplies the second
//! fence. Each successful `relaunching` transition rotates a random proof,
//! stores only its sha256, and hands the plaintext to the keeper exactly once.
//! The keeper passes it to that attempt's child. Every operation locks the
//! intent first in the caller, then the proof row here, so verify and retry have
//! one deterministic winner.

use super::{constant_time_equal, refusal, sha256_hex};
use crate::config::AppError;
use sqlx::{Postgres, Row, Transaction};

pub(super) async fn rotate(
    tx: &mut Transaction<'_, Postgres>,
    intent_id: &str,
    claim_epoch: i64,
    relaunch_attempt: i32,
) -> Result<String, AppError> {
    let proof: String = sqlx::query_scalar("SELECT encode(gen_random_bytes(32),'hex')")
        .fetch_one(&mut **tx)
        .await?;
    sqlx::query(
        "INSERT INTO restart.successor_proofs (intent_id,claim_epoch,relaunch_attempt,proof_hash) \
         VALUES ($1::text::uuid,$2,$3,$4) \
         ON CONFLICT (intent_id) DO UPDATE SET claim_epoch=EXCLUDED.claim_epoch, \
         relaunch_attempt=EXCLUDED.relaunch_attempt,proof_hash=EXCLUDED.proof_hash,minted_at=NOW()",
    )
    .bind(intent_id)
    .bind(claim_epoch)
    .bind(relaunch_attempt)
    .bind(sha256_hex(proof.as_bytes()))
    .execute(&mut **tx)
    .await?;
    Ok(proof)
}

pub(super) async fn require_current(
    tx: &mut Transaction<'_, Postgres>,
    intent_id: &str,
    claim_epoch: i64,
    relaunch_attempt: i32,
    presented_proof: &str,
) -> Result<(), AppError> {
    let stored = sqlx::query(
        "SELECT claim_epoch,relaunch_attempt,proof_hash \
         FROM restart.successor_proofs WHERE intent_id=$1::text::uuid FOR UPDATE",
    )
    .bind(intent_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(stored) = stored else {
        return Err(not_authorized());
    };
    let stored_epoch: i64 = stored.try_get("claim_epoch")?;
    let stored_attempt: i32 = stored.try_get("relaunch_attempt")?;
    let stored_hash: String = stored.try_get("proof_hash")?;
    let presented_hash = sha256_hex(presented_proof.as_bytes());
    if stored_epoch != claim_epoch
        || stored_attempt != relaunch_attempt
        || !constant_time_equal(presented_hash.as_bytes(), stored_hash.as_bytes())
    {
        return Err(not_authorized());
    }
    Ok(())
}

pub(super) async fn clear(
    tx: &mut Transaction<'_, Postgres>,
    intent_id: &str,
) -> Result<(), AppError> {
    sqlx::query("DELETE FROM restart.successor_proofs WHERE intent_id=$1::text::uuid")
        .bind(intent_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

fn not_authorized() -> AppError {
    refusal(
        "verify_not_authorized",
        "only the keeper-launched successor for the current attempt can verify this intent",
    )
}
