$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot "omp-adapter-component.ps1")

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

function Test-LinkKindSupported([string]$Root, [string]$Kind) {
  # Junctions work on any NTFS volume; file symlinks need Developer Mode or an
  # elevated session. A boundary the host cannot express is reported, not faked.
  $Probe = Join-Path $Root "link-capability-$Kind"
  $TargetDirectory = Join-Path $Probe "target"
  New-Item $TargetDirectory -ItemType Directory -Force | Out-Null
  $TargetFile = Join-Path $TargetDirectory "target.txt"
  Set-Content -LiteralPath $TargetFile "target" -Encoding ascii
  $Target = if ($Kind -eq "Junction") { $TargetDirectory } else { $TargetFile }
  try {
    New-Item -ItemType $Kind -Path (Join-Path $Probe "probe") -Target $Target -ErrorAction Stop | Out-Null
    return $true
  } catch {
    return $false
  }
}

function Write-SandboxFile([string]$Path, [string]$Content) {
  New-Item ([IO.Path]::GetDirectoryName($Path)) -ItemType Directory -Force | Out-Null
  [IO.File]::WriteAllText($Path, $Content, [Text.UTF8Encoding]::new($false))
}

function New-SandboxRepository([string]$Root, [string]$Version) {
  $Adapter = Join-Path $Root "adapters/omp"
  Write-SandboxFile (Join-Path $Root "installer/dependencies.json") "{`"format`":1,`"schemaVersion`":19,`"components`":{`"hostApi`":2,`"substrateApi`":3,`"deliveryApi`":5,`"godotApi`":`"4.7`"}}"
  Write-SandboxFile (Join-Path $Adapter "package.json") "{`"name`":`"the-athanor-omp`",`"version`":`"$Version`"}"
  foreach ($Name in @("index.ts", "hygiene.ts", "athanor-root.ts", "discovery.ts", "giga.ts", "kitten-lineage.ts", "rust-transport.ts")) {
    Write-SandboxFile (Join-Path $Adapter $Name) "export const source = `"$Name`";`n"
  }
  Write-SandboxFile (Join-Path $Adapter "bunfig.toml") "[test]`nroot = `".`"`n"
  Write-SandboxFile (Join-Path $Adapter "README.md") "# adapter`n"
  Write-SandboxFile (Join-Path $Adapter "LICENSE") "Apache-2.0`n"
  Write-SandboxFile (Join-Path $Adapter "NOTICE") "notice`n"
  Write-SandboxFile (Join-Path $Adapter "house-proof/host.ts") "export const host = 1;`n"
  Write-SandboxFile (Join-Path $Adapter "house-proof/room.ts") "export const room = 2;`n"
  Write-SandboxFile (Join-Path $Adapter "starter-room/example/AGENTS.md") "# room`n"
  Write-SandboxFile (Join-Path $Adapter "starter-room/example/.athanor-room.json") "{`"room`":`"example`"}"
  # Present in the source tree, deliberately outside the runtime allowlist.
  Write-SandboxFile (Join-Path $Adapter "installed-loader.ts") "export const loader = 3;`n"
  Write-SandboxFile (Join-Path $Adapter "tests/adapter.test.ts") "export const test = 4;`n"
  return $Adapter
}

function Get-ExpectedReleaseId([object]$Manifest) {
  # An independent construction of the canonical identity from the manifest as
  # published, so the shipped releaseId is proven byte-for-byte rather than by
  # calling the same helper twice.
  $Text = "format=1`n"
  $Text += "component=omp-adapter`n"
  $Text += "version=$([string]$Manifest.version)`n"
  $Text += "hostApi=$([string]$Manifest.compatibility.hostApi)`n"
  $Text += "substrateApi=$([string]$Manifest.compatibility.substrateApi)`n"
  $Text += "deliveryApi=$([string]$Manifest.compatibility.deliveryApi)`n"
  $Text += "schemaVersion=$([string]$Manifest.compatibility.schemaVersion)`n"
  foreach ($Artifact in $Manifest.artifacts) {
    $Text += "artifact=$([string]$Artifact.path)`t$([string]$Artifact.sha256)`t$([string]$Artifact.size)`n"
  }
  $Bytes = [Text.UTF8Encoding]::new($false).GetBytes($Text)
  $Sha256 = [Security.Cryptography.SHA256]::Create()
  try { $Digest = $Sha256.ComputeHash($Bytes) } finally { $Sha256.Dispose() }
  return "$([string]$Manifest.version)-$([BitConverter]::ToString($Digest).Replace('-','').ToLowerInvariant())"
}

function Read-BundleManifest([string]$BundleRoot) {
  return Get-Content -LiteralPath (Join-Path $BundleRoot "component-manifest.json") -Raw | ConvertFrom-Json
}

function Get-FileBase64([string]$Path) {
  return [Convert]::ToBase64String([IO.File]::ReadAllBytes($Path))
}

$Sandbox = Join-Path ([IO.Path]::GetTempPath()) "athanor-omp-component-test-$PID-$([Guid]::NewGuid().ToString('N'))"
try {
  New-Item $Sandbox -ItemType Directory -Force | Out-Null
  $Repository = Join-Path $Sandbox "repo"
  $AdapterRoot = New-SandboxRepository -Root $Repository -Version "7.8.9"

  # --- authorities: adapter package.json version, dependencies.json compatibility ---
  Assert-True ((Get-OmpAdapterComponentVersion -RepositoryRoot $Repository) -ceq "7.8.9") "the adapter package.json must own the component version"
  $Compatibility = Get-OmpAdapterComponentCompatibility -RepositoryRoot $Repository
  Assert-True ($Compatibility.hostApi -eq 2 -and $Compatibility.substrateApi -eq 3 -and $Compatibility.deliveryApi -eq 5 -and $Compatibility.schemaVersion -eq 19) "compatibility must be read from installer/dependencies.json, never hardcoded"

  # --- one build stages the runtime allowlist and nothing else ---
  $First = New-OmpAdapterComponentBundle -RepositoryRoot $Repository -Destination (Join-Path $Sandbox "bundle-first")
  $Manifest = Read-BundleManifest $First.Root
  $Paths = @($Manifest.artifacts | ForEach-Object { [string]$_.path })

  Assert-True ($Manifest.format -eq 1) "the manifest must declare format 1"
  Assert-True ([string]$Manifest.component -ceq "omp-adapter") "the manifest must name the omp-adapter component"
  Assert-True ([string]$Manifest.version -ceq "7.8.9") "the manifest must publish the adapter version authority"
  Assert-True ($Manifest.compatibility.hostApi -eq 2 -and $Manifest.compatibility.schemaVersion -eq 19) "the manifest must publish the compatibility authority"
  Assert-True ($Paths.Count -eq $First.ArtifactCount) "the build result must report the manifest artifact count"
  Assert-True ($Paths -ccontains "index.ts" -and $Paths -ccontains "house-proof/host.ts" -and $Paths -ccontains "starter-room/example/AGENTS.md") "the component root must be flat relative to adapters/omp"
  Assert-True (-not ($Paths -ccontains "installed-loader.ts")) "installed-loader.ts must stay product-owned and out of the component"
  Assert-True (-not ($Paths | Where-Object { $_ -clike "tests/*" })) "adapter tests must stay out of the component"
  Assert-True (-not ($Paths | Where-Object { $_ -clike "adapters/omp/*" })) "component artifact paths must carry no adapters/omp prefix"

  $Staged = @(Get-ChildItem -LiteralPath $First.Root -File -Recurse -Force | ForEach-Object { [IO.Path]::GetRelativePath($First.Root, $_.FullName).Replace("\", "/") })
  Assert-True (@($Staged | Where-Object { $_ -cne "component-manifest.json" }).Count -eq $Paths.Count) "the bundle root must hold the manifest plus exactly the manifest artifacts"
  foreach ($Artifact in $Manifest.artifacts) {
    $StagedFile = Join-Path $First.Root ([string]$Artifact.path)
    Assert-True (Test-Path -LiteralPath $StagedFile -PathType Leaf) "every manifest artifact must exist in the bundle: $($Artifact.path)"
    Assert-True (((Get-FileHash -LiteralPath $StagedFile -Algorithm SHA256).Hash.ToLowerInvariant()) -ceq [string]$Artifact.sha256) "every manifest hash must describe the shipped bytes: $($Artifact.path)"
    Assert-True ((Get-Item -LiteralPath $StagedFile).Length -eq [long]$Artifact.size) "every manifest size must describe the shipped bytes: $($Artifact.path)"
    Assert-True ([string]$Artifact.sha256 -cmatch '^[0-9a-f]{64}$') "every manifest hash must be a lowercase full SHA-256: $($Artifact.path)"
  }

  # --- artifacts are sorted by ordinal path, not by culture ---
  for ($Index = 1; $Index -lt $Paths.Count; $Index++) {
    Assert-True ([string]::CompareOrdinal($Paths[$Index - 1], $Paths[$Index]) -lt 0) "artifacts must be strictly ordinal-sorted: '$($Paths[$Index - 1])' then '$($Paths[$Index])'"
  }
  Assert-True ([Array]::IndexOf($Paths, "LICENSE") -lt [Array]::IndexOf($Paths, "index.ts")) "ordinal sorting must place LICENSE before index.ts, unlike a culture-aware sort"

  # --- release identity is the canonical fingerprint of the manifest itself ---
  Assert-True ([string]$Manifest.releaseId -ceq (Get-ExpectedReleaseId $Manifest)) "releaseId must equal <version>-<sha256 of the canonical identity>"
  Assert-True ([string]$Manifest.releaseId -ceq $First.ReleaseId) "the build result must report the manifest releaseId"
  Assert-True ([string]$Manifest.releaseId -cmatch '^7\.8\.9-[0-9a-f]{64}$') "releaseId must be the adapter version joined to a lowercase digest"

  $Identity = Get-OmpAdapterComponentIdentity -Version ([string]$Manifest.version) -Compatibility $Manifest.compatibility -Artifacts @($Manifest.artifacts)
  Assert-True ($Identity.ReleaseId -ceq [string]$Manifest.releaseId) "recomputing identity from the published manifest must reproduce the releaseId"
  $CanonicalLines = $Identity.CanonicalIdentity.Split("`n")
  Assert-True ($CanonicalLines[-1] -ceq "") "every canonical line must be LF-terminated"
  Assert-True ($CanonicalLines[0] -ceq "format=1" -and $CanonicalLines[1] -ceq "component=omp-adapter" -and $CanonicalLines[2] -ceq "version=7.8.9") "the canonical header must be format, component, version"
  Assert-True ($CanonicalLines[3] -ceq "hostApi=2" -and $CanonicalLines[4] -ceq "substrateApi=3" -and $CanonicalLines[5] -ceq "deliveryApi=5" -and $CanonicalLines[6] -ceq "schemaVersion=19") "the canonical header must carry the four compatibility fields in order"
  Assert-True ($CanonicalLines[7] -ceq "artifact=$($Paths[0])`t$([string]$Manifest.artifacts[0].sha256)`t$([string]$Manifest.artifacts[0].size)") "each artifact line must be tab separated path, hash, size"
  Assert-True (-not ($Identity.CanonicalIdentity.Contains("`r"))) "the canonical identity must never contain CR"

  # --- identical inputs produce identical manifest bytes ---
  $Second = New-OmpAdapterComponentBundle -RepositoryRoot $Repository -Destination (Join-Path $Sandbox "bundle-second")
  Assert-True ((Get-FileBase64 $Second.ManifestPath) -ceq (Get-FileBase64 $First.ManifestPath)) "identical sources must produce identical manifest bytes"
  Assert-True ($Second.ReleaseId -ceq $First.ReleaseId) "identical sources must produce an identical releaseId"

  # --- one changed source byte invalidates the release identity ---
  $Loaded = [IO.File]::ReadAllText((Join-Path $AdapterRoot "index.ts"))
  [IO.File]::WriteAllText((Join-Path $AdapterRoot "index.ts"), $Loaded.Replace("index.ts", "index.tS"), [Text.UTF8Encoding]::new($false))
  $Changed = New-OmpAdapterComponentBundle -RepositoryRoot $Repository -Destination (Join-Path $Sandbox "bundle-changed")
  $ChangedManifest = Read-BundleManifest $Changed.Root
  $BeforeEntry = $Manifest.artifacts | Where-Object { [string]$_.path -ceq "index.ts" }
  $AfterEntry = $ChangedManifest.artifacts | Where-Object { [string]$_.path -ceq "index.ts" }
  Assert-True ([long]$AfterEntry.size -eq [long]$BeforeEntry.size) "the changed-byte case must keep the artifact size identical"
  Assert-True ([string]$AfterEntry.sha256 -cne [string]$BeforeEntry.sha256) "a changed source byte must change the artifact hash"
  Assert-True ([string]$ChangedManifest.releaseId -cne [string]$Manifest.releaseId) "a changed source byte must change the releaseId"
  Assert-True ([string]$ChangedManifest.releaseId -ceq (Get-ExpectedReleaseId $ChangedManifest)) "the changed release must still publish its own canonical identity"
  $UnchangedBefore = ($Manifest.artifacts | Where-Object { [string]$_.path -ceq "hygiene.ts" }).sha256
  $UnchangedAfter = ($ChangedManifest.artifacts | Where-Object { [string]$_.path -ceq "hygiene.ts" }).sha256
  Assert-True ([string]$UnchangedAfter -ceq [string]$UnchangedBefore) "untouched artifacts must keep their hashes"

  # --- unsafe artifact paths are refused, matching the installed validator ---
  foreach ($Unsafe in @("", ".", "..", "./index.ts", "a/../b.ts", "a/./b.ts", "/index.ts", "C:/index.ts", "c:\index.ts", "a\b.ts", "a//b.ts", "index:ts", "house-proof/", ("hygi" + [char]0x00E8 + "ne.ts"), ("index" + [char]0x0009 + ".ts"))) {
    Assert-Refused { Assert-OmpAdapterComponentArtifactPath -Path $Unsafe } "an unsafe artifact path must be refused: '$Unsafe'"
  }
  foreach ($Safe in @("index.ts", "house-proof/host.ts", "starter-room/example/.athanor-room.json")) {
    Assert-OmpAdapterComponentArtifactPath -Path $Safe
  }

  $GoodHash = "0" * 64
  $OtherHash = "1" * 64
  $Sound = @([ordered]@{ path = "LICENSE"; sha256 = $GoodHash; size = 0 }, [ordered]@{ path = "index.ts"; sha256 = $OtherHash; size = 12 })
  Assert-True ((Get-OmpAdapterComponentIdentity -Version "1.0.0" -Compatibility $Compatibility -Artifacts $Sound).ReleaseId -cmatch '^1\.0\.0-[0-9a-f]{64}$') "a sound artifact set must produce an identity"
  Assert-Refused { Get-OmpAdapterComponentIdentity -Version "1.0.0" -Compatibility $Compatibility -Artifacts @([ordered]@{ path = "index.ts"; sha256 = $GoodHash; size = 1 }, [ordered]@{ path = "LICENSE"; sha256 = $OtherHash; size = 1 }) } "an unsorted artifact list must be refused"
  Assert-Refused { Get-OmpAdapterComponentIdentity -Version "1.0.0" -Compatibility $Compatibility -Artifacts @([ordered]@{ path = "index.ts"; sha256 = $GoodHash; size = 1 }, [ordered]@{ path = "index.ts"; sha256 = $GoodHash; size = 1 }) } "a duplicated artifact path must be refused"
  Assert-Refused { Get-OmpAdapterComponentIdentity -Version "1.0.0" -Compatibility $Compatibility -Artifacts @([ordered]@{ path = "Index.ts"; sha256 = $GoodHash; size = 1 }, [ordered]@{ path = "index.ts"; sha256 = $OtherHash; size = 1 }) } "an ASCII case-folded duplicate path must be refused like the installed validator"
  Assert-Refused { Get-OmpAdapterComponentIdentity -Version "1.0.0" -Compatibility $Compatibility -Artifacts @([ordered]@{ path = "index.ts"; sha256 = ("A" * 64); size = 1 }) } "an uppercase hash must be refused"
  Assert-Refused { Get-OmpAdapterComponentIdentity -Version "1.0.0" -Compatibility $Compatibility -Artifacts @([ordered]@{ path = "index.ts"; sha256 = ("0" * 40); size = 1 }) } "a truncated hash must be refused"
  Assert-Refused { Get-OmpAdapterComponentIdentity -Version "1.0.0" -Compatibility $Compatibility -Artifacts @([ordered]@{ path = "index.ts"; sha256 = $GoodHash; size = -1 }) } "a negative size must be refused"
  Assert-Refused { Get-OmpAdapterComponentIdentity -Version "1.0.0" -Compatibility $Compatibility -Artifacts @([ordered]@{ path = "index.ts"; sha256 = $GoodHash; size = 1.5 }) } "a fractional size must be refused"
  Assert-Refused { Get-OmpAdapterComponentIdentity -Version "1.0.0" -Compatibility $Compatibility -Artifacts @([ordered]@{ path = "index.ts"; size = 1 }) } "an artifact without a hash must be refused"
  Assert-Refused { Get-OmpAdapterComponentIdentity -Version "1.0.0" -Compatibility $Compatibility -Artifacts @() } "an empty artifact set must be refused"
  Assert-Refused { Get-OmpAdapterComponentIdentity -Version "1.0.0" -Compatibility ([ordered]@{ hostApi = 1; substrateApi = 1; deliveryApi = 1 }) -Artifacts $Sound } "a compatibility set without schemaVersion must be refused"
  Assert-Refused { Get-OmpAdapterComponentIdentity -Version "1.0.0" -Compatibility ([ordered]@{ hostApi = 1; substrateApi = 1; deliveryApi = 1; schemaVersion = "18.1" }) -Artifacts $Sound } "a non-integer compatibility field must be refused"
  $MaxCompatibility = [ordered]@{ hostApi = 4294967295; substrateApi = 1; deliveryApi = 1; schemaVersion = 18 }
  Assert-True ((Get-OmpAdapterComponentIdentity -Version "1.0.0" -Compatibility $MaxCompatibility -Artifacts $Sound).Compatibility.hostApi -eq [uint32]::MaxValue) "the full unsigned 32-bit compatibility range must match Rust and the loader"
  Assert-Refused { Get-OmpAdapterComponentIdentity -Version "1.0.0" -Compatibility ([ordered]@{ hostApi = 4294967296; substrateApi = 1; deliveryApi = 1; schemaVersion = 18 }) -Artifacts $Sound } "compatibility above the unsigned 32-bit range must be refused"
  foreach ($BadVersion in @("", " ", "../evil", "1.0.0/2", "1.0.0\2", "1_0_0", "1..0", "1.0.0:1", ("9" * 129))) {
    Assert-Refused { Get-OmpAdapterComponentIdentity -Version $BadVersion -Compatibility $Compatibility -Artifacts $Sound } "an unsafe version must be refused: '$BadVersion'"
  }
  foreach ($GoodVersion in @("0.9.3", "1.0.0-rc1", "1.0.0+build7", "-1.0.0", ("9" * 128))) {
    $Accepted = Get-OmpAdapterComponentIdentity -Version $GoodVersion -Compatibility $Compatibility -Artifacts $Sound
    Assert-True ($Accepted.ReleaseId -ceq "$GoodVersion-$($Accepted.Fingerprint)") "the installed version grammar must be accepted: '$GoodVersion'"
    Assert-True ($Accepted.Fingerprint -cmatch '^[0-9a-f]{64}$') "a release fingerprint must be 64 lowercase hex: '$GoodVersion'"
  }

  # --- refusals around the bundle boundary ---
  Assert-Refused { New-OmpAdapterComponentBundle -RepositoryRoot $Repository -Destination $First.Root } "a non-empty destination must be refused rather than mixed with stale bytes"
  Assert-Refused { New-OmpAdapterComponentBundle -RepositoryRoot (Join-Path $Sandbox "absent") -Destination (Join-Path $Sandbox "bundle-absent") } "a missing repository root must be refused"

  $Incomplete = Join-Path $Sandbox "repo-incomplete"
  New-SandboxRepository -Root $Incomplete -Version "7.8.9" | Out-Null
  Remove-Item -LiteralPath (Join-Path $Incomplete "adapters/omp/NOTICE") -Force
  Assert-Refused { New-OmpAdapterComponentBundle -RepositoryRoot $Incomplete -Destination (Join-Path $Sandbox "bundle-incomplete") } "a missing allowlisted source must be refused"

  $NoRooms = Join-Path $Sandbox "repo-no-rooms"
  New-SandboxRepository -Root $NoRooms -Version "7.8.9" | Out-Null
  Remove-Item -LiteralPath (Join-Path $NoRooms "adapters/omp/starter-room") -Recurse -Force
  Assert-Refused { New-OmpAdapterComponentBundle -RepositoryRoot $NoRooms -Destination (Join-Path $Sandbox "bundle-no-rooms") } "a missing allowlisted source directory must be refused"

  $NoAuthority = Join-Path $Sandbox "repo-no-authority"
  New-SandboxRepository -Root $NoAuthority -Version "7.8.9" | Out-Null
  Remove-Item -LiteralPath (Join-Path $NoAuthority "installer/dependencies.json") -Force
  Assert-Refused { New-OmpAdapterComponentBundle -RepositoryRoot $NoAuthority -Destination (Join-Path $Sandbox "bundle-no-authority") } "a missing compatibility authority must be refused"
  Assert-True ((New-OmpAdapterComponentBundle -RepositoryRoot $NoAuthority -Destination (Join-Path $Sandbox "bundle-explicit") -Compatibility $Compatibility).ReleaseId -ceq $First.ReleaseId) "an explicitly supplied compatibility must reproduce the same identity"

  $BadVersionRepo = Join-Path $Sandbox "repo-bad-version"
  New-SandboxRepository -Root $BadVersionRepo -Version "0.0.0" | Out-Null
  Write-SandboxFile (Join-Path $BadVersionRepo "adapters/omp/package.json") "{`"name`":`"the-athanor-omp`"}"
  Assert-Refused { New-OmpAdapterComponentBundle -RepositoryRoot $BadVersionRepo -Destination (Join-Path $Sandbox "bundle-bad-version") } "an adapter manifest without a version must be refused"

  # --- linked external bytes are refused, never ingested ---
  $SymbolicLinksSupported = Test-LinkKindSupported -Root $Sandbox -Kind "SymbolicLink"
  Assert-True (Test-LinkKindSupported -Root $Sandbox -Kind "Junction") "this host must support directory junctions to prove the component source boundary"
  if (-not $SymbolicLinksSupported) {
    Write-Host "note: symbolic links are unavailable in this session; symlink boundary cases are skipped"
  }
  $External = Join-Path $Sandbox "external-bytes"
  Write-SandboxFile (Join-Path $External "smuggled.ts") "export const smuggled = 9;`n"

  $JunctionRepository = Join-Path $Sandbox "repo-junction-subtree"
  New-SandboxRepository -Root $JunctionRepository -Version "7.8.9" | Out-Null
  $LinkedSubtree = [IO.Path]::GetFullPath((Join-Path $JunctionRepository "adapters/omp/starter-room/linked"))
  New-Item -ItemType Junction -Path $LinkedSubtree -Target $External | Out-Null
  $JunctionDestination = Join-Path $Sandbox "bundle-junction-subtree"
  $JunctionRefusal = Get-Refusal { New-OmpAdapterComponentBundle -RepositoryRoot $JunctionRepository -Destination $JunctionDestination }
  Assert-True ($JunctionRefusal -clike "*crosses a Windows reparse point*") "a junctioned subtree inside an allowlisted directory must be refused"
  Assert-True ($JunctionRefusal -clike "*$LinkedSubtree*") "a linked-source refusal must name the reparse point"
  Assert-True (-not (Test-Path -LiteralPath $JunctionDestination)) "a refused component must stage nothing at all"
  Assert-True ((Get-Refusal { New-OmpAdapterComponentBundle -RepositoryRoot $JunctionRepository -Destination $JunctionDestination }) -ceq $JunctionRefusal) "the same linked source must be refused deterministically"

  $LinkedRootRepository = Join-Path $Sandbox "repo-junction-root"
  New-SandboxRepository -Root $LinkedRootRepository -Version "7.8.9" | Out-Null
  $RealAdapter = Join-Path $LinkedRootRepository "adapters/omp"
  $MovedAdapter = Join-Path $Sandbox "moved-adapter-source"
  Move-Item -LiteralPath $RealAdapter -Destination $MovedAdapter
  New-Item -ItemType Junction -Path $RealAdapter -Target $MovedAdapter | Out-Null
  Assert-True ((Get-Refusal { New-OmpAdapterComponentBundle -RepositoryRoot $LinkedRootRepository -Destination (Join-Path $Sandbox "bundle-junction-root") }) -clike "*OMP adapter component source root crosses a Windows reparse point*") "an adapters/omp that is a junction must be refused"

  if ($SymbolicLinksSupported) {
    $LinkedFileRepository = Join-Path $Sandbox "repo-symlinked-file"
    New-SandboxRepository -Root $LinkedFileRepository -Version "7.8.9" | Out-Null
    $AllowlistedEntry = Join-Path $LinkedFileRepository "adapters/omp/index.ts"
    Remove-Item -LiteralPath $AllowlistedEntry -Force
    New-Item -ItemType SymbolicLink -Path $AllowlistedEntry -Target (Join-Path $External "smuggled.ts") | Out-Null
    Assert-True ((Get-Refusal { New-OmpAdapterComponentBundle -RepositoryRoot $LinkedFileRepository -Destination (Join-Path $Sandbox "bundle-symlinked-file") }) -clike "*OMP adapter component source 'index.ts' crosses a Windows reparse point*") "a symlinked allowlisted source file must be refused"

    $LinkedProofRepository = Join-Path $Sandbox "repo-symlinked-proof"
    New-SandboxRepository -Root $LinkedProofRepository -Version "7.8.9" | Out-Null
    New-Item -ItemType SymbolicLink -Path (Join-Path $LinkedProofRepository "adapters/omp/house-proof/smuggled.ts") -Target (Join-Path $External "smuggled.ts") | Out-Null
    $ProofRefusal = Get-Refusal { New-OmpAdapterComponentBundle -RepositoryRoot $LinkedProofRepository -Destination (Join-Path $Sandbox "bundle-symlinked-proof") }
    Assert-True ($ProofRefusal -clike "*crosses a Windows reparse point*") "a symlinked file discovered under an allowlisted directory must be refused"
    Assert-True ($ProofRefusal -clike "*house-proof/smuggled.ts*") "the refusal must name the smuggled artifact path"
  }

  Write-Host "omp-adapter component contract: all assertions passed"
} finally {
  Remove-Item -LiteralPath $Sandbox -Recurse -Force -ErrorAction SilentlyContinue
}
