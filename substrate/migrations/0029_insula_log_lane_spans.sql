-- ---------------------------------------------------------------------------
-- 0029: the lane-spans index for insula.log
-- ---------------------------------------------------------------------------
--
-- `insula.spans.recent` (crates/akasha/src/insula/query.rs::query_spans) is the
-- operator's door from a Pulse lane to a trace id. It asks one shape:
--
--     WHERE  house_id = ? AND room = ? AND operation = ?
--       AND  observed_at >= now() - window
--     ORDER BY observed_at DESC, span_id DESC
--     LIMIT  <= 100
--
-- Migration 0022 left no index that answers it. The closest two both fail:
--
--   idx_insula_log_vitals  (observed_at, house_id, room, spirit, component,
--                           layer, operation, phase, outcome_class)
--       leads on observed_at, so house/room/operation are in-index *filters*
--       rather than a search prefix: a backward scan must inspect every row in
--       the window across every room and every operation to find one lane's
--       newest few.
--   idx_insula_log_window  (observed_at DESC)
--       the same walk with no in-index filtering at all.
--
-- At the observed corpus size (~3 M rows) that is a multi-million-row walk to
-- return ten. The composite below makes house/room/operation a real equality
-- prefix, so the window bound becomes a range seek and the requested ordering
-- is the index's own: an index scan that stops at LIMIT rows.
--
-- `span_id DESC` is carried so the total order the query asks for is fully
-- satisfied by the index. Without it, timestamp ties force a Sort node over the
-- whole matched window instead of a bounded walk.
--
-- Deliberately absent: `phase` and `outcome_class`. Placing them between the
-- equality prefix and `observed_at` would break the ordering prefix for the
-- common unfiltered lane read, which is the read the drawer performs on every
-- click. They stay recheck filters, bounded by lane and by window.
--
-- Idempotent: CREATE INDEX IF NOT EXISTS, no data movement, no lock beyond the
-- build. Applies cleanly to a schema that already carries 0022. Plain
-- CREATE INDEX rather than CONCURRENTLY, so it belongs inside the one outer
-- transaction this lineage requires of every migration.

BEGIN;

CREATE INDEX IF NOT EXISTS idx_insula_log_lane_spans
    ON insula.log (house_id, room, operation, observed_at DESC, span_id DESC);

COMMIT;
