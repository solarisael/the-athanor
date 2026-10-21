$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot "prepared-release.ps1")

function Assert-PreparedCondition([bool]$Condition, [string]$Message) {
  if (-not $Condition) { throw $Message }
}

function Assert-PreparedRefusal([scriptblock]$Action, [string]$Message) {
  $Refused = $false
  try { & $Action | Out-Null } catch { $Refused = $true }
  Assert-PreparedCondition $Refused $Message
}

function Write-FixtureManifest([string]$Payload, [object]$Manifest) {
  $Manifest | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $Payload "release-manifest.json") -Encoding utf8NoBOM
}

function New-PreparedFixture([string]$Directory) {
  $Payload = Join-Path $Directory "payload"
  $Files = @{
    "bin/athanor.exe" = "prepared-manager"
    "bin/athanor-substrate.exe" = "prepared-substrate"
    "bin/athanor-omp-loader.ts" = "prepared-loader"
    "components/omp-adapter/index.ts" = "prepared-adapter"
    "runtime/postgresql/pgAdmin 4/docs/index.html" = "bundled documentation"
  }
  foreach ($Entry in $Files.GetEnumerator()) {
    $File = Join-Path $Payload $Entry.Key
    New-Item ([IO.Path]::GetDirectoryName($File)) -ItemType Directory -Force | Out-Null
    [IO.File]::WriteAllText($File, $Entry.Value)
  }
  $AdapterFile = Join-Path $Payload "components/omp-adapter/index.ts"
  $Compatibility = @{ hostApi = 2; substrateApi = 1; deliveryApi = 2; schemaVersion = 38 }
  $Component = @{
    format = 1; component = "omp-adapter"; version = "1.0.0"
    releaseId = "1.0.0-$('a' * 64)"; compatibility = $Compatibility
    artifacts = @(@{ path = "index.ts"; size = (Get-Item $AdapterFile).Length; sha256 = (Get-FileHash $AdapterFile).Hash.ToLowerInvariant() })
  }
  $Component | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $Payload "components/omp-adapter/component-manifest.json") -Encoding utf8NoBOM
  $Artifacts = @(Get-ChildItem $Payload -File -Recurse | Sort-Object FullName | ForEach-Object {
    @{
      path = [IO.Path]::GetRelativePath($Payload, $_.FullName).Replace("\", "/")
      size = $_.Length; sha256 = (Get-FileHash $_.FullName).Hash.ToLowerInvariant()
      component = "app"; executable = $_.Extension -eq ".exe"
    }
  })
  $Manifest = @{
    format = 1; product = "the-athanor"; platform = "windows-x64"
    version = "8.7.6+prepared"; schemaVersion = 38; compatibility = $Compatibility
    artifacts = $Artifacts
    rollback = @{ databaseRestoreRequired = $true; minimumRetainedVersions = 2 }
  }
  Write-FixtureManifest $Payload $Manifest
  return $Payload
}

$PreparedSandbox = Join-Path ([IO.Path]::GetTempPath()) "athanor-prepared-test-$PID-$([Guid]::NewGuid().ToString('N'))"
try {
  $Payload = New-PreparedFixture (Join-Path $PreparedSandbox "original")
  $Receipt = Write-PreparedRelease -Payload $Payload -TestsPassed $true
  $Accepted = Read-PreparedRelease -Path $Receipt
  Assert-PreparedCondition ($Accepted.Release -ceq "8.7.6+prepared") "receipt loading must not depend on the checkout product version"
  Assert-PreparedCondition ($Accepted.AdapterSource -eq (Join-Path $Payload "components/omp-adapter")) "the prepared adapter must come from the verified payload"
  $OriginalReceipt = [IO.File]::ReadAllText($Receipt)
  Assert-PreparedRefusal { Write-PreparedRelease -Payload $Payload -TestsPassed $false } "a second preparation must not replace an existing receipt"
  Assert-PreparedCondition ([IO.File]::ReadAllText($Receipt) -ceq $OriginalReceipt) "failed publication must preserve the accepted receipt"

  $Moved = Join-Path $PreparedSandbox "moved"
  Move-Item -LiteralPath (Split-Path $Payload -Parent) -Destination $Moved
  $Payload = Join-Path $Moved "payload"
  $Receipt = Join-Path $Moved "prepared-release.json"
  $Accepted = Read-PreparedRelease -Path $Receipt
  Assert-PreparedCondition ($Accepted.StagedManager -eq (Join-Path $Payload "bin/athanor.exe")) "a moved preparation must resolve its own bytes, not its former location"

  $Adapter = Join-Path $Payload "components/omp-adapter/index.ts"
  $OriginalAdapter = [IO.File]::ReadAllText($Adapter)
  [IO.File]::WriteAllText($Adapter, ("x" * $OriginalAdapter.Length))
  Assert-PreparedRefusal { Read-PreparedRelease -Path $Receipt } "same-size adapter corruption must fail its digest check"
  [IO.File]::WriteAllText($Adapter, $OriginalAdapter)
  $Manager = Join-Path $Payload "bin/athanor.exe"
  Remove-Item -LiteralPath $Manager
  Assert-PreparedRefusal { Read-PreparedRelease -Path $Receipt } "a missing manager must fail before execution"
  [IO.File]::WriteAllText($Manager, "prepared-manager")

  $ManifestPath = Join-Path $Payload "release-manifest.json"
  $OriginalManifest = [IO.File]::ReadAllText($ManifestPath)
  [IO.File]::AppendAllText($ManifestPath, " ")
  Assert-PreparedRefusal { Read-PreparedRelease -Path $Receipt } "changed manifest bytes must fail the preparation binding"
  [IO.File]::WriteAllText($ManifestPath, $OriginalManifest)

  foreach ($Mutation in @(
      @{ field = "format"; value = 2 },
      @{ field = "testsPassed"; value = "true" },
      @{ field = "version"; value = "8.7.5+prepared" },
      @{ field = "manifestSha256"; value = ("0" * 64) })) {
    $Changed = $OriginalReceipt | ConvertFrom-Json -AsHashtable
    $Changed[$Mutation.field] = $Mutation.value
    $Changed | ConvertTo-Json | Set-Content -LiteralPath $Receipt -Encoding utf8NoBOM
    Assert-PreparedRefusal { Read-PreparedRelease -Path $Receipt } "invalid receipt field $($Mutation.field) must refuse"
  }
  [IO.File]::WriteAllText($Receipt, $OriginalReceipt)

  foreach ($BadPath in @("../outside.exe", "bin/../../outside.exe", "bin/athanor.exe:alternate", "C:/outside.exe", "bin\athanor.exe")) {
    $CasePayload = New-PreparedFixture (Join-Path $PreparedSandbox ([Guid]::NewGuid().ToString('N')))
    $Manifest = Get-Content (Join-Path $CasePayload "release-manifest.json") -Raw | ConvertFrom-Json
    $Manifest.artifacts[0].path = $BadPath
    Write-FixtureManifest $CasePayload $Manifest
    Assert-PreparedRefusal { Write-PreparedRelease -Payload $CasePayload -TestsPassed $true } "unsafe manifest path '$BadPath' must refuse"
    Assert-PreparedCondition (-not (Test-Path (Join-Path (Split-Path $CasePayload -Parent) "prepared-release.json"))) "failed preparation must not publish a receipt"
  }

  foreach ($Mutation in @("duplicate", "case-collision", "missing-loader", "size-type", "digest-shape")) {
    $CasePayload = New-PreparedFixture (Join-Path $PreparedSandbox $Mutation)
    $Manifest = Get-Content (Join-Path $CasePayload "release-manifest.json") -Raw | ConvertFrom-Json
    switch ($Mutation) {
      "duplicate" { $Manifest.artifacts += $Manifest.artifacts[0] }
      "case-collision" {
        $Duplicate = $Manifest.artifacts[0] | ConvertTo-Json | ConvertFrom-Json
        $Duplicate.path = $Duplicate.path.ToUpperInvariant()
        $Manifest.artifacts += $Duplicate
      }
      "missing-loader" { $Manifest.artifacts = @($Manifest.artifacts | Where-Object path -ne "bin/athanor-omp-loader.ts") }
      "size-type" { $Manifest.artifacts[0].size = "16" }
      "digest-shape" { $Manifest.artifacts[0].sha256 = "invalid" }
    }
    Write-FixtureManifest $CasePayload $Manifest
    Assert-PreparedRefusal { Write-PreparedRelease -Payload $CasePayload -TestsPassed $true } "invalid artifact contract '$Mutation' must refuse"
  }

  if ($IsWindows) {
    $LinkRoot = Join-Path $PreparedSandbox "linked"
    New-Item $LinkRoot -ItemType Directory | Out-Null
    New-Item -ItemType Junction -Path (Join-Path $LinkRoot "payload") -Target $Payload | Out-Null
    try {
      Assert-PreparedRefusal { Write-PreparedRelease -Payload (Join-Path $LinkRoot "payload") -TestsPassed $true } "a junction must not redirect preparation bytes"
    } finally {
      Remove-Item -LiteralPath (Join-Path $LinkRoot "payload") -Force
    }
    $Repository = Join-Path $PreparedSandbox "failed-preparation"
    New-Item (Join-Path $Repository "substrate") -ItemType Directory -Force | Out-Null
    New-Item (Join-Path $Repository "installer") -ItemType Directory -Force | Out-Null
    Copy-Item (Join-Path $PSScriptRoot "../substrate/deploy-local.ps1") (Join-Path $Repository "substrate")
    foreach ($File in @("prepared-release.ps1", "native-release-contract.ps1")) {
      Copy-Item (Join-Path $PSScriptRoot $File) (Join-Path $Repository "installer")
    }
    Set-Content (Join-Path $Repository "package.json") '{"version":"8.7.6"}'
    $OtherBuild = Join-Path $Repository "target/releases/8.7.6+other"
    New-Item $OtherBuild -ItemType Directory -Force | Out-Null
    Set-Content (Join-Path $OtherBuild "in-progress") "owned by another preparation"
    $SavedPath = $env:PATH
    $SavedJobs = [Environment]::GetEnvironmentVariable("CARGO_BUILD_JOBS", "Process")

    try {
      $env:PATH = ""
      foreach ($Jobs in @($null, "", "3")) {
        if ($null -eq $Jobs) {
          Remove-Item Env:CARGO_BUILD_JOBS -ErrorAction SilentlyContinue
        } else {
          $env:CARGO_BUILD_JOBS = $Jobs
        }
        $Refusal = ""

        try {
          & (Join-Path $Repository "substrate/deploy-local.ps1") -PrepareOnly -Build "failure"
        } catch {
          $Refusal = $_.Exception.Message
        }
        Assert-PreparedCondition ($Refusal -like "*cargo*") "unavailable cargo must remain the reported preparation failure"
        Assert-PreparedCondition (-not (Test-Path (Join-Path $Repository "target/releases/8.7.6+failure"))) "failed preparation must remove its own incomplete output"
        Assert-PreparedCondition (Test-Path (Join-Path $OtherBuild "in-progress")) "failed preparation must preserve another build"
        Assert-PreparedCondition ([Environment]::GetEnvironmentVariable("CARGO_BUILD_JOBS", "Process") -ceq $Jobs) "preparation must restore the exact caller build-job environment"
      }
    } finally {
      $env:PATH = $SavedPath
      if ($null -eq $SavedJobs) {
        Remove-Item Env:CARGO_BUILD_JOBS -ErrorAction SilentlyContinue
      } else {
        $env:CARGO_BUILD_JOBS = $SavedJobs
      }
    }
  }
  Write-Host "prepared-release boundary tests passed"
} finally {
  if (Test-Path $PreparedSandbox) { Remove-Item -LiteralPath $PreparedSandbox -Recurse -Force }
}
