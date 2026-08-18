$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot "native-build-cache.ps1")

function Assert-True([bool]$Condition, [string]$Message) {
  if (-not $Condition) { throw $Message }
}

$Root = Join-Path ([IO.Path]::GetTempPath()) "athanor-native-cache-test-$PID-$([Guid]::NewGuid().ToString('N'))"
$Required = @("postgresql/bin/postgres.exe", "postgresql/lib/vector.dll", "godot/athanor-gui.exe")
$Key = Get-NativeBuildCacheKey @("cache-v1", "postgres-sha", "pgvector-sha", "godot-sha")
try {
  New-Item $Root -ItemType Directory -Force | Out-Null
  Assert-True (-not (Test-NativeBuildCache $Root $Key $Required)) "an unmarked cache must be rejected"

  foreach ($Relative in $Required) {
    $Target = Join-Path $Root $Relative
    New-Item ([IO.Path]::GetDirectoryName($Target)) -ItemType Directory -Force | Out-Null
    Set-Content $Target "fixture" -Encoding ascii
  }
  Assert-True (-not (Test-NativeBuildCache $Root $Key $Required)) "files without a completion marker must be rejected"

  Complete-NativeBuildCache $Root $Key $Required
  Assert-True (Test-NativeBuildCache $Root $Key $Required) "a complete cache with the matching key must be accepted"
  Assert-True (-not (Test-NativeBuildCache $Root "wrong-key" $Required)) "a cache built from different inputs must be rejected"

  Remove-Item (Join-Path $Root $Required[1]) -Force
  Assert-True (-not (Test-NativeBuildCache $Root $Key $Required)) "a cache missing a required artifact must be rejected"

  $Escaped = $false
  try { Test-NativeBuildCache $Root $Key @("../outside") | Out-Null } catch { $Escaped = $true }
  Assert-True $Escaped "required cache paths must not escape the cache root"

  Write-Host "native build cache contract passed"
} finally {
  Remove-Item $Root -Recurse -Force -ErrorAction SilentlyContinue
}
