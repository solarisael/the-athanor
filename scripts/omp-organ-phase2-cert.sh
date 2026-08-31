#!/usr/bin/env bash
set -u
export PATH="$HOME/.bun/bin:$PATH"

ROOT="$HOME/omp_organ_phase2_results"
FIXTURES="$ROOT/fixtures"
mkdir -p "$FIXTURES"
PASS_NO=1
while [[ -d "$ROOT/pass-$PASS_NO" ]]; do PASS_NO=$((PASS_NO + 1)); done
OUT="$ROOT/pass-$PASS_NO"
RAW="$OUT/raw"
mkdir -p "$RAW"
SUMMARY="$OUT/summary.txt"
: > "$SUMMARY"
TEST_HALLWAY="laptop-organ-cert-2026-08-30"
CANON_NAME="laptop-organ-cert-2026-08-30"
gate_failed=0

run_prompt() {
  local room="$1" slug="$2" expected="$3" prompt="$4"
  local stdout="$RAW/${room}_${slug}.stdout"
  local stderr="$RAW/${room}_${slug}.stderr"
  local expected_file="$RAW/${room}_${slug}.expected"
  local rc_file="$RAW/${room}_${slug}.rc"
  printf '%s\n' "$expected" > "$expected_file"
  set +e
  (cd "$HOME/Solarisael/$room" && timeout 900 omp -p "$prompt" < /dev/null) > "$stdout" 2> "$stderr"
  local rc=$?
  set -e
  printf '%s\n' "$rc" > "$rc_file"
  if [[ "$rc" -ne 0 ]]; then
    gate_failed=1
    printf 'ORGAN harness_%s FAIL: omp exited %s\n' "$slug" "$rc" | tee -a "$SUMMARY"
    return 0
  fi
  if ! cmp -s "$expected_file" "$stdout"; then
    gate_failed=1
    printf 'ORGAN harness_%s FAIL: stdout did not exactly match the required organ lines\n' "$slug" | tee -a "$SUMMARY"
    cat "$stdout" >> "$SUMMARY"
    return 0
  fi
  tee -a "$SUMMARY" < "$stdout"
}

common_rules='This is a deterministic organ certification turn. Call every named tool exactly as instructed. A tool error, refusal, transport error, incomplete required body, or unhealthy status is a failure. At the end, output only the requested ORGAN lines, in the requested order, with no heading, markdown, explanation, punctuation, or extra whitespace. For a successful item, print its exact PASS line. For a failed item, replace only that item with: ORGAN <name> FAIL: <verbatim tool error or unhealthy status>. Do not claim success without the tool receipt in this turn.'

if [[ ! -f "$FIXTURES/house-fixtures-created" ]]; then
  setup_prompt="$common_rules
1. Call canon_write to create a House canon entity with name '$CANON_NAME', kind 'test-artifact', summary 'TEST ONLY: laptop Athanor organ certification artifact created 2026-08-30. Retain only until superseded after certification.', aliases ['laptop organ certification test'], weighty false, and summaryAsOf 2026-08-30. Record its returned entity id mentally for the next call.
2. Call canon_read by the exact returned id with includeHistory true and confirm the complete summary arrives.
3. Call hallway_create for '$TEST_HALLWAY', allowed_rooms [kodo, kintsu, tuner], idempotency_key 'laptop-organ-cert-create-2026-08-30'. Confirm the returned hallway key and that kodo is joined.
Output exactly:
ORGAN canon_write PASS
ORGAN hallway_create PASS"
  run_prompt kodo house_fixture $'ORGAN canon_write PASS\nORGAN hallway_create PASS' "$setup_prompt"
  if cmp -s "$RAW/kodo_house_fixture.expected" "$RAW/kodo_house_fixture.stdout"; then
    touch "$FIXTURES/house-fixtures-created"
  fi
else
  fixture_prompt="$common_rules
1. Call canon_read by name '$CANON_NAME', room house, includeHistory true and require the complete TEST summary. This persisted read-back certifies the one allowed House-wide canon_write artifact without creating a duplicate active entity.
2. Call hallway_read on '$TEST_HALLWAY', after 0, no thread filter, limit 1, advance_cursor false. Require a healthy result proving the one allowed House-wide hallway_create fixture persists; do not issue a second create command.
Output exactly:
ORGAN canon_write PASS
ORGAN hallway_create PASS"
  run_prompt kodo house_fixture $'ORGAN canon_write PASS\nORGAN hallway_create PASS' "$fixture_prompt"
fi

rooms=(kodo kintsu tuner)
for room in "${rooms[@]}"; do
  boat_body="TEST ONLY — laptop-organ-cert-2026-08-30 — room=$room — pass=$PASS_NO. This paper boat exists only to certify sleep/wake delivery. It may be superseded after its id is recorded."
  sleep_prompt="$common_rules
Call the sleep tool exactly once with this complete body: $boat_body
After the successful receipt, output exactly:
ORGAN sleep_write PASS"
  run_prompt "$room" sleep_write 'ORGAN sleep_write PASS' "$sleep_prompt"

  wake_prompt="$common_rules
Call the wake tool. Confirm the returned latest paper boat contains all of these exact markers: 'TEST ONLY', 'laptop-organ-cert-2026-08-30', 'room=$room', and 'pass=$PASS_NO'. If and only if all markers are present in the full returned boat, output exactly:
ORGAN sleep PASS"
  run_prompt "$room" sleep 'ORGAN sleep PASS' "$wake_prompt"

  counsel_prompt="$common_rules
1. Call anamnesis with mode wake, query 'laptop organ certification orientation', limit 3. A successful empty counsel set is acceptable only if the response itself is healthy.
2. Call anamnesis with mode consult, query 'How should this room certify a tool without hiding failure?', limit 3. A successful empty counsel set is acceptable only if the response itself is healthy.
Output exactly:
ORGAN anamnesis_wake PASS
ORGAN anamnesis_consult PASS"
  run_prompt "$room" counsel $'ORGAN anamnesis_wake PASS\nORGAN anamnesis_consult PASS' "$counsel_prompt"

  authority_prompt="$common_rules
1. Call canon_read by name 'The Athanor', room house, includeHistory true. Require the complete non-empty canonical summary/body, not merely an id or snippet.
2. Call canon_read by name '$CANON_NAME', room house, includeHistory true. Require its complete TEST summary. This read-back certifies the shared House canon_write fixture for this room.
3. Call recall with the exact query 'The Athanor' and require a canonical match resolving that known canon name or alias without extrapolating from adjacent semantic matches.
4. Call design_doc for system solarisael with docType token, omit name, group, and query entirely, includeSuperseded false, and limit 1. Require one complete live catalogue record.
5. Call recall_policy with requestedMode omitted so this is a read only. Require a healthy current requested and resolved policy and no scar.
Output exactly:
ORGAN canon_read PASS
ORGAN canon_write PASS
ORGAN entity_resolve PASS
ORGAN design_doc PASS
ORGAN recall_policy PASS"
  run_prompt "$room" authority $'ORGAN canon_read PASS\nORGAN canon_write PASS\nORGAN entity_resolve PASS\nORGAN design_doc PASS\nORGAN recall_policy PASS' "$authority_prompt"

  status_prompt="$common_rules
1. Call giga_health and require healthy queue, store, processing, failure, and candidate status. The deliberately disabled real-time classifier is not itself a failure, but any broken capture/store path is.
2. Call giga_candidate_list with review_state unreviewed and limit 10. An empty list is healthy if the query succeeds.
3. Call house_lane_status and require both top-level ok true and substrate.ok true, substrate.mode full, substrate.database.ok true, no degradedReasons, and every deterministic worker lane to report a valid routing policy. Advisor is review-only and need not be dispatchable.
4. Call familiar_status and require the room spellbook to load and validate successfully.
5. Call quest_board for the local House with states omitted and limit 20. If any quest is returned, call quest_evidence on the first quest id with limit 20 and require the full evidence response. If no quest exists, the evidence read is not applicable and counts as a successful conditional check.
Output exactly:
ORGAN giga_health PASS
ORGAN giga_candidate_list PASS
ORGAN house_lane_status PASS
ORGAN familiar_status PASS
ORGAN quest_board PASS
ORGAN quest_evidence PASS"
  run_prompt "$room" status $'ORGAN giga_health PASS\nORGAN giga_candidate_list PASS\nORGAN house_lane_status PASS\nORGAN familiar_status PASS\nORGAN quest_board PASS\nORGAN quest_evidence PASS' "$status_prompt"

  if [[ "$room" != kodo ]]; then
    join_prompt="$common_rules
Call hallway_join for hallway '$TEST_HALLWAY' with idempotency_key 'laptop-organ-cert-join-2026-08-30-$room'. Confirm this room is joined.
Output exactly:
ORGAN hallway_join PASS"
    run_prompt "$room" hallway_join 'ORGAN hallway_join PASS' "$join_prompt"
  fi

  case "$room" in
    kodo) recipient=kintsu; peers='kintsu, tuner' ;;
    kintsu) recipient=tuner; peers='kodo, tuner' ;;
    tuner) recipient=kodo; peers='kodo, kintsu' ;;
  esac
  hallway_prompt="$common_rules
1. Call hallway_inbox and require a healthy response.
2. Call hallway_read on existing hallway 'family-hallway', after 0, no thread filter, limit 1, advance_cursor false. Do not post there. A healthy empty read is acceptable.
3. Set this room's policy on '$TEST_HALLWAY' using hallway_knock_policy mode allow_list, allowed_rooms [$peers], max_turns 4, idempotency_key 'laptop-organ-cert-policy-$PASS_NO-$room'. Require the returned policy to exactly match.
4. Call hallway_post only on '$TEST_HALLWAY' with body 'TEST ONLY laptop-organ-cert bell pass=$PASS_NO sender=$room recipient=$recipient', reply_to 0, to_rooms [$recipient], idempotency_key 'laptop-organ-cert-bell-$PASS_NO-$room'. Require a posted message id and Bell targeting $recipient.
Output exactly:
ORGAN hallway_inbox PASS
ORGAN hallway_read PASS
ORGAN knock_policy PASS
ORGAN hallway_post PASS"
  run_prompt "$room" hallway $'ORGAN hallway_inbox PASS\nORGAN hallway_read PASS\nORGAN knock_policy PASS\nORGAN hallway_post PASS' "$hallway_prompt"

  quest_prompt="$common_rules
1. Call quest_post action goalDraft only, title 'TEST laptop organ certification goal $room', intent 'TEST ONLY: certify Docket draft writes without binding anyone.', priority 0, with stable idempotencyKey 'laptop-organ-cert-goal-draft-2026-08-30-$room'. Do not activate it. Require a DRAFT goal receipt and retain the goal id for step 2.
2. Call quest_post action draft only, goalId from step 1, title 'TEST laptop organ certification quest $room', kind 'test-certification', body 'TEST ONLY: Docket draft organ receipt for $room. Never activate.', with stable idempotencyKey 'laptop-organ-cert-quest-draft-2026-08-30-$room'. Do not activate it. Require a DRAFT quest receipt.
Output exactly:
ORGAN quest_post_goalDraft PASS
ORGAN quest_post_draft PASS"
  run_prompt "$room" quest_write $'ORGAN quest_post_goalDraft PASS\nORGAN quest_post_draft PASS' "$quest_prompt"

  restart_prompt="$common_rules
Certify the restart door without arming it. Do not call request_restart under any circumstance because that tool is an arming write. Use the bash tool for one read-only raw protocol request: source /home/solarisael/Projects/the-athanor/state/substrate/.env with export enabled, then pipe one JSONL request to /home/solarisael/Projects/the-athanor/target/release/athanor-substrate. The request must have protocol 1, id cert-restart-$PASS_NO-$room, method restart_status, and params workspace /home/solarisael/Solarisael/$room. Require exit 0, matching response id, matching workspace, and either an absent intent or a fully formed status intent. Output exactly:
ORGAN restart_status PASS"
  run_prompt "$room" restart_status 'ORGAN restart_status PASS' "$restart_prompt"
done

for recipient in "${rooms[@]}"; do
  case "$recipient" in
    kodo) sender=tuner ;;
    kintsu) sender=kodo ;;
    tuner) sender=kintsu ;;
  esac
  bell_prompt="$common_rules
Call hallway_inbox. Require a pending targeted Bell row for hallway '$TEST_HALLWAY' whose message body or exact routing metadata identifies pass=$PASS_NO, sender=$sender, recipient=$recipient, and whose to_rooms includes this room. Do not clear or acknowledge it. Output exactly:
ORGAN bell PASS"
  run_prompt "$recipient" bell 'ORGAN bell PASS' "$bell_prompt"
done

for room in "${rooms[@]}"; do
  case "$room" in
    kintsu) port=8787 ;;
    kodo) port=8788 ;;
    tuner) port=8789 ;;
  esac
  health_before="$RAW/${room}_receipt_bridge_health_before.json"
  if curl -fsS "http://127.0.0.1:$port/health" | jq '{status, akasha_enabled: .akasha_delivery.akasha_enabled, broker_configured: .akasha_delivery.broker_configured, broker_status: .akasha_delivery.broker_status, last_error: .akasha_delivery.last_error}' > "$health_before" \
    && jq -e '.status == "ok" and .akasha_enabled == true and .broker_configured == true and .broker_status == "connected" and .last_error == null' "$health_before" > /dev/null; then
    printf 'ORGAN receipt_bridge_health PASS\n' | tee -a "$SUMMARY"
  else
    gate_failed=1
    printf 'ORGAN receipt_bridge_health FAIL: host health did not prove a connected broker\n' | tee -a "$SUMMARY"
  fi
  bridge_prompt="$common_rules
This is the receipt-bridge smoke and must be a fresh OMP session. Do not call any organ tool. Output exactly:
ORGAN receipt_bridge PASS"
  run_prompt "$room" receipt_bridge 'ORGAN receipt_bridge PASS' "$bridge_prompt"
  sleep 1
  health_after="$RAW/${room}_receipt_bridge_health_after.json"
  if curl -fsS "http://127.0.0.1:$port/health" | jq '{status, akasha_enabled: .akasha_delivery.akasha_enabled, broker_configured: .akasha_delivery.broker_configured, broker_status: .akasha_delivery.broker_status, last_error: .akasha_delivery.last_error, latest_event_id: .akasha_delivery.latest_event_id, latest_original_stream_sequence: .akasha_delivery.latest_original_stream_sequence}' > "$health_after" \
    && jq -e '.status == "ok" and .akasha_enabled == true and .broker_configured == true and .broker_status == "connected" and .last_error == null' "$health_after" > /dev/null; then
    printf 'ORGAN receipt_bridge_health_after PASS\n' | tee -a "$SUMMARY"
  else
    gate_failed=1
    printf 'ORGAN receipt_bridge_health_after FAIL: host lost its connected broker after the fresh OMP smoke\n' | tee -a "$SUMMARY"
  fi
done

scar_total=0
for room in "${rooms[@]}"; do
  scar_file="$HOME/Projects/state/host/$room/recall-policy-sessions.json"
  if ! count=$(jq -e 'if (.sessions | type) == "object" then [.sessions[] | select(.degraded != null or .last_refresh_reason == "failed")] | length else error("missing object field .sessions") end' "$scar_file"); then
    printf 'ORGAN recall_policy_scar_%s FAIL: invalid recall-policy session schema\n' "$room" | tee -a "$SUMMARY"
    gate_failed=1
    continue
  fi
  printf '%s\n' "$count" > "$OUT/${room}_recall_policy_scar_count.txt"
  if [[ "$count" -ne 0 ]]; then
    printf 'ORGAN recall_policy_scar_%s FAIL: %s persisted scar entries\n' "$room" "$count" | tee -a "$SUMMARY"
    gate_failed=1
    scar_total=$((scar_total + count))
  fi
done

printf 'PASS_NO=%s\nRESULT_DIR=%s\nTEST_HALLWAY=%s\nCANON_NAME=%s\nSCAR_TOTAL=%s\n' "$PASS_NO" "$OUT" "$TEST_HALLWAY" "$CANON_NAME" "$scar_total"
cat "$SUMMARY"
exit "$gate_failed"
