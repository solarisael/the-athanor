#!/usr/bin/env bash
set -euo pipefail
umask 077

usage() {
    cat <<'EOF'
Usage: bash substrate/test-ignored-db.sh [--with-retention]

Run the database tests that have #[ignore] on a throwaway PostgreSQL server.
CI does not run these tests.

The script makes a new PostgreSQL data directory, starts a server on
127.0.0.1:55461, runs the tests, stops the server, and removes the directory.
It needs initdb, pg_ctl, createdb, and psql on PATH, and the pgvector extension.
Set CARGO_TARGET_DIR before you run it if the checkout needs its own target.

--with-retention  Also run the Insula retention test. It waits 300 seconds.

Run this script as its own process. A shell that runs it in-process skips the cleanup.
The four tests that need a NATS server do not run.
EOF
}

fail() {
    printf 'Test run refused: %s\n' "$1" >&2
    exit 1
}

with_retention=0
case "${1:-}" in
    '') ;;
    --with-retention) with_retention=1 ;;
    -h | --help) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
esac

# `finish` must run on every exit. A shell that runs this script in-process skips EXIT traps.
[[ -e /proc/$$/exe ]] || fail 'this shell is not its own process; run the script with the system bash'

for tool in initdb pg_ctl createdb psql cargo; do
    command -v "$tool" >/dev/null || fail "$tool is not on PATH"
done

source_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
# Some tests refuse each server except this address. The values are in their source.
host=127.0.0.1
port=55461
orihon_database=orihon_live_test_973137750b62482e8c884c62f6b54891
retention_database=athanor_insula_retention_test_1

# A test must not find the configuration of a running House.
while IFS= read -r name; do
    unset "$name"
done < <(env | awk -F= '/^(ATHANOR_|SOLARISAEL_|PG|DATABASE_URL|NATS)/{print $1}')

data=$(mktemp -d "${TMPDIR:-/tmp}/athanor-test-pg.XXXXXXXX")
logs=$(mktemp -d "${TMPDIR:-/tmp}/athanor-test-logs.XXXXXXXX")

finish() {
    pg_ctl -D "$data" -m immediate stop >/dev/null 2>&1 || true
    rm -rf -- "$data"
}
trap finish EXIT

initdb -D "$data" -U tester --auth=trust -E UTF8 >"$logs/initdb.log" 2>&1 || fail "initdb failed; read $logs/initdb.log"
pg_ctl -D "$data" -l "$logs/postgres.log" -o "-p $port -k $data -c listen_addresses=$host" -w start >/dev/null \
    || fail "PostgreSQL did not start on $host:$port; read $logs/postgres.log"

url() {
    printf 'postgres://tester@%s:%s/%s' "$host" "$port" "$1"
}

migrate() {
    local file

    for file in "$source_root"/substrate/migrations/*.sql; do
        psql -q -v ON_ERROR_STOP=1 -h "$host" -p "$port" -U tester -d "$1" -f "$file" >>"$logs/migrate-$1.log" 2>&1 \
            || fail "migration $(basename -- "$file") failed on $1; read $logs/migrate-$1.log"
    done
}

for database in athanor_canon_test athanor_test_isolation athanor_hallway_temp "$orihon_database"; do
    createdb -h "$host" -p "$port" -U tester "$database"
done
migrate "$orihon_database"

failed=0
run() {
    local label=$1 database=$2
    shift 2

    printf '%s\n' "$label"
    if ATHANOR_SUBSTRATE_TEST_DATABASE_URL=$(url "$database") cargo test "$@" >"$logs/$label.log" 2>&1; then
        awk '/^test result:/{passed += $4} END{printf "  passed: %d\n", passed}' "$logs/$label.log"
    else
        failed=1
        awk '/^test .* \.\.\. FAILED/{print "  " $0} /^error(\[|:)/{print "  " $0}' "$logs/$label.log"
        printf '  FAILED; read %s\n' "$logs/$label.log"
    fi
}

cd -- "$source_root"
export ATHANOR_SUBSTRATE_TEST_SCHEMA=solarisael_tuner_test_run

# The desktop crate needs system libraries that the tests do not use.
run workspace athanor_canon_test --workspace --exclude athanor-pulse --no-fail-fast -- --ignored --test-threads=1 \
    --skip generated_credentials_enforce_delivery_and_reply_boundaries \
    --skip crane_outbox_restart_dedup_receipt_replay_expiry_and_poison_contract \
    --skip serve_delivers_until_cancelled_and_leaves_no_lease_behind \
    --skip retired_rooms_close_together_and_forget_their_hallway_consumers \
    --skip automatic_retention_fires_after_real_startup_delay_and_preserves_recent_rows \
    --skip supersession_refuses_foreign_and_missing_predecessors_atomically \
    --skip hallway_temp_session_exchanges_messages_without_persistent_state \
    --skip pg_ingest_reconciles_a_lost_acknowledgment \
    --skip pg_node_retry_rolls_back_the_duplicate_job

run room-isolation athanor_test_isolation -p akasha --test room_memory_isolation -- --ignored --test-threads=1

export ATHANOR_HALLWAY_TEMP_PROOF=1
export ATHANOR_HALLWAY_TEMP_DATABASE_URL
ATHANOR_HALLWAY_TEMP_DATABASE_URL=$(url athanor_hallway_temp)
run hallway-temp athanor_hallway_temp -p akasha --test hallway_integration -- --ignored --test-threads=1 \
    --exact hallway_temp_session_exchanges_messages_without_persistent_state
unset ATHANOR_HALLWAY_TEMP_PROOF ATHANOR_HALLWAY_TEMP_DATABASE_URL

run orihon-live "$orihon_database" -p origami --lib -- --ignored --test-threads=1 orihon::live::store::tests::pg_

if (( with_retention )); then
    createdb -h "$host" -p "$port" -U tester "$retention_database"
    migrate "$retention_database"
    run retention "$retention_database" -p akasha --test insula_ingest_contract -- --ignored --test-threads=1 \
        --exact automatic_retention_fires_after_real_startup_delay_and_preserves_recent_rows
fi

if (( failed )); then
    printf 'One or more test groups failed. Logs: %s\n' "$logs"
    exit 1
fi

rm -rf -- "$logs"
printf 'All groups passed.\n'
