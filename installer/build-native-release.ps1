param(
  [string]$Version = "",
  [string]$OutDir = "dist"
)
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot "native-build-cache.ps1")
. (Join-Path $PSScriptRoot "native-release-contract.ps1")
. (Join-Path $PSScriptRoot "omp-adapter-component.ps1")
$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$Version = Get-NativeReleaseVersion -RepositoryRoot $Root -Requested $Version
# One compatibility authority for the product manifest and the OMP adapter
# component, read before any work so a broken authority refuses early.
$Compatibility = Get-OmpAdapterComponentCompatibility -RepositoryRoot $Root
$Out = if ([IO.Path]::IsPathRooted($OutDir)) { [IO.Path]::GetFullPath($OutDir) } else { [IO.Path]::GetFullPath((Join-Path $Root $OutDir)) }
New-Item $Out -ItemType Directory -Force | Out-Null

# Nothing above this point downloads, expands a cache, or creates work
# directories: a missing toolchain must refuse before any of that happens.
$Toolchain = Invoke-NativeReleaseStage -Name "toolchain-preflight" -OutDir $Out -Action {
  Assert-NativeReleaseToolchain -RepositoryRoot $Root
}

$TempRoot = if ([string]::IsNullOrWhiteSpace($env:RUNNER_TEMP)) { [IO.Path]::GetTempPath() } else { $env:RUNNER_TEMP }
$Work = Join-Path $TempRoot "athanor-native-$Version"
if (Test-Path $Work) { Remove-Item $Work -Recurse -Force }
$Stage = Join-Path $Work "payload"
New-Item $Stage -ItemType Directory -Force | Out-Null

$Dependencies = Get-Content (Join-Path $PSScriptRoot "dependencies.json") -Raw | ConvertFrom-Json
function Fetch-Verified([string]$Url, [string]$Sha256, [string]$Destination) {
  if (Test-Path $Destination) {
    $Cached = (Get-FileHash $Destination -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($Cached -eq $Sha256.ToLowerInvariant()) { return }
    Remove-Item $Destination -Force
  }
  Invoke-WebRequest -Uri $Url -OutFile $Destination
  $Actual = (Get-FileHash $Destination -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($Actual -ne $Sha256.ToLowerInvariant()) { throw "checksum mismatch for $Url; expected $Sha256, got $Actual" }
}

$Downloads = Join-Path $TempRoot "athanor-native-downloads"
$PostgresZip = Join-Path $Downloads "postgresql.zip"
$PgvectorZip = Join-Path $Downloads "pgvector.zip"
$NatsZip = Join-Path $Downloads "nats.zip"
$GodotZip = Join-Path $Downloads "godot.zip"
Invoke-NativeReleaseStage -Name "download-verification" -OutDir $Out -Action {
  New-Item $Downloads -ItemType Directory -Force | Out-Null
  Fetch-Verified $Dependencies.managed.postgresql.url $Dependencies.managed.postgresql.sha256 $PostgresZip
  Fetch-Verified $Dependencies.managed.pgvector.sourceUrl $Dependencies.managed.pgvector.sha256 $PgvectorZip
  Fetch-Verified $Dependencies.managed.natsServer.url $Dependencies.managed.natsServer.sha256 $NatsZip
  Fetch-Verified $Dependencies.managed.godot.url $Dependencies.managed.godot.sha256 $GodotZip
}

$DependencyCacheKey = Get-NativeBuildCacheKey (@(
  "native-dependencies-v2",
  [string]$Dependencies.managed.postgresql.sha256,
  [string]$Dependencies.managed.pgvector.sha256,
  [string]$Dependencies.managed.natsServer.sha256,
  [string]$Dependencies.managed.godot.sha256
) + $Toolchain.cacheKeyMaterial)
$DependencyCacheRoot = Join-Path $Root "target/native-release-cache"
$DependencyCache = Join-Path $DependencyCacheRoot $DependencyCacheKey
$RequiredDependencyFiles = @(
  "postgresql/bin/postgres.exe",
  "postgresql/lib/vector.dll",
  "nats/nats-server.exe",
  "godot/athanor-gui.exe"
)
# One exclusive lock covers this build's entire cache lifetime: selection,
# production, publication, every use of the cached binaries, and pruning.
# Concurrent native release builds serialize here rather than racing, which is
# what lets pruning delete superseded caches without ever pulling one out from
# under another build.
New-Item $DependencyCacheRoot -ItemType Directory -Force | Out-Null
$DependencyCacheLock = Enter-NativeBuildCacheOperationLock $DependencyCacheRoot
try {
Invoke-NativeReleaseStage -Name "dependency-preparation" -OutDir $Out -Action {
  # A cache another build finished earlier is recognized here, never deleted.
  if (-not (Clear-NativeBuildCacheForRebuild $DependencyCacheRoot $DependencyCacheKey $RequiredDependencyFiles $DependencyCacheLock)) {
    $DependencyCacheCandidate = "$DependencyCache.pending-$PID"
    if (Test-Path $DependencyCacheCandidate) { Remove-Item $DependencyCacheCandidate -Recurse -Force }
    try {
      $PostgresExtract = Join-Path $Work "postgresql"
      $PgvectorExtract = Join-Path $Work "pgvector"
      $NatsExtract = Join-Path $Work "nats"
      $GodotExtract = Join-Path $Work "godot"
      Expand-Archive $PostgresZip $PostgresExtract
      Expand-Archive $PgvectorZip $PgvectorExtract
      Expand-Archive $NatsZip $NatsExtract
      Expand-Archive $GodotZip $GodotExtract
      $PreparedPostgres = (Get-ChildItem $PostgresExtract -Directory | Select-Object -First 1).FullName
      $PgvectorRoot = (Get-ChildItem $PgvectorExtract -Directory | Select-Object -First 1).FullName
      $PreviousPgRoot = [Environment]::GetEnvironmentVariable("PGROOT", "Process")
      $env:PGROOT = $PreparedPostgres
      Push-Location $PgvectorRoot
      try {
        & nmake /NOLOGO /F Makefile.win
        if ($LASTEXITCODE -ne 0) { throw "pgvector build failed" }
        & nmake /NOLOGO /F Makefile.win install
        if ($LASTEXITCODE -ne 0) { throw "pgvector install failed" }
      } finally {
        Pop-Location
        if ($null -eq $PreviousPgRoot) { Remove-Item Env:PGROOT -ErrorAction SilentlyContinue }
        else { $env:PGROOT = $PreviousPgRoot }
      }
      $PreparedNats = Get-ChildItem $NatsExtract -Filter "nats-server.exe" -Recurse | Select-Object -First 1
      if (-not $PreparedNats) { throw "NATS archive did not contain nats-server.exe" }
      $PreparedGodot = Get-ChildItem $GodotExtract -Filter "Godot_v4.7.1-stable_win64.exe" -Recurse | Select-Object -First 1
      if (-not $PreparedGodot) { throw "Godot archive did not contain the pinned Windows x64 executable" }

      New-Item (Join-Path $DependencyCacheCandidate "postgresql") -ItemType Directory -Force | Out-Null
      New-Item (Join-Path $DependencyCacheCandidate "nats") -ItemType Directory -Force | Out-Null
      New-Item (Join-Path $DependencyCacheCandidate "godot") -ItemType Directory -Force | Out-Null
      Copy-Item (Join-Path $PreparedPostgres "*") (Join-Path $DependencyCacheCandidate "postgresql") -Recurse
      Copy-Item $PreparedNats.FullName (Join-Path $DependencyCacheCandidate "nats/nats-server.exe")
      Copy-Item $PreparedGodot.FullName (Join-Path $DependencyCacheCandidate "godot/athanor-gui.exe")
      Complete-NativeBuildCache $DependencyCacheCandidate $DependencyCacheKey $RequiredDependencyFiles
      Move-Item $DependencyCacheCandidate $DependencyCache
    } catch {
      Remove-Item $DependencyCacheCandidate -Recurse -Force -ErrorAction SilentlyContinue
      throw
    }
  }
  $script:PgRoot = Join-Path $DependencyCache "postgresql"
  $script:NatsExe = Get-Item (Join-Path $DependencyCache "nats/nats-server.exe")
  $script:GodotExe = Get-Item (Join-Path $DependencyCache "godot/athanor-gui.exe")
  # Only now that the requested cache is verified complete may its stale
  # siblings go: one dependency payload per key would otherwise accumulate
  # forever.
  Remove-StaleNativeBuildCaches $DependencyCacheRoot $DependencyCacheKey $RequiredDependencyFiles $DependencyCacheLock | Out-Null
}

$CargoTarget = Join-Path $Root "target"
Invoke-NativeReleaseStage -Name "cargo-build" -OutDir $Out -Action {
  $PreviousCargoTarget = $env:CARGO_TARGET_DIR
  $PreviousRustc = [Environment]::GetEnvironmentVariable("RUSTC", "Process")
  $env:CARGO_TARGET_DIR = $CargoTarget
  # Build with the exact binaries preflight verified inside the pinned rustup
  # toolchain, and pin the compiler too: neither cargo nor rustc may be taken
  # from PATH here, so a shadow can never produce release bytes.
  $env:RUSTC = $Toolchain.rustcPath
  Push-Location $Root
  try {
    & $Toolchain.cargoPath build --release --locked -p athanor-install -p athanor-substrate -p house-host -p athanor-house-delivery -p athanor-godot -p omp-keeper
    if ($LASTEXITCODE -ne 0) { throw "Rust release build failed" }
  } finally {
    Pop-Location
    $env:CARGO_TARGET_DIR = $PreviousCargoTarget
    if ($null -eq $PreviousRustc) { Remove-Item Env:RUSTC -ErrorAction SilentlyContinue }
    else { $env:RUSTC = $PreviousRustc }
  }
}

$ReleaseBin = Join-Path $CargoTarget "release"
$Bin = Join-Path $Stage "bin"
$Runtime = Join-Path $Stage "runtime"
$GodotProject = Join-Path $Runtime "godot"
Invoke-NativeReleaseStage -Name "payload-materialization" -OutDir $Out -Action {
  New-Item $Bin -ItemType Directory -Force | Out-Null
  New-Item (Join-Path $Runtime "postgresql") -ItemType Directory -Force | Out-Null
  New-Item (Join-Path $Runtime "nats") -ItemType Directory -Force | Out-Null
  New-Item (Join-Path $GodotProject "target/debug") -ItemType Directory -Force | Out-Null
  New-Item (Join-Path $GodotProject "target/release") -ItemType Directory -Force | Out-Null
  Copy-Item (Join-Path $ReleaseBin "athanor.exe") $Bin
  Copy-Item (Join-Path $ReleaseBin "athanor-manage.exe") $Bin
  Copy-Item (Join-Path $ReleaseBin "athanor-substrate.exe") $Bin
  Copy-Item (Join-Path $ReleaseBin "house-host.exe") $Bin
  Copy-Item (Join-Path $ReleaseBin "athanor-house-delivery.exe") $Bin
  Copy-Item (Join-Path $ReleaseBin "omp-keeper.exe") $Bin
  Copy-Item (Join-Path $Root "adapters/omp/installed-loader.ts") (Join-Path $Bin "athanor-omp-loader.ts")
  Copy-Item $GodotExe.FullName (Join-Path $Bin "athanor-gui.exe")
  $GodotSource = Join-Path $Root "gui"
  @("project.godot", "main.tscn", "athanor.gdextension", "icon.svg") | ForEach-Object {
    Copy-Item (Join-Path $GodotSource $_) $GodotProject
  }
  Copy-Item (Join-Path $GodotSource "assets") (Join-Path $GodotProject "assets") -Recurse
  Copy-Item (Join-Path $GodotSource "theme") (Join-Path $GodotProject "theme") -Recurse
  Copy-Item (Join-Path $GodotSource "navigation") (Join-Path $GodotProject "navigation") -Recurse
  Copy-Item (Join-Path $GodotSource "design-system") (Join-Path $GodotProject "design-system") -Recurse
  Copy-Item (Join-Path $GodotSource "screens") (Join-Path $GodotProject "screens") -Recurse
  Copy-Item (Join-Path $ReleaseBin "athanor_godot.dll") (Join-Path $GodotProject "target/debug/athanor_godot.dll")
  Copy-Item (Join-Path $ReleaseBin "athanor_godot.dll") (Join-Path $GodotProject "target/release/athanor_godot.dll")
  Copy-Item (Join-Path $PgRoot "*") (Join-Path $Runtime "postgresql") -Recurse
  $KeeperComponent = Join-Path $Stage "components/omp-keeper"
  New-Item $KeeperComponent -ItemType Directory -Force | Out-Null
  Copy-Item (Join-Path $Root "crates/omp-keeper/README.md") $KeeperComponent
  Copy-Item (Join-Path $Root "crates/omp-keeper/scripts/provision-local.ps1") (Join-Path $KeeperComponent "provision-omp-keeper.ps1")
  Copy-Item (Join-Path $Root "substrate/provision-restart-capability.ps1") $KeeperComponent
  Copy-Item $NatsExe.FullName (Join-Path $Runtime "nats/nats-server.exe")
  # The product ships a fallback OMP adapter component built by the one shared
  # component builder; adapter-only deployment builds the same shape.
  $AdapterComponent = New-OmpAdapterComponentBundle -RepositoryRoot $Root -Destination (Join-Path $Stage "components/omp-adapter") -Compatibility $Compatibility
  Write-Host "omp-adapter fallback component $($AdapterComponent.ReleaseId) ($($AdapterComponent.ArtifactCount) artifacts)"
  Copy-Item (Join-Path $Root "installer/dependencies.json") (Join-Path $Stage "compatibility.json")
}

Invoke-NativeReleaseStage -Name "godot-import" -OutDir $Out -Action {
  $GodotImportOutput = (& $GodotExe.FullName --headless --editor --path $GodotProject --quit 2>&1 | Out-String)
  Write-Host $GodotImportOutput
  if ($GodotImportOutput -notmatch "Initialize godot-rust" -or $GodotImportOutput -match "(?m)^ERROR:") {
    throw "Godot runtime project import failed"
  }
}

Invoke-NativeReleaseStage -Name "manifest-hashing" -OutDir $Out -Action {
  $Artifacts = @()
  Get-ChildItem $Stage -File -Recurse | Sort-Object FullName | ForEach-Object {
    $Relative = [IO.Path]::GetRelativePath($Stage, $_.FullName).Replace("\", "/")
    $Component = if ($Relative -eq "bin/athanor.exe") { "app" }
      elseif ($Relative -eq "bin/athanor-manage.exe") { "installer" }
      elseif ($Relative -like "runtime/postgresql/*") { "postgresql-pgvector" }
      elseif ($Relative -like "runtime/nats/*") { "nats-server" }
      elseif ($Relative -like "bin/house-host.exe") { "host" }
      elseif ($Relative -like "bin/athanor-house-delivery.exe") { "delivery" }
      elseif ($Relative -like "bin/athanor-substrate.exe") { "substrate" }
      elseif ($Relative -like "bin/omp-keeper.exe") { "omp-keeper" }
      elseif ($Relative -like "components/omp-keeper/*") { "omp-keeper" }
      elseif ($Relative -eq "bin/athanor-gui.exe" -or $Relative -like "runtime/godot/*") { "godot-client" }
      else { "omp-adapter" }
    $Artifacts += [ordered]@{
      component = $Component
      path = $Relative
      sha256 = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
      size = $_.Length
      executable = $_.Extension -in ".exe", ".dll"
    }
  }
  $Manifest = [ordered]@{
    format = 1; product = "the-athanor"; version = $Version; platform = "windows-x64"; schemaVersion = $Compatibility.schemaVersion
    compatibility = [ordered]@{ hostApi = $Compatibility.hostApi; substrateApi = $Compatibility.substrateApi; deliveryApi = $Compatibility.deliveryApi; godotApi = "4.7"; godot = "4.7.1-stable"; postgresql = "18.4-2"; pgvector = "0.8.6"; natsServer = "2.14.4" }
    artifacts = $Artifacts
    rollback = [ordered]@{ databaseRestoreRequired = $true; minimumRetainedVersions = 2 }
  }
  $Manifest | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $Stage "release-manifest.json") -Encoding utf8NoBOM
}

Invoke-NativeReleaseStage -Name "output-copy" -OutDir $Out -Action {
  Remove-Item (Join-Path $Out "payload") -Recurse -Force -ErrorAction SilentlyContinue
  Copy-Item $Stage (Join-Path $Out "payload") -Recurse -Force
  Remove-Item $Work -Recurse -Force
}
} finally {
  Exit-NativeBuildCacheOperationLock $DependencyCacheLock
}
