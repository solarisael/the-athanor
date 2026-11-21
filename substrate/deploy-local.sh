#!/usr/bin/env bash
set -euo pipefail
umask 077

usage() {
    cat <<'EOF'
Usage: bash substrate/deploy-local.sh LIVE_CHECKOUT DATA_ROOT RELEASE_BIN_DIR
       bash substrate/deploy-local.sh LIVE_CHECKOUT DATA_ROOT RELEASE_BIN_DIR --register-operator KEY DISPLAY_NAME UNIT UNIT_NAME

Update a provisioned systemd user unit from this source checkout.
Build the release binaries from this checkout before you run this script.
OMP sessions can stay open during the deployment.
Restart each open OMP session after the deployment. A session loads the adapter only at its start.
Run this script as its own process. A shell that runs it in-process skips the recovery step.
Set ATHANOR_DEPLOY_UNIT to select a unit other than athanor.service.
EOF
}

fail() {
    printf 'Deployment refused: %s\n' "$1" >&2
    exit 1
}

installed_process() {
    local pid executable pids

    pids=$(pgrep -u "$UID" -x 'athanor|athanor-host|athanor-substra|omp-keeper') || {
        if [[ $? == 1 ]]; then
            return 1
        fi
        return 2
    }

    for pid in $pids; do
        executable=$(readlink "/proc/$pid/exe") || continue
        if [[ $executable == "$live_bin/"* ]]; then
            printf '%s (%s)\n' "$pid" "$executable"
            return 0
        fi
    done
    return 1
}

maintenance() {
    systemd-run --user --wait --pipe --quiet \
        --property="EnvironmentFile=$env_file" \
        --property=UnsetEnvironment=ATHANOR_SUBSTRATE_TEST_DATABASE_URL \
        --setenv="ATHANOR_BACKUP_DIR=$receipt/database" -- "$@"
}

restart_notice() {
    local processes sessions status=0

    processes=$(pgrep -u "$UID" -a -x omp) || status=$?

    if ((status > 1)); then
        printf 'Could not list the OMP sessions. Restart each open OMP session.\n'
        return
    fi

    # OMP worker processes share the process name and are not sessions.
    sessions=$(awk '!/__omp_worker/' <<<"$processes")

    if [[ -z $sessions ]]; then
        printf 'No OMP session is open.\n'
        return
    fi

    printf 'Restart these OMP sessions to load the new adapter:\n%s\n' "$sessions"
}

finish() {
    local status=$?
    trap - EXIT

    if ((status != 0)); then
        if [[ $phase == migration ]]; then
            if ! systemctl --user stop "$unit"; then
                printf 'Could not stop %s. Stop it before recovery.\n' "$unit" >&2
            fi
            printf 'Recovery required. Keep the Host stopped. See %s and %s.\n' "$marker" "$receipt" >&2
            exit "$status"
        fi
        if [[ $phase == stopped && $was_active == yes ]] && ! systemctl --user start "$unit"; then
            printf 'Could not restart the original Host.\n' >&2
        fi
    fi

    if [[ $marker -ef $receipt/cutover.json ]]; then
        rm -- "$marker"
        if [[ $phase == complete ]]; then
            printf 'Deployment complete.\n'
            restart_notice
        fi
    else
        fail "the cutover marker changed during deployment: $marker"
    fi
    exit "$status"
}

if [[ ${1:-} == --help ]]; then
    usage
    exit 0
fi
if (($# != 3 && $# != 8)) || [[ $# == 8 && $4 != --register-operator ]]; then
    usage >&2
    exit 2
fi

# `finish` must run on every exit. A shell that runs this script in-process
# does not run the EXIT trap. A failed step then leaves the Host down with the
# cutover marker in place and no recovery message.
[[ -e /proc/$$/exe ]] || fail 'this shell is not its own process; run the script with the system bash'

source_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
live=$(realpath -e -- "$1")
data=$(realpath -e -- "$2")
built=$(realpath -e -- "$3")
shift 3

unit=${ATHANOR_DEPLOY_UNIT:-athanor.service}
live_bin="$live/target/release"
live_adapter="$live/adapters/omp"
env_file="$data/substrate.env"
config="$data/config/runtime.json"
binaries=(athanor athanor-host athanor-substrate)

[[ $unit =~ ^[A-Za-z0-9_.@-]+\.service$ && $unit != -* ]] || fail 'invalid systemd user unit name'
[[ $source_root != "$live" ]] || fail 'the source and live checkouts must be separate'
[[ -r $env_file && -r $config ]] || fail 'the user unit must provision the runtime and database environment first'
[[ -d $live_bin && ! -L $live_bin && -d $live_adapter && ! -L $live_adapter ]] ||
    fail 'the live binary and adapter paths must be ordinary directories'
for bin in "${binaries[@]}"; do
    [[ -x $built/$bin && -x $live_bin/$bin && ! $built/$bin -ef $live_bin/$bin ]] ||
        fail "missing or unstaged release binary: $bin"
done

unit_start=$(systemctl --user show "$unit" -p ExecStart --value)
unit_env=$(systemctl --user show "$unit" -p EnvironmentFiles --value)
unit_vars=$(systemctl --user show "$unit" -p Environment --value)
port=$(jq -er '.hostPort | numbers | select(. >= 1 and . <= 65535 and floor == .)' "$config")
room=$(jq -er '.defaultRoom | select(test("^[a-z0-9]+(-[a-z0-9]+)*$"))' "$config")
state=$(jq -er '.operatorStateRoot | select(startswith("/"))' "$config")
vault=$(jq -er '.roomsRoot | select(startswith("/"))' "$config")
rooms=$(jq -er '.rooms | select(length > 0) | .[].room | select(test("^[a-z0-9]+(-[a-z0-9]+)*$"))' "$config")

[[ $unit_start == *"path=$live_bin/athanor ;"* && $unit_env == "$env_file (ignore_errors=no)"* &&
    " $unit_vars " == *" ATHANOR_DATA_ROOT=$data "* &&
    " $unit_vars " == *" ATHANOR_PROGRAM_ROOT=$live_bin "* ]] ||
    fail 'the user unit does not use this checkout and database environment'
for configured_room in $rooms; do
    [[ -s $vault/$configured_room/.omp/runtime/athanor-house-state.json ]] ||
        fail "missing room state: $configured_room"
done
if [[ $# != 0 ]]; then
    [[ -n $2 && -n $3 && -n $4 && -n $5 ]] || fail 'operator registration requires four nonempty values'
    jq -e --arg key "$2" '.rooms | any(.operator == $key)' "$config" >/dev/null ||
        fail 'no configured room uses this operator key'
fi

# The receipt keeps the old adapter intact and gives the operator the exact restore input.
exec 9>"$data/deploy.lock"
flock -n 9 || fail 'another deployment holds the lock'
cutover="$state/cutover"
marker="$cutover/cutover.json"
[[ ! -e $marker && ! -L $marker ]] || fail "a cutover already exists: $marker"
receipt=$(mktemp -d -- "$(dirname -- "$live")/.athanor-release.XXXXXXXX")
release_device=$(stat -c %d -- "$receipt")
binary_device=$(stat -c %d -- "$live_bin")
adapter_device=$(stat -c %d -- "$live_adapter")

[[ $release_device == "$binary_device" && $release_device == "$adapter_device" ]] ||
    fail 'the release and both live directories must share a filesystem'
mkdir -- "$receipt/new" "$receipt/previous" "$receipt/database" "$receipt/adapter"
# Test the atomic swap before the Host stops.
mv --exchange --no-copy -T -- "$receipt/new" "$receipt/adapter"
for bin in "${binaries[@]}"; do
    install -m 755 -- "$built/$bin" "$receipt/new/$bin"
    cp -p -- "$live_bin/$bin" "$receipt/previous/$bin"
done
rsync -a --exclude=/node_modules/ --exclude=/.git/ -- "$source_root/adapters/omp/" "$receipt/adapter/"
if [[ -e $live_adapter/node_modules ]]; then
    cp -a --reflink=auto -- "$live_adapter/node_modules" "$receipt/adapter/"
fi

was_active=no
if systemctl --user is-active --quiet "$unit"; then
    was_active=yes
fi
phase=prepared
mkdir -p -- "$cutover"
release_id=$(basename -- "$receipt")
jq -n --arg id "linux-${release_id##*.}" --arg path "$receipt" \
    '{format: 1, cutoverId: $id, release: $id, linuxReceipt: $path}' >"$receipt/cutover.json"
ln -T -- "$receipt/cutover.json" "$marker" || fail "cannot acquire the cutover: $marker"
trap finish EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

printf 'Release receipt: %s\n' "$receipt"
phase=stopped
systemctl --user stop "$unit"
if remaining=$(installed_process); then
    fail "an installed process is still active: $remaining"
elif [[ $? != 1 ]]; then
    fail 'cannot check for active release processes'
fi
listeners=$(ss -H -ltn "sport = :$port")
[[ -z $listeners ]] || fail "another process listens on the Host port $port"

maintenance "$receipt/new/athanor-substrate" backup --output-dir "$receipt/database" --keep 1 \
    >"$receipt/database/backup.json"
manifest=$(jq -er '.dump | select(endswith(".dump")) | sub("\\.dump$"; ".manifest.json")' \
    "$receipt/database/backup.json")
[[ -s $receipt/database/$manifest ]] || fail 'the backup manifest is missing'
printf 'Database backup: %s\n' "$receipt/database/$manifest"

phase=migration
maintenance "$receipt/new/athanor-substrate" migrations
if [[ $# != 0 ]]; then
    maintenance "$receipt/new/athanor-substrate" operator-register \
        --key "$2" --display-name "$3" --unit "$4" --unit-name "$5"
fi

for bin in "${binaries[@]}"; do
    mv -fT -- "$receipt/new/$bin" "$live_bin/$bin"
done
mv --exchange --no-copy -T -- "$receipt/adapter" "$live_adapter"
systemctl --user start "$unit"
for ((attempt = 0; attempt < 90; attempt++)); do
    if curl -fsS --max-time 1 "http://127.0.0.1:$port/room/$room/health" 2>/dev/null |
        jq -e '.status == "ok"' >/dev/null; then
        break
    fi
    sleep 1
    case $(systemctl --user show "$unit" -p ActiveState --value) in
    failed | inactive) fail "the Host stopped before readiness; see journalctl --user -u $unit" ;;
    esac
done
((attempt < 90)) || fail 'the new Host did not serve scoped health'
maintenance "$live_bin/athanor-substrate" health
systemctl --user is-active --quiet "$unit"
phase=complete
