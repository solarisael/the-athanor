# One-shot naming purge: plugin-internal solarisael-* identifiers become
# athanor-* (the House name survives only as installation naming: install
# paths, service name, database, legacy-door literals, docs/history).
# Deliberately excluded: CHANGELOG.md, docs/history/, this script.
$ErrorActionPreference = "Stop"

$replacements = [ordered]@{
  # customTypes and marker tokens
  "solarisael-room-context"          = "athanor-room-context"
  "solarisael-routing-mode"          = "athanor-routing-mode"
  "solarisael-wake-context"          = "athanor-wake-context"
  "solarisael-anamnesis-wake"        = "athanor-anamnesis-wake"
  "solarisael-recall-context"        = "athanor-recall-context"
  "solarisael-presence-context"      = "athanor-presence-context"
  "solarisael-keyword-directive"     = "athanor-keyword-directive"
  "solarisael-hallway-knock"         = "athanor-hallway-knock"
  "solarisael-hallway-bell"          = "athanor-hallway-bell"
  "solarisael-turn-key"              = "athanor-turn-key"
  "solarisael-process-lesson-smoke"  = "athanor-process-lesson-smoke"
  "solarisael-house-craft-starter"   = "athanor-house-craft-starter"
  # component ids and state file names
  "solarisael-house-omp"             = "athanor-omp"
  "solarisael-house-substrate"       = "athanor-substrate"
  "solarisael-house-transcript-debug" = "athanor-transcript-debug"
  "solarisael-house-state.json"      = "athanor-house-state.json"
  ".solarisael-room.json"            = ".athanor-room.json"
  "solarisael-substrate-"            = "athanor-substrate-"
  "solarisael-recall-telemetry-"     = "athanor-recall-telemetry-"
}

# Environment variables: every SOLARISAEL_* reader/setter moves to ATHANOR_*.
# SOLARISAEL_STATE_DIR is dead (guard-list only) and is dropped separately.
$envNames = @(
  "BACKUP_DIR", "BACKUP_KEEP", "DELIVERY_INSTANCE_ID", "DELIVERY_TEST_NATS_URL",
  "DISABLE_AUTO_RECALL", "DISABLE_EMBEDDING", "DISABLE_LESSON_TRIGGERS",
  "EMBED_DIMENSION", "EMBED_MODEL", "EMBED_URL",
  "GIGA_CLAIM_OWNER", "GIGA_ENABLED", "GIGA_PROJECT_KEY",
  "GIGA_SOURCE_LEDGER_DIR", "GIGA_SOURCE_ROOM",
  "HALLWAY_TEMP_DATABASE_URL", "HALLWAY_TEMP_PROOF",
  "HIPPOCAMPUS_ENABLED", "HIPPOCAMPUS_OLLAMA_ENDPOINT", "HIPPOCAMPUS_REMOTE_CONSENT",
  "HOUSE_AUTO", "HOUSE_CORE", "HOUSE_RUST", "HOUSE_RUST_AUTO", "HOUSE_TZ",
  "I_UNDERSTAND_THIS_IS_AN_ISOLATED_POSTGRES_TEST",
  "NATS_URL", "OMP_POSTGRES_TEST", "PG_WSL",
  "RECALL_TELEMETRY", "REPLAY_MODE", "SUBSTRATE_DOTENV_PATH",
  "SUBSTRATE_TEST_DATABASE_URL", "SUBSTRATE_TEST_SCHEMA", "SUBSTRATE",
  "TEST_DISABLE_EMBEDDING", "TEST_SUBSTRATE_HEALTH_SCRIPT", "VAULT_ROOT"
)
foreach ($name in $envNames) {
  $replacements["SOLARISAEL_$name"] = "ATHANOR_$name"
}

$files = git ls-files |
  Where-Object { $_ -notmatch "^docs/history/" } |
  Where-Object { $_ -ne "CHANGELOG.md" -and $_ -ne "scripts/naming-purge.ps1" }

$rewritten = 0
foreach ($file in $files) {
  if (-not (Test-Path $file -PathType Leaf)) { continue }
  $bytes = [IO.File]::ReadAllBytes($file)
  if ($bytes -contains 0) { continue } # binary
  $text = [Text.Encoding]::UTF8.GetString($bytes)
  $result = $text
  foreach ($pair in $replacements.GetEnumerator()) {
    $result = $result.Replace($pair.Key, $pair.Value)
  }
  if ($result -ne $text) {
    [IO.File]::WriteAllText($file, $result, [Text.UTF8Encoding]::new($false))
    $rewritten++
  }
}
Write-Host "rewrote $rewritten files"
