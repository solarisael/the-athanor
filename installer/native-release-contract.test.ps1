$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot "native-release-contract.ps1")

function Assert-True([bool]$Condition, [string]$Message) {
  if (-not $Condition) { throw $Message }
}

function Assert-Refused([scriptblock]$Action, [string]$Message) {
  $Refused = $false
  try { & $Action | Out-Null } catch { $Refused = $true }
  Assert-True $Refused $Message
}

function Get-Refusal([scriptblock]$Action) {
  try { & $Action | Out-Null } catch { return [string]$_.Exception.Message }
  return ""
}

function Read-Timestamp([object]$Value) {
  if ($Value -is [DateTime]) { return ([DateTime]$Value).ToUniversalTime() }
  return [DateTime]::Parse([string]$Value, [Globalization.CultureInfo]::InvariantCulture, [Globalization.DateTimeStyles]::RoundtripKind)
}

function Join-FixturePath([string]$Base, [string]$Relative) {
  # Fixture paths are compared against refusal messages, which carry fully
  # normalised paths, so every fixture path is normalised at birth.
  return [IO.Path]::GetFullPath((Join-Path $Base $Relative))
}

function New-CommandFixture([string]$Path, [string[]]$Lines) {
  New-Item ([IO.Path]::GetDirectoryName($Path)) -ItemType Directory -Force | Out-Null
  Set-Content -LiteralPath $Path -Value ((@("@echo off") + $Lines) -join "`r`n") -Encoding ascii
}

function Test-LinkKindSupported([string]$Root, [string]$Kind) {
  # Junctions work on any NTFS volume; file symlinks need Developer Mode or an
  # elevated session. A boundary the host cannot express is reported, not faked.
  $Probe = Join-FixturePath $Root "link-capability-$Kind"
  $TargetDirectory = Join-FixturePath $Probe "target"
  New-Item $TargetDirectory -ItemType Directory -Force | Out-Null
  $TargetFile = Join-FixturePath $TargetDirectory "target.txt"
  Set-Content -LiteralPath $TargetFile "target" -Encoding ascii
  $Target = if ($Kind -eq "Junction") { $TargetDirectory } else { $TargetFile }
  try {
    New-Item -ItemType $Kind -Path (Join-FixturePath $Probe "probe") -Target $Target -ErrorAction Stop | Out-Null
    return $true
  } catch {
    return $false
  }
}

$Sandbox = Join-FixturePath ([IO.Path]::GetTempPath()) "athanor-native-contract-test-$PID-$([Guid]::NewGuid().ToString('N'))"
$OriginalToolchainEnvironment = @{}
foreach ($Name in @("VCToolsVersion", "VCToolsInstallDir", "WindowsSDKVersion", "VSCMD_ARG_TGT_ARCH", "CARGO_HOME", "RUSTUP_HOME")) {
  $OriginalToolchainEnvironment[$Name] = [Environment]::GetEnvironmentVariable($Name, "Process")
}
$OriginalPath = $env:PATH
try {
  New-Item $Sandbox -ItemType Directory -Force | Out-Null
  $JunctionsSupported = Test-LinkKindSupported -Root $Sandbox -Kind "Junction"
  $SymbolicLinksSupported = Test-LinkKindSupported -Root $Sandbox -Kind "SymbolicLink"
  Assert-True $JunctionsSupported "this host must support directory junctions to prove the physical-root boundary"
  if (-not $SymbolicLinksSupported) {
    Write-Host "note: symbolic links are unavailable in this session; symlink boundary cases are skipped"
  }

  # --- root package.json is the sole product version authority ---
  $Authority = Join-FixturePath $Sandbox "repo-authority"
  New-Item $Authority -ItemType Directory -Force | Out-Null
  Set-Content (Join-Path $Authority "package.json") '{"name":"the-athanor","version":"1.2.3"}' -Encoding utf8NoBOM
  Assert-True ((Get-NativeReleaseVersion -RepositoryRoot $Authority) -ceq "1.2.3") "an omitted version must resolve from the root authority"
  Assert-True ((Get-NativeReleaseVersion -RepositoryRoot $Authority -Requested "") -ceq "1.2.3") "an empty requested version must resolve from the root authority"
  Assert-True ((Get-NativeReleaseVersion -RepositoryRoot $Authority -Requested "1.2.3") -ceq "1.2.3") "a requested version equal to the authority must be accepted"
  Assert-Refused { Get-NativeReleaseVersion -RepositoryRoot $Authority -Requested "9.9.9" } "a requested version that disagrees with the authority must be refused"
  Assert-Refused { Get-NativeReleaseVersion -RepositoryRoot $Authority -Requested "1.2.3.0" } "a near-miss requested version must be refused"
  Assert-Refused { Get-NativeReleaseVersion -RepositoryRoot $Authority -Requested "v1.2.3" } "a tag-shaped requested version must be refused rather than silently normalised"
  Assert-True ((Get-NativeReleaseVersion -RepositoryRoot $Authority -Requested "1.2.3+dev.202609051700.b639752") -ceq "1.2.3+dev.202609051700.b639752") "a build suffix over the authority must be accepted and kept as the release identity"
  Assert-Refused { Get-NativeReleaseVersion -RepositoryRoot $Authority -Requested "9.9.9+dev1" } "a build suffix must not excuse a product version that disagrees with the authority"
  Assert-Refused { Get-NativeReleaseVersion -RepositoryRoot $Authority -Requested "1.2.3-rc1" } "a prerelease is product lineage and must match the authority exactly"

  $Mismatch = Get-Refusal { Get-NativeReleaseVersion -RepositoryRoot $Authority -Requested "9.9.9" }
  Assert-True ($Mismatch -like "*9.9.9*" -and $Mismatch -like "*1.2.3*") "a mismatch refusal must name both versions"

  $Unversioned = Join-FixturePath $Sandbox "repo-unversioned"
  New-Item $Unversioned -ItemType Directory -Force | Out-Null
  Set-Content (Join-Path $Unversioned "package.json") '{"name":"the-athanor"}' -Encoding utf8NoBOM
  Assert-Refused { Get-NativeReleaseVersion -RepositoryRoot $Unversioned } "a root manifest without a version must be refused"
  Assert-Refused { Get-NativeReleaseVersion -RepositoryRoot (Join-Path $Sandbox "repo-absent") } "a missing root manifest must be refused"

  # --- the Athanor product-version grammar, deliberately not SemVer ---
  foreach ($Good in @("1.2.3", "0.9.7.2", "0.9.6.2-rc2", "1.0.0-beta.1", "1.2.3+build7", "10.20.30.40", "1.2.3-rc1")) {
    Assert-NativeReleaseProductVersion -Version $Good -Description "product version grammar probe"
  }
  foreach ($Bad in @(
      "", " ", "1", "1.2", "1.2.3.4.5", "v1.2.3", "1.2.3.", ".1.2.3", "1..2.3", "1.2.3-", "1.2.3+",
      "1.2.3-+rc1", "1.2.3--rc1", "1.2.3-.rc1", "1.2.3-rc..1", "1.2.3_4", "1.2.3 ", "1.2.3-rc 1",
      "1.2.3-rc:1", "1.2.3-../evil", ("1.2.3-" + ("a" * 130)))) {
    Assert-Refused { Assert-NativeReleaseProductVersion -Version $Bad -Description "product version grammar probe" } "the product-version grammar must refuse '$Bad'"
  }

  # The authorized product lineage carries a fourth numeric component.
  $Lineage = Join-FixturePath $Sandbox "repo-lineage"
  New-Item $Lineage -ItemType Directory -Force | Out-Null
  Set-Content (Join-Path $Lineage "package.json") '{"name":"the-athanor","version":"0.9.7.2"}' -Encoding utf8NoBOM
  Assert-True ((Get-NativeReleaseVersion -RepositoryRoot $Lineage) -ceq "0.9.7.2") "a four-component product version must be accepted from the authority"
  Assert-True ((Get-NativeReleaseVersion -RepositoryRoot $Lineage -Requested "0.9.7.2") -ceq "0.9.7.2") "a four-component request equal to the authority must be accepted"
  Assert-Refused { Get-NativeReleaseVersion -RepositoryRoot $Lineage -Requested "0.9.7" } "a three-component request must not match a four-component authority"

  $Malformed = Get-Refusal { Get-NativeReleaseVersion -RepositoryRoot $Authority -Requested "1.2" }
  Assert-True ($Malformed -clike "*not an Athanor product version*") "a malformed request must be refused by shape"
  Assert-True (-not ($Malformed -clike "*does not match*")) "shape must be refused before the equality check, never reported as a mismatch"

  $BadAuthority = Join-FixturePath $Sandbox "repo-bad-authority"
  New-Item $BadAuthority -ItemType Directory -Force | Out-Null
  Set-Content (Join-Path $BadAuthority "package.json") '{"name":"the-athanor","version":"1.2.3-rc_1"}' -Encoding utf8NoBOM
  Assert-Refused { Get-NativeReleaseVersion -RepositoryRoot $BadAuthority } "a malformed authority must be refused before any build or packaging"

  # --- one timing report, one record per stage ---
  $StageOut = Join-FixturePath $Sandbox "dist"
  Assert-True ((Get-NativeReleaseTimingReportPath $StageOut) -eq (Join-Path $StageOut "native-release-timings.jsonl")) "the timing report must live at <OutDir>/native-release-timings.jsonl"
  Assert-True (-not (Test-Path $StageOut)) "the timing sandbox must start absent"

  $Captured = Invoke-NativeReleaseStage -Name "toolchain-preflight" -OutDir $StageOut -Action {
    Start-Sleep -Milliseconds 25
    "preflight-identity"
  }
  Assert-True ($Captured -ceq "preflight-identity") "a stage must pass its action result through"
  $Report = Get-NativeReleaseTimingReportPath $StageOut
  Assert-True (Test-Path $Report -PathType Leaf) "a stage must create the report inside a previously absent OutDir"

  Assert-Refused { Invoke-NativeReleaseStage -Name "cargo-build" -OutDir $StageOut -Action { throw "stage exploded" } } "a failing stage must rethrow after recording"

  $Lines = @(Get-Content $Report)
  Assert-True ($Lines.Count -eq 2) "each stage must append exactly one record; found $($Lines.Count)"
  $Success = $Lines[0] | ConvertFrom-Json
  $Failure = $Lines[1] | ConvertFrom-Json

  foreach ($Record in @($Success, $Failure)) {
    $Started = Read-Timestamp $Record.startedAt
    $Completed = Read-Timestamp $Record.completedAt
    Assert-True ($Record.schema -ceq "athanor.native-release.stage-timing") "every record must carry the stable schema name"
    Assert-True ($Record.schemaVersion -eq 1) "every record must carry the schema version"
    Assert-True ($Started.Kind -eq [DateTimeKind]::Utc) "record start must be UTC"
    Assert-True ($Completed.Kind -eq [DateTimeKind]::Utc) "record completion must be UTC"
    Assert-True ($Completed -ge $Started) "a record must not complete before it starts"
    Assert-True ($Record.elapsedMs -ge 0) "elapsed milliseconds must be recorded"
  }
  Assert-True ($Success.stage -ceq "toolchain-preflight") "a record must name its stage"
  Assert-True ($Success.status -ceq "success") "a completed stage must be recorded as success"
  Assert-True ($Success.error -ceq "") "a successful stage must record no error"
  Assert-True ($Success.elapsedMs -ge 20) "a stage must measure its real duration"
  Assert-True ($Failure.stage -ceq "cargo-build") "a failure record must name its stage"
  Assert-True ($Failure.status -ceq "failure") "a thrown stage must be recorded as failure"
  Assert-True ($Failure.error -clike "*stage exploded*") "a failure record must retain the refusal message"

  Assert-Refused { Invoke-NativeReleaseStage -Name "not-a-stage" -OutDir $StageOut -Action { } } "an unknown stage name must be refused"
  Assert-True ((@(Get-Content $Report)).Count -eq 2) "a refused stage name must not append a record"

  $Stages = Get-NativeReleaseStageNames
  foreach ($Required in @(
      "toolchain-preflight", "download-verification", "dependency-preparation", "cargo-build",
      "payload-materialization", "godot-import", "manifest-hashing", "output-copy", "inno-packaging")) {
    Assert-True ($Stages -ccontains $Required) "the shared contract must own the stage name $Required"
  }

  # --- stable file identity ---
  $FirstCopy = Join-FixturePath $Sandbox "identity-a.bin"
  $SecondCopy = Join-FixturePath $Sandbox "nested/identity-b.bin"
  $Different = Join-FixturePath $Sandbox "identity-c.bin"
  New-Item (Join-Path $Sandbox "nested") -ItemType Directory -Force | Out-Null
  Set-Content $FirstCopy "same-bytes" -Encoding ascii
  Set-Content $SecondCopy "same-bytes" -Encoding ascii
  Set-Content $Different "other-bytes" -Encoding ascii
  $FirstIdentity = Get-NativeReleaseFileIdentity $FirstCopy
  Assert-True ($FirstIdentity -ceq (Get-NativeReleaseFileIdentity $FirstCopy)) "file identity must be stable across repeated reads"
  Assert-True ($FirstIdentity -ceq (Get-NativeReleaseFileIdentity $SecondCopy)) "identical bytes in another location must share one identity"
  Assert-True ($FirstIdentity -cne (Get-NativeReleaseFileIdentity $Different)) "different bytes must produce a different identity"
  Assert-Refused { Get-NativeReleaseFileIdentity (Join-Path $Sandbox "absent.bin") } "a missing file must be refused"
  Assert-Refused { Get-NativeReleaseFileIdentity $Sandbox } "a directory must be refused as a file identity"

  if ($SymbolicLinksSupported) {
    $LinkedIdentity = Join-FixturePath $Sandbox "identity-link.bin"
    New-Item -ItemType SymbolicLink -Path $LinkedIdentity -Target $FirstCopy | Out-Null
    Assert-True ((Get-Refusal { Get-NativeReleaseFileIdentity $LinkedIdentity }) -clike "*reparse point*") "a symlinked file must be refused as trusted build input bytes"
  }

  # --- the physical-root boundary: lexical containment is never enough ---
  $BoundaryRoot = Join-FixturePath $Sandbox "boundary/root"
  $BoundaryOutside = Join-FixturePath $Sandbox "boundary/outside"
  $BoundaryInside = Join-FixturePath $BoundaryRoot "real/inside.bin"
  New-Item (Join-Path $BoundaryRoot "real") -ItemType Directory -Force | Out-Null
  New-Item $BoundaryOutside -ItemType Directory -Force | Out-Null
  Set-Content $BoundaryInside "inside" -Encoding ascii
  Set-Content (Join-Path $BoundaryOutside "outside.bin") "outside" -Encoding ascii
  Assert-True ((Get-NativeReleaseTrustedPathRefusal -Path $BoundaryInside -Root $BoundaryRoot -Description "probe") -ceq "") "a regular file under its physical root must be trusted"
  Assert-True ((Get-NativeReleaseTrustedPathRefusal -Path (Join-Path $BoundaryRoot "real") -Root $BoundaryRoot -Description "probe" -Directory) -ceq "") "a real directory under its physical root must be trusted"
  Assert-True ((Get-NativeReleaseTrustedPathRefusal -Path (Join-Path $BoundaryRoot "real/../../outside/outside.bin") -Root $BoundaryRoot -Description "probe") -clike "*resolved outside its declared root*") "a traversal escape must be refused"
  Assert-True ((Get-NativeReleaseTrustedPathRefusal -Path (Join-Path $BoundaryRoot "real") -Root $BoundaryRoot -Description "probe") -clike "*not a regular file*") "a directory must be refused where a trusted file is required"
  Assert-True ((Get-NativeReleaseTrustedPathRefusal -Path $BoundaryInside -Root $BoundaryRoot -Description "probe" -Directory) -clike "*not a directory*") "a file must be refused where a trusted directory is required"
  Assert-True ((Get-NativeReleaseTrustedPathRefusal -Path (Join-Path $BoundaryRoot "real/absent.bin") -Root $BoundaryRoot -Description "probe") -clike "*does not exist*") "an absent input must be refused"
  Assert-True ((Get-NativeReleaseTrustedPathRefusal -Path $BoundaryRoot -Root $BoundaryRoot -Description "probe" -Directory) -clike "*resolved outside its declared root*") "the declared root is not itself an input under the root"
  Assert-Refused { Assert-NativeReleaseTrustedPath -Path (Join-Path $BoundaryOutside "outside.bin") -Root $BoundaryRoot -Description "probe" } "the asserting form must throw on refusal"
  Assert-True ((Assert-NativeReleaseTrustedPath -Path $BoundaryInside -Root $BoundaryRoot -Description "probe") -ceq $BoundaryInside) "the asserting form must return the verified full path"

  $JunctionInside = Join-FixturePath $BoundaryRoot "linked"
  New-Item -ItemType Junction -Path $JunctionInside -Target $BoundaryOutside | Out-Null
  $JunctionRefusal = Get-NativeReleaseTrustedPathRefusal -Path (Join-Path $JunctionInside "outside.bin") -Root $BoundaryRoot -Description "probe"
  Assert-True ($JunctionRefusal -clike "*crosses a Windows reparse point*") "a file reached through a junction ancestor must be refused"
  Assert-True ($JunctionRefusal -clike "*$JunctionInside*") "a reparse refusal must name the redirected ancestor"
  Assert-True ((Get-NativeReleaseTrustedPathRefusal -Path $JunctionInside -Root $BoundaryRoot -Description "probe" -Directory) -clike "*crosses a Windows reparse point*") "a junction must be refused as a trusted directory"
  Assert-True ((Get-NativeReleaseTrustedPathRefusal -Path (Join-Path $JunctionInside "outside.bin") -Root $BoundaryRoot -Description "probe") -ceq $JunctionRefusal) "the same boundary breach must be refused deterministically"

  $RedirectedRoot = Join-FixturePath $Sandbox "boundary/redirected-root"
  New-Item -ItemType Junction -Path $RedirectedRoot -Target $BoundaryOutside | Out-Null
  Assert-True ((Get-NativeReleaseTrustedPathRefusal -Path (Join-Path $RedirectedRoot "outside.bin") -Root $RedirectedRoot -Description "probe") -clike "*crosses a Windows reparse point*") "a declared root that is itself a reparse point must be refused"

  if ($SymbolicLinksSupported) {
    $LinkedFile = Join-FixturePath $BoundaryRoot "real/linked.bin"
    New-Item -ItemType SymbolicLink -Path $LinkedFile -Target (Join-Path $BoundaryOutside "outside.bin") | Out-Null
    Assert-True ((Get-NativeReleaseTrustedPathRefusal -Path $LinkedFile -Root $BoundaryRoot -Description "probe") -clike "*crosses a Windows reparse point*") "a symlinked file inside the root must be refused"
  }

  # --- required command refusal and stable tool identity ---
  Assert-Refused { Get-NativeReleaseCommandIdentity -Name "athanor-absent-tool-$([Guid]::NewGuid().ToString('N'))" } "a missing required command must be refused"

  $ProbeName = "athanor-contract-probe"
  $BinFirst = Join-FixturePath $Sandbox "bin-first"
  $BinSecond = Join-FixturePath $Sandbox "bin-second"
  $FirstProbeTool = Join-FixturePath $BinFirst "$ProbeName.cmd"
  $SecondProbeTool = Join-FixturePath $BinSecond "$ProbeName.cmd"
  New-CommandFixture $FirstProbeTool @("echo $ProbeName 1.2.3")
  New-CommandFixture $SecondProbeTool @("echo $ProbeName 1.2.3")

  $env:PATH = "$BinFirst;$OriginalPath"
  $ProbedFirst = Get-NativeReleaseCommandIdentity -Name $ProbeName -VersionArguments @("--version")
  Assert-True ($ProbedFirst.version -ceq "$ProbeName 1.2.3") "a probed tool identity must record its first version line"
  Assert-True ($ProbedFirst.path.StartsWith($BinFirst, [StringComparison]::OrdinalIgnoreCase)) "a tool identity must record where it resolved from"
  Assert-True ($ProbedFirst.key -ceq (Get-NativeReleaseCommandIdentity -Name $ProbeName -VersionArguments @("--version")).key) "a tool cache key must be stable across repeated probes"

  $Unprobed = Get-NativeReleaseCommandIdentity -Name $ProbeName
  Assert-True ($Unprobed.version -ceq "") "a tool identity without probe arguments must stay version-free"
  Assert-True ($Unprobed.file -ceq $ProbedFirst.file) "both identity modes must agree on the tool bytes"

  # An explicit path takes the identity of exactly those bytes, whatever PATH says.
  $Explicit = Get-NativeReleaseCommandIdentity -Name $ProbeName -Path $SecondProbeTool -VersionArguments @("--version")
  Assert-True ($Explicit.path -ceq $SecondProbeTool) "an explicit command path must be honoured over PATH resolution"
  Assert-True ($Explicit.key -ceq $ProbedFirst.key) "identical bytes must keep one cache key however they were named"
  Assert-Refused { Get-NativeReleaseCommandIdentity -Name $ProbeName -Path (Join-Path $Sandbox "absent-tool.cmd") } "an explicit path that does not exist must be refused"

  $env:PATH = "$BinSecond;$OriginalPath"
  $ProbedSecond = Get-NativeReleaseCommandIdentity -Name $ProbeName -VersionArguments @("--version")
  Assert-True ($ProbedSecond.path -cne $ProbedFirst.path) "the second fixture must resolve from a different path"
  Assert-True ($ProbedSecond.key -ceq $ProbedFirst.key) "an identical tool must keep one cache key across install locations"

  New-CommandFixture $SecondProbeTool @("echo $ProbeName 9.9.9")
  $ProbedChanged = Get-NativeReleaseCommandIdentity -Name $ProbeName -VersionArguments @("--version")
  Assert-True ($ProbedChanged.key -cne $ProbedFirst.key) "a changed tool must invalidate its cache key"

  $FailingName = "athanor-contract-failing-probe"
  New-CommandFixture (Join-FixturePath $BinSecond "$FailingName.cmd") @("exit /b 3")
  Assert-Refused { Get-NativeReleaseCommandIdentity -Name $FailingName -VersionArguments @("--version") } "a command that fails its identity probe must be refused"

  # --- rust-toolchain.toml is the one Rust pin authority ---
  $RustChannel = "1.95.0"
  $PinnedRepository = Join-FixturePath $Sandbox "repo-pinned"
  New-Item $PinnedRepository -ItemType Directory -Force | Out-Null
  Set-Content (Join-Path $PinnedRepository "rust-toolchain.toml") "[toolchain]`nchannel = `"$RustChannel`"`n" -Encoding utf8NoBOM
  Assert-True ((Get-NativeReleaseRustChannel -RepositoryRoot $PinnedRepository) -ceq $RustChannel) "the Rust channel must come from rust-toolchain.toml"

  $FloatingRepository = Join-FixturePath $Sandbox "repo-floating"
  New-Item $FloatingRepository -ItemType Directory -Force | Out-Null
  Set-Content (Join-Path $FloatingRepository "rust-toolchain.toml") "[toolchain]`nchannel = `"stable`"`n" -Encoding utf8NoBOM
  Assert-True ((Get-Refusal { Get-NativeReleaseRustChannel -RepositoryRoot $FloatingRepository }) -clike "*exactly pinned Rust channel*") "a floating Rust channel must be refused"
  Assert-Refused { Get-NativeReleaseRustChannel -RepositoryRoot (Join-Path $Sandbox "repo-absent") } "a missing Rust pin must be refused"

  # --- cheap refusal must not invoke a version probe ---
  $EarlyBin = Join-FixturePath $Sandbox "early-refusal-tools"
  $EarlyProbeSentinel = Join-FixturePath $Sandbox "early-version-probe-ran"
  foreach ($Name in @("cargo", "rustc", "rustup")) {
    New-CommandFixture (Join-FixturePath $EarlyBin "$Name.cmd") @("echo invoked>>`"$EarlyProbeSentinel`"", "echo $Name $RustChannel")
  }
  foreach ($Name in $OriginalToolchainEnvironment.Keys) {
    [Environment]::SetEnvironmentVariable($Name, $null, "Process")
  }
  $env:CARGO_HOME = Join-FixturePath $Sandbox "absent-cargo-home"
  $env:RUSTUP_HOME = Join-FixturePath $Sandbox "absent-rustup-home"
  $env:PATH = $EarlyBin
  Assert-Refused { Assert-NativeReleaseToolchain -RepositoryRoot $PinnedRepository } "preflight must refuse an incomplete native environment"
  Assert-True (-not (Test-Path $EarlyProbeSentinel)) "preflight must not invoke cargo, rustc, or rustup before cheap native checks pass"

  # --- one trusted rustup proxy root, one pinned selected toolchain ---
  $VisualCppRoot = Join-FixturePath $Sandbox "vs-tools"
  $VisualCppBin = Join-FixturePath $VisualCppRoot "bin/Hostx64/x64"
  $ForeignBin = Join-FixturePath $Sandbox "foreign-tools"
  $CargoHome = Join-FixturePath $Sandbox "cargo-home"
  $ProxyRoot = Join-FixturePath $CargoHome "bin"
  $RustupHome = Join-FixturePath $Sandbox "rustup-home"
  $ToolchainBin = Join-FixturePath $RustupHome "toolchains/$RustChannel-x86_64-pc-windows-msvc/bin"
  $SelectedCargo = Join-FixturePath $ToolchainBin "cargo.cmd"
  $SelectedRustc = Join-FixturePath $ToolchainBin "rustc.cmd"
  $ProxySentinel = Join-FixturePath $Sandbox "path-proxy-probe-ran"

  foreach ($Name in @("cl", "nmake")) {
    New-CommandFixture (Join-FixturePath $VisualCppBin "$Name.cmd") @("echo $Name")
  }
  New-CommandFixture (Join-FixturePath $ForeignBin "link.cmd") @("echo foreign link")
  # The PATH proxies must never be executed: cargo and rustc take their identity
  # from the pinned toolchain, so a proxy probe here would be a contract breach.
  foreach ($Name in @("cargo", "rustc")) {
    New-CommandFixture (Join-FixturePath $ProxyRoot "$Name.cmd") @("echo invoked>>`"$ProxySentinel`"", "echo $Name 0.0.0-proxy")
  }
  New-CommandFixture (Join-FixturePath $ProxyRoot "rustup.cmd") @(
    "if `"%1`"==`"which`" goto which",
    "echo rustup 1.28.2 fixture",
    "exit /b 0",
    ":which",
    "if `"%4`"==`"cargo`" echo $SelectedCargo",
    "if `"%4`"==`"rustc`" echo $SelectedRustc",
    "exit /b 0"
  )
  New-CommandFixture $SelectedCargo @("echo cargo $RustChannel selected")
  New-CommandFixture $SelectedRustc @("echo rustc $RustChannel selected")

  $env:VCToolsVersion = "14.test"
  $env:VCToolsInstallDir = $VisualCppRoot
  $env:WindowsSDKVersion = "10.0.test\"
  $env:VSCMD_ARG_TGT_ARCH = "x64"
  $env:CARGO_HOME = $CargoHome
  $env:RUSTUP_HOME = $RustupHome

  $env:PATH = "$ForeignBin;$VisualCppBin;$ProxyRoot;$OriginalPath"
  $ForeignLinkError = Get-Refusal { Assert-NativeReleaseToolchain -RepositoryRoot $PinnedRepository }
  Assert-True ($ForeignLinkError -clike "*Visual Studio command 'link' under VCToolsInstallDir resolved outside its declared root*") "preflight must reject a foreign link command"
  Assert-True (-not (Test-Path $ProxySentinel)) "a refused Visual Studio command must be refused before any probe"

  New-CommandFixture (Join-FixturePath $VisualCppBin "link.cmd") @("echo Microsoft link")
  $env:PATH = "$VisualCppBin;$ProxyRoot;$OriginalPath"
  $AcceptedToolchain = Assert-NativeReleaseToolchain -RepositoryRoot $PinnedRepository
  Assert-True ($AcceptedToolchain.format -eq 2) "the accepted toolchain must declare its contract format"
  Assert-True ($AcceptedToolchain.commands.Count -eq 6) "preflight must return all six command identities"
  Assert-True ($AcceptedToolchain.visualCppToolsRoot -ceq $VisualCppRoot) "preflight must retain the verified Visual Studio root"
  Assert-True (($AcceptedToolchain.commands | Where-Object { $_.name -ceq "link" }).path.StartsWith($VisualCppRoot, [StringComparison]::OrdinalIgnoreCase)) "accepted link must come from VCToolsInstallDir"
  Assert-True ($AcceptedToolchain.rustChannel -ceq $RustChannel) "preflight must retain the pinned Rust channel"
  Assert-True ($AcceptedToolchain.rustupProxyRoot -ceq $ProxyRoot) "preflight must retain the verified rustup proxy root"
  Assert-True ($AcceptedToolchain.cargoPath -ceq $SelectedCargo) "the build must use the cargo the pinned toolchain selects"
  Assert-True ($AcceptedToolchain.rustcPath -ceq $SelectedRustc) "the build must use the rustc the pinned toolchain selects"
  Assert-True (-not (Test-Path $ProxySentinel)) "preflight must probe the selected pinned binaries, never the PATH proxies"
  foreach ($Name in @("cargo", "rustc")) {
    $Identity = $AcceptedToolchain.commands | Where-Object { $_.name -ceq $Name }
    Assert-True ($Identity.path.StartsWith($ToolchainBin, [StringComparison]::OrdinalIgnoreCase)) "the $Name identity must bind the selected pinned binary, not the proxy"
    Assert-True ($Identity.version -clike "$Name $RustChannel*") "the $Name identity must report the pinned toolchain version"
    Assert-True ($AcceptedToolchain.cacheKeyMaterial -ccontains $Identity.key) "the cache key must bind the selected $Name identity"
  }
  Assert-True ($AcceptedToolchain.cacheKeyMaterial -ccontains "rust=$RustChannel") "the cache key must bind the pinned Rust channel"
  $AcceptedCacheKey = $AcceptedToolchain.cacheKeyMaterial -join "|"
  $RepeatedCacheKey = (Assert-NativeReleaseToolchain -RepositoryRoot $PinnedRepository).cacheKeyMaterial -join "|"
  Assert-True ($RepeatedCacheKey -ceq $AcceptedCacheKey) "an unchanged environment must produce one stable cache key"

  # Changed pinned bytes must invalidate the cache key even though every version
  # string and every PATH proxy stayed exactly the same.
  New-CommandFixture $SelectedCargo @("echo cargo $RustChannel selected", "rem rebuilt toolchain bytes")
  $RebuiltCacheKey = (Assert-NativeReleaseToolchain -RepositoryRoot $PinnedRepository).cacheKeyMaterial -join "|"
  Assert-True ($RebuiltCacheKey -cne $AcceptedCacheKey) "changed pinned toolchain bytes must invalidate the cache key"
  New-CommandFixture $SelectedCargo @("echo cargo $RustChannel selected")

  # --- a PATH shadow can never reach the build ---
  $ShadowBin = Join-FixturePath $Sandbox "rust-shadow"
  $ShadowSentinel = Join-FixturePath $Sandbox "shadow-probe-ran"
  foreach ($Name in @("cargo", "rustc")) {
    New-CommandFixture (Join-FixturePath $ShadowBin "$Name.cmd") @("echo invoked>>`"$ShadowSentinel`"", "echo $Name $RustChannel")
  }
  $env:PATH = "$ShadowBin;$VisualCppBin;$ProxyRoot;$OriginalPath"
  $ShadowRefusal = Get-Refusal { Assert-NativeReleaseToolchain -RepositoryRoot $PinnedRepository }
  Assert-True ($ShadowRefusal -clike "*rustup proxy command 'cargo' resolved outside its declared root*") "a shadow cargo ahead of the rustup proxy root must be refused"
  Assert-True ($ShadowRefusal -clike "*rustup proxy command 'rustc' resolved outside its declared root*") "a shadow rustc ahead of the rustup proxy root must be refused"
  Assert-True (-not (Test-Path $ShadowSentinel)) "a shadowed Rust environment must be refused before any probe"
  Assert-True ((Get-Refusal { Assert-NativeReleaseToolchain -RepositoryRoot $PinnedRepository }) -ceq $ShadowRefusal) "the same shadowed environment must be refused deterministically"

  # A proxy root reached through a junction is not the trusted proxy root.
  $LinkedCargoHome = Join-FixturePath $Sandbox "linked-cargo-home"
  $LinkedProxyRoot = Join-FixturePath $LinkedCargoHome "bin"
  New-Item $LinkedCargoHome -ItemType Directory -Force | Out-Null
  New-Item -ItemType Junction -Path $LinkedProxyRoot -Target $ProxyRoot | Out-Null
  $env:CARGO_HOME = $LinkedCargoHome
  $env:PATH = "$LinkedProxyRoot;$VisualCppBin;$OriginalPath"
  Assert-True ((Get-Refusal { Assert-NativeReleaseToolchain -RepositoryRoot $PinnedRepository }) -clike "*crosses a Windows reparse point*") "a rustup proxy root reached through a junction must be refused"
  $env:CARGO_HOME = $CargoHome

  # --- rustup's real installation shape: proxies are links to rustup.exe ---
  if ($SymbolicLinksSupported) {
    $LinkedProxyHome = Join-FixturePath $Sandbox "linked-proxy-cargo-home"
    $LinkedProxyBin = Join-FixturePath $LinkedProxyHome "bin"
    New-Item $LinkedProxyBin -ItemType Directory -Force | Out-Null
    Copy-Item -LiteralPath (Join-FixturePath $ProxyRoot "rustup.cmd") -Destination (Join-FixturePath $LinkedProxyBin "rustup.cmd")
    foreach ($Name in @("cargo", "rustc")) {
      New-Item -ItemType SymbolicLink -Path (Join-FixturePath $LinkedProxyBin "$Name.cmd") -Target (Join-FixturePath $LinkedProxyBin "rustup.cmd") | Out-Null
    }
    $env:CARGO_HOME = $LinkedProxyHome
    $env:PATH = "$VisualCppBin;$LinkedProxyBin;$OriginalPath"
    $LinkedProxyToolchain = Assert-NativeReleaseToolchain -RepositoryRoot $PinnedRepository
    Assert-True ($LinkedProxyToolchain.cargoPath -ceq $SelectedCargo) "a rustup-installed proxy link must be accepted: only the selected pinned binary is ever executed"
    Assert-True (($LinkedProxyToolchain.cacheKeyMaterial -join "|") -ceq $AcceptedCacheKey) "a linked proxy must not change the pinned toolchain identity"

    # rustup itself is executed by preflight, so rustup must be real bytes.
    $LinkedRustupHome = Join-FixturePath $Sandbox "linked-rustup-cargo-home"
    $LinkedRustupBin = Join-FixturePath $LinkedRustupHome "bin"
    New-Item $LinkedRustupBin -ItemType Directory -Force | Out-Null
    foreach ($Name in @("cargo", "rustc")) {
      Copy-Item -LiteralPath (Join-FixturePath $ProxyRoot "$Name.cmd") -Destination (Join-FixturePath $LinkedRustupBin "$Name.cmd")
    }
    New-Item -ItemType SymbolicLink -Path (Join-FixturePath $LinkedRustupBin "rustup.cmd") -Target (Join-FixturePath $ProxyRoot "rustup.cmd") | Out-Null
    $env:CARGO_HOME = $LinkedRustupHome
    $env:PATH = "$VisualCppBin;$LinkedRustupBin;$OriginalPath"
    Assert-True ((Get-Refusal { Assert-NativeReleaseToolchain -RepositoryRoot $PinnedRepository }) -clike "*rustup proxy command 'rustup' crosses a Windows reparse point*") "a linked rustup must be refused, because preflight executes it"
    $env:CARGO_HOME = $CargoHome
  }

  # --- Visual Studio tools redirected out of a lexically correct path ---
  $OutsideMsvc = Join-FixturePath $Sandbox "outside-msvc-tools"
  New-Item $OutsideMsvc -ItemType Directory -Force | Out-Null
  foreach ($Name in @("cl", "link", "nmake")) {
    New-CommandFixture (Join-FixturePath $OutsideMsvc "$Name.cmd") @("echo redirected $Name")
  }
  $RedirectedMsvcBin = Join-FixturePath $VisualCppRoot "bin/Hostx64/x64-redirected"
  New-Item -ItemType Junction -Path $RedirectedMsvcBin -Target $OutsideMsvc | Out-Null
  $env:PATH = "$RedirectedMsvcBin;$ProxyRoot;$OriginalPath"
  $RedirectedRefusal = Get-Refusal { Assert-NativeReleaseToolchain -RepositoryRoot $PinnedRepository }
  foreach ($Name in @("cl", "link", "nmake")) {
    Assert-True ($RedirectedRefusal -clike "*Visual Studio command '$Name' under VCToolsInstallDir crosses a Windows reparse point*") "a $Name inside VCToolsInstallDir but physically redirected outside must be refused"
  }
  Assert-True ($RedirectedRefusal -clike "*$RedirectedMsvcBin*") "a redirected toolset refusal must name the reparse point"
  Assert-True (-not (Test-Path $ProxySentinel)) "a redirected toolset must be refused before any probe"

  if ($SymbolicLinksSupported) {
    $SymlinkedMsvcBin = Join-FixturePath $VisualCppRoot "bin/Hostx64/x64-symlinked"
    New-Item $SymlinkedMsvcBin -ItemType Directory -Force | Out-Null
    foreach ($Name in @("cl", "link", "nmake")) {
      New-Item -ItemType SymbolicLink -Path (Join-FixturePath $SymlinkedMsvcBin "$Name.cmd") -Target (Join-FixturePath $OutsideMsvc "$Name.cmd") | Out-Null
    }
    $env:PATH = "$SymlinkedMsvcBin;$ProxyRoot;$OriginalPath"
    Assert-True ((Get-Refusal { Assert-NativeReleaseToolchain -RepositoryRoot $PinnedRepository }) -clike "*Visual Studio command 'cl' under VCToolsInstallDir crosses a Windows reparse point*") "a symlinked MSVC tool inside the toolset root must be refused"
  }

  # --- rustup must not hand out bytes from outside its toolchains root ---
  $RogueCargoHome = Join-FixturePath $Sandbox "rogue-cargo-home"
  $RogueProxyRoot = Join-FixturePath $RogueCargoHome "bin"
  $RogueCargo = Join-FixturePath $OutsideMsvc "cargo.cmd"
  New-CommandFixture $RogueCargo @("echo cargo $RustChannel rogue")
  foreach ($Name in @("cargo", "rustc")) {
    New-CommandFixture (Join-FixturePath $RogueProxyRoot "$Name.cmd") @("echo $Name 0.0.0-proxy")
  }
  New-CommandFixture (Join-FixturePath $RogueProxyRoot "rustup.cmd") @(
    "if `"%1`"==`"which`" goto which",
    "echo rustup 1.28.2 fixture",
    "exit /b 0",
    ":which",
    "if `"%4`"==`"cargo`" echo $RogueCargo",
    "if `"%4`"==`"rustc`" echo $SelectedRustc",
    "exit /b 0"
  )
  $env:CARGO_HOME = $RogueCargoHome
  $env:PATH = "$VisualCppBin;$RogueProxyRoot;$OriginalPath"
  Assert-True ((Get-Refusal { Assert-NativeReleaseToolchain -RepositoryRoot $PinnedRepository }) -clike "*pinned Rust $RustChannel cargo resolved outside its declared root*") "a selected binary outside the rustup toolchains root must be refused"
  $env:CARGO_HOME = $CargoHome

  # --- the selected toolchain must actually be the pinned one ---
  New-CommandFixture $SelectedRustc @("echo rustc 1.94.0 selected")
  $env:PATH = "$VisualCppBin;$ProxyRoot;$OriginalPath"
  Assert-True ((Get-Refusal { Assert-NativeReleaseToolchain -RepositoryRoot $PinnedRepository }) -clike "*selected rustc is not the pinned Rust toolchain $RustChannel*") "a selected rustc that is not the pinned toolchain must be refused"
  New-CommandFixture $SelectedRustc @("echo rustc $RustChannel selected")

  Assert-True ((Get-Refusal { Assert-NativeReleaseToolchain -RepositoryRoot $FloatingRepository }) -clike "*exactly pinned Rust channel*") "preflight must refuse a floating Rust channel"
  Assert-Refused { Assert-NativeReleaseToolchain -RepositoryRoot (Join-Path $Sandbox "repo-absent") } "preflight must refuse a repository without a Rust pin"
  $RestoredToolchain = Assert-NativeReleaseToolchain -RepositoryRoot $PinnedRepository
  Assert-True (($RestoredToolchain.cacheKeyMaterial -join "|") -ceq $AcceptedCacheKey) "the restored environment must reproduce the accepted cache key"

  # --- the local deploy driver is thin: tests, one payload, one staged install, proofs ---
  $DeployScript = Join-Path $PSScriptRoot "../substrate/deploy-local.ps1"
  $DeployParseErrors = $null
  $DeployTokens = $null
  $DeployAst = [Management.Automation.Language.Parser]::ParseFile($DeployScript, [ref]$DeployTokens, [ref]$DeployParseErrors)
  Assert-True ($DeployParseErrors.Count -eq 0) "the local deploy script must remain valid PowerShell"
  $DeployCalls = @($DeployAst.FindAll({
        param($Node)
        $Node -is [Management.Automation.Language.CommandAst] -and
        $Node.GetCommandName() -eq "Invoke-Checked"
      }, $true))
  $DeploySource = Get-Content $DeployScript -Raw
  $WorkspaceTests = @($DeployCalls | Where-Object { $_.Extent.Text -match '"test",\s*"--workspace"' })
  $AdapterTests = @($DeployCalls | Where-Object { $_.Extent.Text -match 'adapters/omp/deploy-local\.ps1.*"-TestsOnly"' })
  $PayloadBuilds = @($DeployCalls | Where-Object { $_.Extent.Text -match 'installer/build-native-release\.ps1' })
  $Installs = @($DeployCalls | Where-Object { $_.Extent.Text -match '"update",\s*"--staging",\s*\$Payload,\s*"--manifest",\s*\$Manifest' })
  $DoctorCalls = @($DeployCalls | Where-Object { $_.Extent.Text -match '"doctor"' })
  Assert-True ($WorkspaceTests.Count -eq 1) "the deploy driver must run the whole Cargo workspace test suite exactly once"
  Assert-True ($AdapterTests.Count -eq 1) "the deploy driver must run the OMP adapter tests exactly once, through the adapter's own driver"
  Assert-True ($PayloadBuilds.Count -eq 1) "the deploy driver must build exactly one native release payload"
  Assert-True ($PayloadBuilds[0].Extent.Text -match '"-Version",\s*\$Release,\s*"-OutDir",\s*\$Out') "the payload build must receive the release identity and the deploy output directory"
  Assert-True ($Installs.Count -eq 1) "the deploy driver must install exactly once, through the staged manager's update command"
  Assert-True ($Installs[0].Extent.Text -match '-FilePath\s+\$StagedManager') "the staged manager must run the update, because it carries the new installer code"
  Assert-True ($DoctorCalls.Count -eq 1) "installed release manifest proof must retain exactly one Doctor invocation"
  Assert-True ($DoctorCalls[0].Extent.Text -match '-FilePath\s+\$InstalledManager' -and $DoctorCalls[0].Extent.Text -match '-Label\s+"native release manifest proof"') "the installed manager must run the Doctor proof under its checked label"
  $TestsAt = $DeploySource.IndexOf('if (-not $SkipTests)', [StringComparison]::Ordinal)
  $PayloadAt = $DeploySource.IndexOf('installer/build-native-release.ps1', [StringComparison]::Ordinal)
  $InstallAt = $DeploySource.IndexOf('"update", "--staging"', [StringComparison]::Ordinal)
  $DoctorAt = $DeploySource.IndexOf('"doctor"', [StringComparison]::Ordinal)
  $PointerAt = $DeploySource.IndexOf('current.json names', [StringComparison]::Ordinal)
  $HealthAt = $DeploySource.IndexOf('Full-mode health proof', [StringComparison]::Ordinal)
  Assert-True ($TestsAt -ge 0 -and $TestsAt -lt $PayloadAt -and $PayloadAt -lt $InstallAt -and $InstallAt -lt $DoctorAt -and $DoctorAt -lt $PointerAt -and $PointerAt -lt $HealthAt) "the driver must order tests, payload, install, Doctor, pointer check, then Full-mode health"
  $ServicePreflight = $DeploySource.IndexOf('$Service = Get-Service', [StringComparison]::Ordinal)
  Assert-True ($ServicePreflight -ge 0 -and $ServicePreflight -lt $TestsAt) "service state must be checked before any test or build work"
  Assert-True ($DeploySource -match '(?s)-notin\s+@\("Running",\s*"Stopped"\).*?recover it to Running or Stopped') "transitional service states must refuse with a recovery instruction"
  Assert-True ($DeploySource -match 'git -C \$Root rev-parse --short HEAD') "the release build suffix must name the HEAD commit"
  Assert-True ($DeploySource -match 'status --porcelain --untracked-files=no.*?"\.dirty"') "a release built from modified tracked files must carry the .dirty suffix"
  Assert-True ($DeploySource -match 'Get-NativeReleaseVersion\s+-RepositoryRoot\s+\$Root\s+-Requested\s+"\$Authority\+\$Build"') "the release identity must be the product version plus one build suffix, validated by the shared contract"
  Assert-True ($DeploySource -match '\[string\]\$Current\.version\s+-cne\s+\$Release') "the driver must refuse when current.json does not name the release it installed"
  Assert-True ($DeploySource -match '(?s)VSCMD_ARG_TGT_ARCH.*?Enter-VsDevShell.*?-arch=x64') "the driver must enter an x64 developer shell when the caller did not"
  Assert-True (-not ($DeploySource -match '\b(Copy|Move)-Item\b')) "the driver must never copy or move files itself; the manager owns the program root"
  $Removals = @([Regex]::Matches($DeploySource, 'Remove-Item'))
  Assert-True ($Removals.Count -eq 1 -and $Removals[0].Index -gt $HealthAt) "the driver's only deletion is the payload cleanup after every proof passed"
  Assert-True ($DeploySource -match '(?s)\$env:DATABASE_URL\s*=\s*\[string\]\$Secrets\.externalDatabaseUrl.*?finally\s*\{\s*\$env:DATABASE_URL\s*=\s*\$SavedDatabaseUrl') "the health proof must borrow the runtime database URL and give it back"
  Assert-True ($DeploySource -match '(?s)\$HealthTries\s*=\s*3.*?Start-Sleep\s+-Seconds\s+10') "the Full-mode health proof must retry a cold embedder before counting as red"
  $AdapterComponentSource = Get-Content (Join-Path $PSScriptRoot "omp-adapter-component.ps1") -Raw
  $AdapterAllowlist = [Regex]::Match($AdapterComponentSource, '(?s)function\s+Get-OmpAdapterComponentRuntimeAllowlist\b.*?\n\}')
  Assert-True ($AdapterAllowlist.Success) "the adapter component runtime allowlist must remain discoverable"
  $AdapterAllowlistEntries = (($AdapterAllowlist.Value -split "`r?`n" | Where-Object { $_ -notmatch '^\s*#' }) -join "`n")
  Assert-True (-not $AdapterAllowlistEntries.Contains("installed-loader.ts", [StringComparison]::OrdinalIgnoreCase)) "the product-owned loader must never move into the adapter component allowlist"
  $KeeperProvisioner = Join-Path $PSScriptRoot "../crates/omp-keeper/scripts/provision-local.ps1"
  $KeeperProvisionErrors = $null
  $KeeperProvisionTokens = $null
  [void][Management.Automation.Language.Parser]::ParseFile($KeeperProvisioner, [ref]$KeeperProvisionTokens, [ref]$KeeperProvisionErrors)
  Assert-True ($KeeperProvisionErrors.Count -eq 0) "the keeper provisioner must remain valid PowerShell"
  $KeeperProvisionSource = Get-Content $KeeperProvisioner -Raw
  foreach ($RequiredFragment in @("restart_claim", "restart_request", "restart_exit", "restart_verify", "omp-keeper.json", "restart-capability", "stateRoot", "watchIntervalSecs", "-Remove")) {
    Assert-True ($KeeperProvisionSource.Contains($RequiredFragment, [StringComparison]::Ordinal)) "keeper provisioner must retain $RequiredFragment"
  }
  $CapabilityProvisionSource = Get-Content (Join-Path $PSScriptRoot "../substrate/provision-restart-capability.ps1") -Raw
  Assert-True ($CapabilityProvisionSource -match 'Remove-Item\s+\$secretPath\s+-Force\s+-ErrorAction\s+Stop') "capability removal must refuse a holder secret it cannot delete"
  $SecretRemoval = $CapabilityProvisionSource.IndexOf('Remove-Item $secretPath', [StringComparison]::Ordinal)
  $DatabaseRemoval = $CapabilityProvisionSource.IndexOf('DELETE FROM restart.principal_capabilities', [StringComparison]::Ordinal)
  Assert-True ($SecretRemoval -ge 0 -and $SecretRemoval -lt $DatabaseRemoval) "capability rollback must remove plaintext before deleting its authority row"
  & (Join-Path $PSScriptRoot "../crates/omp-keeper/scripts/provision-local.test.ps1")
  $ReleaseBuilderSource = Get-Content (Join-Path $PSScriptRoot "build-native-release.ps1") -Raw
  foreach ($RequiredFragment in @("-p omp-keeper", "omp-keeper.exe", "components/omp-keeper", '"omp-keeper"', "-p athanor-install", "athanor.exe", "bin/athanor.exe", '"app"')) {
    Assert-True ($ReleaseBuilderSource.Contains($RequiredFragment, [StringComparison]::Ordinal)) "native release builder must package $RequiredFragment"
  }
  $InnoSource = Get-Content (Join-Path $PSScriptRoot "athanor.iss") -Raw
  foreach ($ForbiddenFragment in @("payload\bin\omp-keeper.exe", "payload\components\omp-keeper\provision-omp-keeper.ps1", "payload\components\omp-keeper\provision-restart-capability.ps1")) {
    Assert-True (-not $InnoSource.Contains($ForbiddenFragment, [StringComparison]::OrdinalIgnoreCase)) "native installer must not activate $ForbiddenFragment before the manager accepts the payload"
  }
  foreach ($RequiredFragment in @('payload\bin\athanor.exe', '{app}\bin\athanor.exe', '{app}\bin\athanor-manage.exe')) {
    Assert-True ($InnoSource.Contains($RequiredFragment, [StringComparison]::Ordinal)) "native installer must ship $RequiredFragment"
  }
  Assert-True ($InnoSource -match '(?m)^Name:\s*"\{group\}\\The Athanor";\s*Filename:\s*"\{app\}\\bin\\athanor\.exe"') "the Start Menu entry must launch the canonical app"
  Assert-True ($InnoSource -match '(?m)^Filename:\s*"\{app\}\\bin\\athanor-manage\.exe";\s*Parameters:\s*"uninstall"') "athanor-manage must remain the installer authority the uninstaller calls"


  Write-Host "native release contract passed"
} finally {
  foreach ($Name in $OriginalToolchainEnvironment.Keys) {
    [Environment]::SetEnvironmentVariable($Name, $OriginalToolchainEnvironment[$Name], "Process")
  }
  $env:PATH = $OriginalPath
  Remove-Item $Sandbox -Recurse -Force -ErrorAction SilentlyContinue
}
