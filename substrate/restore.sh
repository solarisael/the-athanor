#!/usr/bin/env bash
# Restore solarisael_memory from a custom-format dump.
# Usage: ./restore.sh path/to/dumpfile.dump
# Drops + recreates schema; --clean handles existing objects.
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: $0 path/to/dumpfile.dump" >&2
  exit 2
fi

dump="$1"
if [[ ! -f "$dump" ]]; then
  echo "not a file: $dump" >&2
  exit 2
fi
dump="$(realpath "$dump")"

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

echo "restoring $dump into $PGDATABASE@$PGHOST:$PGPORT (--clean --if-exists)"
PGPASSWORD="${PGPASSWORD:-}" pg_restore \
  -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$PGDATABASE" \
  --clean --if-exists --no-owner --no-acl \
  "$dump"

echo "done"
