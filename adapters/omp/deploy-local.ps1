param(
  [string]$ProgramRoot = "$env:ProgramFiles\Solarisael\Athanor",
  [switch]$TestsOnly
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# Adapter-only deployment: run the adapter tests, build one component bundle in
# a temporary directory, and hand it to the installed Athanor manager. This
# script never builds a native product, never edits a product payload or release
# manifest, and never writes anything under the program root itself: every
# installed write belongs to the manager.

$AdapterRoot = (Resolve-Path $PSScriptRoot).Path
$RepoRoot = (Resolve-Path (Join-Path $AdapterRoot "../..")).Path
. (Join-Path $RepoRoot "installer/omp-adapter-component.ps1")

$ExpectedProgramRoot = Join-Path $env:ProgramFiles "Solarisael/Athanor"
if (-not [IO.Path]::GetFullPath($ProgramRoot).Equals(
    [IO.Path]::GetFullPath($ExpectedProgramRoot),
    [StringComparison]::OrdinalIgnoreCase
  )) {
  throw "ProgramRoot must match the installed manager target derived from ProgramFiles: $ExpectedProgramRoot"
}

$Manager = Join-Path $ProgramRoot "bin/athanor-manage.exe"
if (-not (Test-Path -LiteralPath $Manager -PathType Leaf)) {
  throw "The installed Athanor manager is missing: $Manager"
}

Write-Host "==> OMP adapter tests"
$GuardedEnvironment = @(
  "ATHANOR_SUBSTRATE_EXE", "ATHANOR_SUBSTRATE_ROOT", "ATHANOR_STATE_DIR", "ATHANOR_AUTO",
  "ATHANOR_GIGA_ENABLED", "ATHANOR_HIPPOCAMPUS_ENABLED", "ATHANOR_HIPPOCAMPUS_OLLAMA_ENDPOINT",
  "ATHANOR_HOST_URL", "ATHANOR_HOST_HOUSE_ID", "ATHANOR_HOST_TOKEN", "ATHANOR_VAULT_ROOT",
  "ATHANOR_PG_WSL", "PG_BIN_DIR", "ATHANOR_SUBSTRATE_TEST_DATABASE_URL",
  "DATABASE_URL", "PGHOST", "PGPORT", "PGDATABASE", "PGUSER", "PGPASSWORD"
)
# Every pre-cutover SOLARISAEL_* export is scrubbed by prefix: the test fence
# refuses each one by name, and a deploy shell may carry any of them.
$GuardedEnvironment += @(
  [Environment]::GetEnvironmentVariables("Process").Keys |
    Where-Object { $_ -like "SOLARISAEL_*" }
)
$SavedEnvironment = @{}
foreach ($Name in $GuardedEnvironment) {
  $SavedEnvironment[$Name] = [Environment]::GetEnvironmentVariable($Name, "Process")
  [Environment]::SetEnvironmentVariable($Name, $null, "Process")
}
try {
  Push-Location $AdapterRoot
  try {
    & bun test --isolate --max-concurrency 1
    if ($LASTEXITCODE -ne 0) { throw "OMP adapter tests failed" }
  } finally {
    Pop-Location
  }
} finally {
  foreach ($Name in $GuardedEnvironment) {
    [Environment]::SetEnvironmentVariable($Name, $SavedEnvironment[$Name], "Process")
  }
}
if ($TestsOnly) {
  Write-Host "==> OMP adapter tests passed"
  return
}

$Work = Join-Path ([IO.Path]::GetTempPath()) "athanor-omp-component-$PID-$([Guid]::NewGuid().ToString('N'))"
try {
  Write-Host "==> build OMP adapter component bundle"
  $Component = New-OmpAdapterComponentBundle -RepositoryRoot $RepoRoot -Destination (Join-Path $Work "omp-adapter")
  $Source = $Component.Root
  Write-Host "version: $($Component.Version)"
  Write-Host "releaseId: $($Component.ReleaseId)"
  Write-Host "artifacts: $($Component.ArtifactCount)"

  Write-Host "==> install the OMP adapter release through the Athanor manager"
  & $Manager install-omp-adapter --source $Source
  if ($LASTEXITCODE -ne 0) {
    throw "install-omp-adapter refused the component bundle (exit code $LASTEXITCODE)"
  }
} finally {
  # The stage carries only copies, so cleanup is unconditional; a scanner may
  # hold a file open for a moment, so retry once before reporting the leftover.
  foreach ($Attempt in 1, 2) {
    if (-not (Test-Path -LiteralPath $Work)) { break }
    Remove-Item -LiteralPath $Work -Recurse -Force -ErrorAction SilentlyContinue
    if (Test-Path -LiteralPath $Work) { Start-Sleep -Milliseconds 200 }
  }
  if (Test-Path -LiteralPath $Work) {
    Write-Warning "temporary OMP adapter component stage could not be removed: $Work"
  }
}

Write-Host "==> OMP adapter deployed"
Write-Host "restart OMP once to load the new TypeScript tool schema"
