#!/usr/bin/env bash
# Usage: substrate/import-spirit-texts.sh <manifest.tsv> <database-url> <written-by>
set -euo pipefail
export LC_ALL=C PGCLIENTENCODING=UTF8

if [ "$#" -ne 3 ]; then
  echo "usage: $0 <manifest.tsv> <database-url> <written-by>" >&2
  exit 2
fi

manifest=$1
database_url=$2
written_by=$3

if ! [[ -f $manifest && -r $manifest ]]; then
  echo "refused: cannot read manifest $manifest" >&2
  exit 1
fi
if [[ -z ${written_by// /} ]]; then
  echo 'refused: written-by is blank' >&2
  exit 1
fi

line_number=0
refuse_line() {
  echo "refused: line $line_number: $1" >&2
  exit 1
}

# Bash read skips NUL bytes unless NUL is the delimiter.
if IFS= read -r -d '' prefix < "$manifest"; then
  breaks=${prefix//[^$'\n']/}
  line_number=$(( ${#breaks} + 1 ))
  refuse_line 'manifest contains a NUL byte'
fi
unset prefix

fields=$'^([^\t]*)\t([^\t]*)\t([^\t]*)\t([^\t]*)$'
room_pattern='^[a-z0-9]+(-[a-z0-9]+)*$'
count=0
psql_args=(-v "author=$written_by")
writes=''
checks=''
while IFS= read -r line || [[ -n $line ]]; do
  line_number=$((line_number + 1))
  [[ -z $line || $line == '#'* ]] && continue
  [[ $line =~ $fields ]] || refuse_line 'expected four tab-separated fields'
  room=${BASH_REMATCH[1]}
  spirit=${BASH_REMATCH[2]}
  kind=${BASH_REMATCH[3]}
  file=${BASH_REMATCH[4]}

  printf '%s' "$line" | iconv -f UTF-8 -t UTF-8 >/dev/null || refuse_line 'invalid UTF-8 in manifest'
  [[ $room =~ $room_pattern ]] || refuse_line 'invalid room'
  [[ -n $spirit && $spirit != ' '* && $spirit != *' ' ]] || refuse_line 'invalid spirit'
  [[ $kind == identity || $kind == presence || $kind == spirit ]] || refuse_line 'invalid kind'
  [[ $file == /* && -f $file && -r $file && -s $file ]] || refuse_line 'expected an absolute path to a readable, nonempty file'
  iconv -f UTF-8 -t UTF-8 < "$file" >/dev/null || refuse_line 'invalid UTF-8 in file'
  if [ "$(tr -d '\000' < "$file" | wc -c)" -ne "$(wc -c < "$file")" ]; then
    refuse_line 'file contains a NUL byte'
  fi
  # enough: the text travels as one psql argument, and Linux limits one
  # argument to 131,072 bytes. Base64 adds one third. Send the text on stdin
  # when a spirit text can exceed 90,000 bytes.
  if [ "$(wc -c < "$file")" -gt 90000 ]; then
    refuse_line 'file is larger than 90,000 bytes'
  fi

  digest=$(sha256sum "$file") || refuse_line 'cannot hash file'
  digest=${digest#\\}
  digest=${digest%% *}
  b64=$(base64 -w0 "$file") || refuse_line 'cannot encode file'
  psql_args+=(-v "room$count=$room" -v "spirit$count=$spirit" -v "kind$count=$kind" -v "sha$count=$digest" -v "b64$count=$b64")
  key="room = :'room$count' AND spirit = :'spirit$count' AND kind = :'kind$count'"
  writes+="
WITH latest AS (SELECT revision, sha256 FROM spirit_texts WHERE $key ORDER BY revision DESC LIMIT 1)
INSERT INTO spirit_texts (room, spirit, kind, revision, body, sha256, written_by)
SELECT :'room$count', :'spirit$count', :'kind$count', COALESCE((SELECT revision FROM latest), 0) + 1,
       convert_from(decode(:'b64$count', 'base64'), 'UTF8'), :'sha$count', :'author'
WHERE (SELECT sha256 FROM latest) IS DISTINCT FROM :'sha$count';
\if :ROW_COUNT
\echo imported
\else
\echo same
\endif
"
  checks+="
SELECT COALESCE((SELECT sha256 = :'sha$count' AND encode(sha256(convert_to(body, 'UTF8')), 'hex') = :'sha$count'
                 FROM spirit_texts WHERE $key ORDER BY revision DESC LIMIT 1), false) AS verified \gset
\if :verified
\else
\echo mismatch at line $line_number
\endif
"
  count=$((count + 1))
done < "$manifest"

results=$(psql "$database_url" -X -qAt -v ON_ERROR_STOP=1 "${psql_args[@]}" <<SQL
BEGIN;
$writes
COMMIT;
$checks
SQL
)
imported=0
same=0
while IFS= read -r result; do
  case $result in
    imported) imported=$((imported + 1)) ;;
    same) same=$((same + 1)); echo same ;;
    '') ;;
    *) echo "$result" >&2; exit 1 ;;
  esac
done <<< "$results"

echo "imported $imported, same $same"
