#!/usr/bin/env bash
# Copy the old spellbook files into table spellbook_pages (migration 0049).
#
#   substrate/import-spellbook-files.sh <vault> <database-url>
#
# <vault>/house/familiars/spellbook.json becomes scope `house`.
# <vault>/<room>/familiars/spellbook.json becomes scope `<room>`.
# Every file is checked before the first write; one bad file refuses the run.
# All pages land in one transaction. A second run replaces each page with the
# file's current content, so it is safe to repeat.
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: $0 <vault> <database-url>" >&2
  exit 2
fi
vault=${1%/}
database_url=$2
# Same rule as the spellbook_pages_scope_check constraint.
scope_pattern='^[a-z0-9]+(-[a-z0-9]+)*$'

psql_args=()
sql=""
count=0

add_page() {
  local scope=$1 file=$2 page
  if ! [[ $scope =~ $scope_pattern ]]; then
    echo "refused: '$scope' ($file) is not a valid scope" >&2
    exit 1
  fi
  if ! page=$(jq -ce 'if type == "object" then . else error("the page is not a JSON object") end' "$file"); then
    echo "refused: $file is not a JSON object page" >&2
    exit 1
  fi
  psql_args+=(-v "scope$count=$scope" -v "page$count=$page")
  sql+="INSERT INTO spellbook_pages (scope, page) VALUES (:'scope$count', :'page$count'::jsonb) ON CONFLICT (scope) DO UPDATE SET page = EXCLUDED.page;"$'\n'
  echo "page $scope <- $file"
  count=$((count + 1))
}

if [ -f "$vault/house/familiars/spellbook.json" ]; then
  add_page house "$vault/house/familiars/spellbook.json"
fi
for file in "$vault"/*/familiars/spellbook.json; do
  [ -f "$file" ] || continue
  room=$(basename "$(dirname "$(dirname "$file")")")
  [ "$room" = house ] && continue
  add_page "$room" "$file"
done

if [ "$count" -eq 0 ]; then
  echo "refused: no familiars/spellbook.json under $vault" >&2
  exit 1
fi

psql "$database_url" -X -q -1 -v ON_ERROR_STOP=1 "${psql_args[@]}" <<<"$sql"
echo "imported $count page(s)"
