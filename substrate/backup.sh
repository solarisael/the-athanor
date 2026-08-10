#!/usr/bin/env bash
# Snapshot solarisael_memory to the substrate state directory.
# Custom-format dump (-Fc) — compressed, parallel-restorable via pg_restore.
# Rotation keeps the latest $KEEP backups; older ones get pruned.
set -euo pipefail

cd "$(dirname "$0")"
# shellcheck source=state_paths.sh
source "$(dirname "$0")/state_paths.sh"

load_env_file() {
  local file="$1" raw key value
  [[ -f "$file" ]] || return
  while IFS= read -r raw || [[ -n "$raw" ]]; do
    raw="${raw%$'\r'}"
    [[ "$raw" =~ ^[[:space:]]*(#|$) ]] && continue
    if [[ "$raw" != *=* ]]; then
      printf 'invalid dotenv line in %s: %s\n' "$file" "$raw" >&2
      exit 2
    fi
    key="${raw%%=*}"
    value="${raw#*=}"
    key="${key#"${key%%[![:space:]]*}"}"
    key="${key%"${key##*[![:space:]]}"}"
    value="${value#"${value%%[![:space:]]*}"}"
    value="${value%"${value##*[![:space:]]}"}"
    if [[ ! "$key" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]]; then
      printf 'invalid dotenv key in %s: %s\n' "$file" "$key" >&2
      exit 2
    fi
    if [[ "$value" == \"*\" && "$value" == *\" ]]; then
      value="${value:1:${#value}-2}"
    elif [[ "$value" == \'*\' && "$value" == *\' ]]; then
      value="${value:1:${#value}-2}"
    fi
    if [[ -z "${!key+x}" ]]; then
      export "$key=$value"
    fi
  done < "$file"
}

load_env_file "$SUBSTRATE_DOTENV"

: "${PGHOST:=127.0.0.1}"
: "${PGPORT:=5432}"
: "${PGUSER:=solarisael}"
: "${PGDATABASE:=solarisael_memory}"
KEEP="${KEEP:-14}"

if [[ ! "$PGDATABASE" =~ ^[A-Za-z0-9_.-]+$ ]]; then
  printf 'PGDATABASE is not filename-safe: %s\n' "$PGDATABASE" >&2
  exit 2
fi
if [[ ! "$KEEP" =~ ^[1-9][0-9]*$ ]]; then
  printf 'KEEP must be a positive integer: %s\n' "$KEEP" >&2
  exit 2
fi

BACKUP_DIR="${SOLARISAEL_BACKUP_DIR:-$SUBSTRATE_STATE_DIR/backups}"
mkdir -p "$BACKUP_DIR"

if [[ -n "${SOLARISAEL_BACKUP_DATABASE_URL:-}" ]]; then
  BACKUP_TARGET="$SOLARISAEL_BACKUP_DATABASE_URL"
  BACKUP_PASSWORD="${SOLARISAEL_BACKUP_PASSWORD:-}"
  PGDATABASE="${PGDATABASE:-substrate}"
else
  BACKUP_TARGET=""
  BACKUP_PASSWORD="${PGPASSWORD:-}"
fi

ts="$(date +%Y-%m-%d_%H%M%S)"
out="$BACKUP_DIR/${PGDATABASE}_${ts}.dump"

if [[ -n "$BACKUP_TARGET" ]]; then
  PGPASSWORD="$BACKUP_PASSWORD" pg_dump \
    --dbname="$BACKUP_TARGET" \
    -Fc --no-owner --no-acl \
    -f "$out"
else
  PGPASSWORD="$BACKUP_PASSWORD" pg_dump \
    -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$PGDATABASE" \
    -Fc --no-owner --no-acl \
    -f "$out"
fi

bytes=$(stat -c %s "$out")
printf 'wrote %s (%s bytes)\n' "$out" "$bytes"

# Rotate: keep the most recent $KEEP, delete the rest.
mapfile -t old < <(ls -1t "$BACKUP_DIR"/${PGDATABASE}_*.dump 2>/dev/null | tail -n +"$((KEEP+1))" || true)
for f in "${old[@]}"; do
  printf 'pruning old: %s\n' "$f"
  rm -f -- "$f"
done
