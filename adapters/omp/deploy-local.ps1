param(
  [string]$ProgramRoot = "$env:ProgramFiles\Solarisael\Athanor"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$AdapterRoot = (Resolve-Path $PSScriptRoot).Path
$RepoRoot = (Resolve-Path (Join-Path $AdapterRoot "../..")).Path
$CurrentPath = Join-Path $ProgramRoot "current.json"
if (-not (Test-Path $CurrentPath -PathType Leaf)) {
  throw "Athanor activation pointer is missing: $CurrentPath"
}

$Current = Get-Content $CurrentPath -Raw | ConvertFrom-Json
$Version = [string]$Current.version
if ($Version -notmatch '^[0-9A-Za-z][0-9A-Za-z._-]*$') {
  throw "Athanor activation pointer contains an unsafe version"
}
$VersionRoot = Join-Path $ProgramRoot "versions/$Version"
$ManifestPath = Join-Path $VersionRoot "release-manifest.json"
if (-not (Test-Path $ManifestPath -PathType Leaf)) {
  throw "Active Athanor release manifest is missing: $ManifestPath"
}

Write-Host "==> OMP adapter tests"
$GuardedEnvironment = @(
  "ATHANOR_SUBSTRATE_EXE", "ATHANOR_SUBSTRATE_ROOT", "ATHANOR_STATE_DIR", "ATHANOR_AUTO",
  "SOLARISAEL_GIGA_ENABLED", "SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL", "DATABASE_URL",
  "PGHOST", "PGPORT", "PGDATABASE", "PGUSER", "PGPASSWORD", "SOLARISAEL_HOUSE_RUST",
  "SOLARISAEL_HOUSE_RUST_AUTO", "SOLARISAEL_HOUSE_AUTO", "SOLARISAEL_SUBSTRATE",
  "SOLARISAEL_STATE_DIR", "SOLARISAEL_HOUSE_CORE"
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

$Work = Join-Path ([IO.Path]::GetTempPath()) "athanor-omp-deploy-$PID"
$Stage = Join-Path $Work "stage"
$Backup = Join-Path $Work "backup"
New-Item $Stage -ItemType Directory -Force | Out-Null
New-Item $Backup -ItemType Directory -Force | Out-Null

$TopLevelFiles = @(
  "index.ts", "hygiene.ts", "athanor-root.ts", "discovery.ts", "giga.ts",
  "kitten-lineage.ts", "rust-transport.ts", "package.json", "bunfig.toml",
  "README.md", "LICENSE", "NOTICE"
)
$Sources = @{}
foreach ($Name in $TopLevelFiles) {
  $Sources["adapters/omp/$Name"] = Join-Path $AdapterRoot $Name
}
foreach ($Directory in @("solarisael-house-proof", "starter-room")) {
  Get-ChildItem (Join-Path $AdapterRoot $Directory) -File -Recurse | ForEach-Object {
    $Relative = [IO.Path]::GetRelativePath($AdapterRoot, $_.FullName).Replace("\", "/")
    $Sources["adapters/omp/$Relative"] = $_.FullName
  }
}
$Sources["bin/athanor-omp-loader.ts"] = Join-Path $AdapterRoot "installed-loader.ts"

$Manifest = Get-Content $ManifestPath -Raw | ConvertFrom-Json
$Artifacts = @{}
foreach ($Artifact in $Manifest.artifacts) {
  $Artifacts[[string]$Artifact.path] = $Artifact
}
foreach ($Relative in $Sources.Keys) {
  if (-not $Artifacts.ContainsKey($Relative)) {
    throw "active release manifest has no adapter artifact: $Relative"
  }
  $Source = $Sources[$Relative]
  if (-not (Test-Path $Source -PathType Leaf)) {
    throw "adapter source is missing: $Source"
  }
  $Staged = Join-Path $Stage $Relative
  New-Item (Split-Path $Staged -Parent) -ItemType Directory -Force | Out-Null
  Copy-Item $Source $Staged -Force
  $Artifacts[$Relative].sha256 = (Get-FileHash $Staged -Algorithm SHA256).Hash.ToLowerInvariant()
  $Artifacts[$Relative].size = (Get-Item $Staged).Length
}

$ManifestStage = Join-Path $Stage "release-manifest.json"
$Manifest | ConvertTo-Json -Depth 8 | Set-Content $ManifestStage -Encoding utf8NoBOM

Write-Host "==> activate OMP adapter without rebuilding Rust/Godot/PostgreSQL"
$Activated = @()
try {
  foreach ($Relative in $Sources.Keys) {
    $Target = Join-Path $VersionRoot $Relative
    $Saved = Join-Path $Backup $Relative
    New-Item (Split-Path $Target -Parent) -ItemType Directory -Force | Out-Null
    New-Item (Split-Path $Saved -Parent) -ItemType Directory -Force | Out-Null
    if (Test-Path $Target -PathType Leaf) { Copy-Item $Target $Saved -Force }
    $Temporary = "$Target.new-$PID"
    Copy-Item (Join-Path $Stage $Relative) $Temporary -Force
    Move-Item $Temporary $Target -Force
    $Activated += $Relative
  }
  Copy-Item $ManifestPath (Join-Path $Backup "release-manifest.json") -Force
  $ManifestTemporary = "$ManifestPath.new-$PID"
  Copy-Item $ManifestStage $ManifestTemporary -Force
  Move-Item $ManifestTemporary $ManifestPath -Force
} catch {
  foreach ($Relative in $Activated) {
    $Saved = Join-Path $Backup $Relative
    $Target = Join-Path $VersionRoot $Relative
    if (Test-Path $Saved -PathType Leaf) { Copy-Item $Saved $Target -Force }
  }
  $SavedManifest = Join-Path $Backup "release-manifest.json"
  if (Test-Path $SavedManifest -PathType Leaf) { Copy-Item $SavedManifest $ManifestPath -Force }
  throw
} finally {
  Remove-Item $Work -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "==> OMP adapter deployed"
Write-Host "version: $Version"
Write-Host "updated artifacts: $($Sources.Count)"
Write-Host "restart OMP once to load the new TypeScript tool schema"
