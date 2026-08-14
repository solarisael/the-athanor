param(
  [Parameter(Mandatory = $true)][string]$Version,
  [string]$OutDir = "dist"
)
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$Out = if ([IO.Path]::IsPathRooted($OutDir)) { [IO.Path]::GetFullPath($OutDir) } else { [IO.Path]::GetFullPath((Join-Path $Root $OutDir)) }
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
New-Item $Downloads -ItemType Directory -Force | Out-Null
$PostgresZip = Join-Path $Downloads "postgresql.zip"
$PgvectorZip = Join-Path $Downloads "pgvector.zip"
$NatsZip = Join-Path $Downloads "nats.zip"
$GodotZip = Join-Path $Downloads "godot.zip"
Fetch-Verified $Dependencies.managed.postgresql.url $Dependencies.managed.postgresql.sha256 $PostgresZip
Fetch-Verified $Dependencies.managed.pgvector.sourceUrl $Dependencies.managed.pgvector.sha256 $PgvectorZip
Fetch-Verified $Dependencies.managed.natsServer.url $Dependencies.managed.natsServer.sha256 $NatsZip
Fetch-Verified $Dependencies.managed.godot.url $Dependencies.managed.godot.sha256 $GodotZip

$PostgresExtract = Join-Path $Work "postgresql"
$PgvectorExtract = Join-Path $Work "pgvector"
$NatsExtract = Join-Path $Work "nats"
$GodotExtract = Join-Path $Work "godot"
Expand-Archive $PostgresZip $PostgresExtract
Expand-Archive $PgvectorZip $PgvectorExtract
Expand-Archive $NatsZip $NatsExtract
Expand-Archive $GodotZip $GodotExtract
$PgRoot = (Get-ChildItem $PostgresExtract -Directory | Select-Object -First 1).FullName
$PgvectorRoot = (Get-ChildItem $PgvectorExtract -Directory | Select-Object -First 1).FullName
$env:PGROOT = $PgRoot
Push-Location $PgvectorRoot
try {
  & nmake /NOLOGO /F Makefile.win
  if ($LASTEXITCODE -ne 0) { throw "pgvector build failed" }
  & nmake /NOLOGO /F Makefile.win install
  if ($LASTEXITCODE -ne 0) { throw "pgvector install failed" }
} finally { Pop-Location }

$CargoTarget = Join-Path $Work "cargo-target"
$PreviousCargoTarget = $env:CARGO_TARGET_DIR
$env:CARGO_TARGET_DIR = $CargoTarget
Push-Location $Root
try {
  cargo build --release --locked -p athanor-install -p athanor-substrate -p house-host -p athanor-house-delivery -p athanor-godot
  if ($LASTEXITCODE -ne 0) { throw "Rust release build failed" }
} finally {
  Pop-Location
  $env:CARGO_TARGET_DIR = $PreviousCargoTarget
}

$ReleaseBin = Join-Path $CargoTarget "release"
$Bin = Join-Path $Stage "bin"
$Runtime = Join-Path $Stage "runtime"
$GodotProject = Join-Path $Runtime "godot"
New-Item $Bin -ItemType Directory -Force | Out-Null
New-Item (Join-Path $Runtime "postgresql") -ItemType Directory -Force | Out-Null
New-Item (Join-Path $Runtime "nats") -ItemType Directory -Force | Out-Null
New-Item (Join-Path $GodotProject "target/debug") -ItemType Directory -Force | Out-Null
New-Item (Join-Path $GodotProject "target/release") -ItemType Directory -Force | Out-Null
Copy-Item (Join-Path $ReleaseBin "athanor-manage.exe") $Bin
Copy-Item (Join-Path $ReleaseBin "athanor-substrate.exe") $Bin
Copy-Item (Join-Path $ReleaseBin "house-host.exe") $Bin
Copy-Item (Join-Path $ReleaseBin "athanor-house-delivery.exe") $Bin
Copy-Item (Join-Path $Root "adapters/omp/installed-loader.ts") (Join-Path $Bin "athanor-omp-loader.ts")
$GodotExe = Get-ChildItem $GodotExtract -Filter "Godot_v4.7.1-stable_win64.exe" -Recurse | Select-Object -First 1
if (-not $GodotExe) { throw "Godot archive did not contain the pinned Windows x64 executable" }
Copy-Item $GodotExe.FullName (Join-Path $Bin "athanor-gui.exe")
$GodotSource = Join-Path $Root "gui"
@("project.godot", "main.tscn", "athanor.gdextension", "icon.svg") | ForEach-Object {
  Copy-Item (Join-Path $GodotSource $_) $GodotProject
}
Copy-Item (Join-Path $GodotSource "assets") (Join-Path $GodotProject "assets") -Recurse
Copy-Item (Join-Path $GodotSource "theme") (Join-Path $GodotProject "theme") -Recurse
Copy-Item (Join-Path $ReleaseBin "athanor_godot.dll") (Join-Path $GodotProject "target/debug/athanor_godot.dll")
Copy-Item (Join-Path $ReleaseBin "athanor_godot.dll") (Join-Path $GodotProject "target/release/athanor_godot.dll")
$GodotImportOutput = (& $GodotExe.FullName --headless --editor --path $GodotProject --quit 2>&1 | Out-String)
Write-Host $GodotImportOutput
if ($GodotImportOutput -notmatch "Initialize godot-rust" -or $GodotImportOutput -match "(?m)^ERROR:") {
  throw "Godot runtime project import failed"
}
Copy-Item (Join-Path $PgRoot "*") (Join-Path $Runtime "postgresql") -Recurse
$NatsExe = Get-ChildItem $NatsExtract -Filter "nats-server.exe" -Recurse | Select-Object -First 1
if (-not $NatsExe) { throw "NATS archive did not contain nats-server.exe" }
Copy-Item $NatsExe.FullName (Join-Path $Runtime "nats/nats-server.exe")
$AdapterSource = Join-Path $Root "adapters/omp"
$AdapterTarget = Join-Path $Stage "adapters/omp"
New-Item $AdapterTarget -ItemType Directory -Force | Out-Null
@(
  "index.ts", "hygiene.ts", "athanor-root.ts", "discovery.ts", "giga.ts",
  "kitten-lineage.ts", "rust-transport.ts", "package.json", "bunfig.toml",
  "README.md", "LICENSE", "NOTICE"
) | ForEach-Object {
  Copy-Item (Join-Path $AdapterSource $_) $AdapterTarget
}
Copy-Item (Join-Path $AdapterSource "solarisael-house-proof") (Join-Path $AdapterTarget "solarisael-house-proof") -Recurse
Copy-Item (Join-Path $AdapterSource "starter-room") (Join-Path $AdapterTarget "starter-room") -Recurse
Copy-Item (Join-Path $Root "installer/dependencies.json") (Join-Path $Stage "compatibility.json")

$Artifacts = @()
Get-ChildItem $Stage -File -Recurse | Sort-Object FullName | ForEach-Object {
  $Relative = [IO.Path]::GetRelativePath($Stage, $_.FullName).Replace("\", "/")
  $Component = if ($Relative -eq "bin/athanor-manage.exe") { "installer" }
    elseif ($Relative -like "runtime/postgresql/*") { "postgresql-pgvector" }
    elseif ($Relative -like "runtime/nats/*") { "nats-server" }
    elseif ($Relative -like "bin/house-host.exe") { "host" }
    elseif ($Relative -like "bin/athanor-house-delivery.exe") { "delivery" }
    elseif ($Relative -like "bin/athanor-substrate.exe") { "substrate" }
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
  format = 1; product = "the-athanor"; version = $Version; platform = "windows-x64"; schemaVersion = 16
  compatibility = [ordered]@{ hostApi = 1; substrateApi = 1; deliveryApi = 1; godotApi = "4.7"; godot = "4.7.1-stable"; postgresql = "18.4-2"; pgvector = "0.8.6"; natsServer = "2.14.4" }
  artifacts = $Artifacts
  rollback = [ordered]@{ databaseRestoreRequired = $true; minimumRetainedVersions = 2 }
}
$Manifest | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $Stage "release-manifest.json") -Encoding utf8NoBOM
New-Item $Out -ItemType Directory -Force | Out-Null
Remove-Item (Join-Path $Out "payload") -Recurse -Force -ErrorAction SilentlyContinue
Copy-Item $Stage (Join-Path $Out "payload") -Recurse -Force
