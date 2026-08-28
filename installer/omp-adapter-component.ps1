Set-StrictMode -Version Latest

# The one deterministic builder for the installed OMP adapter component bundle.
# Both the native release payload (fallback bundle) and adapter-only deployment
# stage through here, so a component built from identical sources always carries
# identical manifest bytes and the same releaseId.
#
# Authorities: adapters/omp/package.json owns the adapter semantic version and
# installer/dependencies.json owns the four compatibility fields the native
# product manifest publishes. Nothing here writes into an installed root: the
# Rust manager owns every installed write.

# The shared native release contract owns the one physical-root rule every
# trusted build input answers to; this builder must not grow a second one.
. (Join-Path $PSScriptRoot "native-release-contract.ps1")

function Get-OmpAdapterComponentName {
  return "omp-adapter"
}

function Get-OmpAdapterComponentFormat {
  return 1
}

function Get-OmpAdapterComponentRuntimeAllowlist {
  # The deliberate runtime allowlist: adapter entry points and their proof
  # rooms. installed-loader.ts is absent on purpose; it ships as the stable
  # product-owned bin/athanor-omp-loader.ts.
  return [ordered]@{
    files = @(
      "index.ts", "hygiene.ts", "athanor-root.ts", "discovery.ts", "giga.ts",
      "kitten-lineage.ts", "rust-transport.ts", "package.json", "bunfig.toml",
      "README.md", "LICENSE", "NOTICE"
    )
    directories = @("house-proof", "starter-room")
  }
}

function Get-OmpAdapterComponentField {
  param(
    [Parameter(Mandatory = $true)][AllowNull()][object]$Source,
    [Parameter(Mandatory = $true)][string]$Name,
    [Parameter(Mandatory = $true)][string]$Description
  )

  if ($null -eq $Source) { throw "$Description is missing" }
  if ($Source -is [Collections.IDictionary]) {
    if (-not $Source.Contains($Name)) { throw "$Description declares no '$Name'" }
    return $Source[$Name]
  }
  $Property = $Source.PSObject.Properties[$Name]
  if ($null -eq $Property) { throw "$Description declares no '$Name'" }
  return $Property.Value
}

function Get-OmpAdapterComponentAsciiFold {
  param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Value)

  # The installed Rust validator folds artifact paths with to_ascii_lowercase
  # before duplicate detection; culture-aware lowering would not match it.
  $Folded = [Text.StringBuilder]::new($Value.Length)
  foreach ($Character in $Value.ToCharArray()) {
    if ($Character -ge [char]'A' -and $Character -le [char]'Z') {
      [void]$Folded.Append([char]([int]$Character + 32))
    } else {
      [void]$Folded.Append($Character)
    }
  }
  return $Folded.ToString()
}

function Assert-OmpAdapterComponentArtifactPath {
  param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Path)

  # Exactly the installed Rust safe_artifact_path rule, plus the producer-only
  # ASCII rule that keeps ordinal ordering identical in PowerShell (UTF-16),
  # Rust (UTF-8 bytes), and the loader (UTF-16).
  if ([string]::IsNullOrEmpty($Path)) {
    throw "OMP adapter component artifact path must not be empty"
  }
  if ($Path.StartsWith("/")) {
    throw "OMP adapter component artifact path must be relative: $Path"
  }
  if ($Path.Contains("\")) {
    throw "OMP adapter component artifact path must use '/' separators: $Path"
  }
  if ($Path.Contains(":")) {
    throw "OMP adapter component artifact path must not contain ':': $Path"
  }
  foreach ($Segment in $Path.Split("/")) {
    if ([string]::IsNullOrEmpty($Segment)) {
      throw "OMP adapter component artifact path must not contain an empty segment: $Path"
    }
    if ($Segment -eq "." -or $Segment -eq "..") {
      throw "OMP adapter component artifact path must not contain '.' or '..': $Path"
    }
  }
  foreach ($Character in $Path.ToCharArray()) {
    if ([int]$Character -lt 0x20 -or [int]$Character -gt 0x7E) {
      throw "OMP adapter component artifact path must be printable ASCII: $Path"
    }
  }
}

function Assert-OmpAdapterComponentVersion {
  param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Version)

  # Exactly the installed Rust safe_version rule: 1..128 bytes of ASCII
  # alphanumerics plus '.', '-', '+', and never a '..' sequence.
  if ($Version -cnotmatch '^[0-9A-Za-z.+-]{1,128}$' -or $Version.Contains("..")) {
    throw "OMP adapter component version is not a safe release identity part: '$Version'"
  }
}

function Get-OmpAdapterComponentNormalizedCompatibility {
  param([Parameter(Mandatory = $true)][AllowNull()][object]$Compatibility)

  $Normalized = [ordered]@{}
  foreach ($Field in @("hostApi", "substrateApi", "deliveryApi", "schemaVersion")) {
    $Value = [string](Get-OmpAdapterComponentField -Source $Compatibility -Name $Field -Description "OMP adapter component compatibility")
    $Parsed = [uint64]0
    if ($Value -notmatch '^(0|[1-9][0-9]*)$' -or -not [uint64]::TryParse($Value, [ref]$Parsed) -or $Parsed -gt [uint32]::MaxValue) {
      throw "OMP adapter component compatibility field '$Field' must be an unsigned 32-bit integer: '$Value'"
    }
    $Normalized[$Field] = [uint32]$Parsed
  }
  return $Normalized
}

function Get-OmpAdapterComponentNormalizedArtifacts {
  param([Parameter(Mandatory = $true)][AllowEmptyCollection()][object[]]$Artifacts)

  $Normalized = [Collections.Generic.List[object]]::new()
  $Seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
  $Previous = $null
  foreach ($Artifact in $Artifacts) {
    $Path = [string](Get-OmpAdapterComponentField -Source $Artifact -Name "path" -Description "OMP adapter component artifact")
    Assert-OmpAdapterComponentArtifactPath -Path $Path
    if (-not $Seen.Add((Get-OmpAdapterComponentAsciiFold -Value $Path))) {
      throw "OMP adapter component artifact path is duplicated: $Path"
    }
    if ($null -ne $Previous -and [string]::CompareOrdinal($Previous, $Path) -ge 0) {
      throw "OMP adapter component artifacts must be strictly ordinal ascending: '$Path' follows '$Previous'"
    }
    $Previous = $Path
    $Hash = [string](Get-OmpAdapterComponentField -Source $Artifact -Name "sha256" -Description "OMP adapter component artifact '$Path'")
    if ($Hash -cnotmatch '^[0-9a-f]{64}$') {
      throw "OMP adapter component artifact '$Path' must carry a lowercase full SHA-256 hash: '$Hash'"
    }
    $Size = [string](Get-OmpAdapterComponentField -Source $Artifact -Name "size" -Description "OMP adapter component artifact '$Path'")
    if ($Size -notmatch '^(0|[1-9][0-9]*)$') {
      throw "OMP adapter component artifact '$Path' must carry a nonnegative integer size: '$Size'"
    }
    $Normalized.Add([ordered]@{ path = $Path; sha256 = $Hash; size = [long]$Size })
  }
  if ($Normalized.Count -eq 0) {
    throw "OMP adapter component must declare at least one artifact"
  }
  return $Normalized.ToArray()
}

function Get-OmpAdapterComponentIdentity {
  param(
    [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Version,
    [Parameter(Mandatory = $true)][AllowNull()][object]$Compatibility,
    [Parameter(Mandatory = $true)][AllowEmptyCollection()][object[]]$Artifacts
  )

  Assert-OmpAdapterComponentVersion -Version $Version
  $NormalizedCompatibility = Get-OmpAdapterComponentNormalizedCompatibility -Compatibility $Compatibility
  $NormalizedArtifacts = Get-OmpAdapterComponentNormalizedArtifacts -Artifacts $Artifacts

  $Lines = [Collections.Generic.List[string]]::new()
  $Lines.Add("format=$(Get-OmpAdapterComponentFormat)")
  $Lines.Add("component=$(Get-OmpAdapterComponentName)")
  $Lines.Add("version=$Version")
  $Lines.Add("hostApi=$($NormalizedCompatibility.hostApi)")
  $Lines.Add("substrateApi=$($NormalizedCompatibility.substrateApi)")
  $Lines.Add("deliveryApi=$($NormalizedCompatibility.deliveryApi)")
  $Lines.Add("schemaVersion=$($NormalizedCompatibility.schemaVersion)")
  foreach ($Artifact in $NormalizedArtifacts) {
    $Lines.Add("artifact=$($Artifact.path)`t$($Artifact.sha256)`t$($Artifact.size)")
  }

  $Canonical = ($Lines -join "`n") + "`n"
  $Bytes = [Text.UTF8Encoding]::new($false).GetBytes($Canonical)
  $Sha256 = [Security.Cryptography.SHA256]::Create()
  try {
    $Digest = $Sha256.ComputeHash($Bytes)
  } finally {
    $Sha256.Dispose()
  }
  $Fingerprint = [BitConverter]::ToString($Digest).Replace("-", "").ToLowerInvariant()
  return [ordered]@{
    CanonicalIdentity = $Canonical
    Fingerprint       = $Fingerprint
    ReleaseId         = "$Version-$Fingerprint"
    Compatibility     = $NormalizedCompatibility
    Artifacts         = $NormalizedArtifacts
  }
}

function New-OmpAdapterComponentManifest {
  param(
    [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Version,
    [Parameter(Mandatory = $true)][AllowNull()][object]$Compatibility,
    [Parameter(Mandatory = $true)][AllowEmptyCollection()][object[]]$Artifacts
  )

  $Identity = Get-OmpAdapterComponentIdentity -Version $Version -Compatibility $Compatibility -Artifacts $Artifacts
  return [ordered]@{
    format        = Get-OmpAdapterComponentFormat
    component     = Get-OmpAdapterComponentName
    version       = $Version
    releaseId     = $Identity.ReleaseId
    compatibility = $Identity.Compatibility
    artifacts     = @($Identity.Artifacts)
  }
}

function Get-OmpAdapterComponentVersion {
  param([Parameter(Mandatory = $true)][string]$RepositoryRoot)

  $ManifestPath = Join-Path $RepositoryRoot "adapters/omp/package.json"
  if (-not (Test-Path -LiteralPath $ManifestPath -PathType Leaf)) {
    throw "OMP adapter version authority is missing: $ManifestPath"
  }
  try {
    $Manifest = Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json
  } catch {
    throw "OMP adapter version authority is unreadable: $ManifestPath"
  }
  $Version = ([string](Get-OmpAdapterComponentField -Source $Manifest -Name "version" -Description "OMP adapter version authority $ManifestPath")).Trim()
  Assert-OmpAdapterComponentVersion -Version $Version
  return $Version
}

function Get-OmpAdapterComponentCompatibility {
  param([Parameter(Mandatory = $true)][string]$RepositoryRoot)

  # One compatibility authority for the product manifest and the component:
  # the release payload already publishes these four fields from this file.
  $AuthorityPath = Join-Path $RepositoryRoot "installer/dependencies.json"
  if (-not (Test-Path -LiteralPath $AuthorityPath -PathType Leaf)) {
    throw "OMP adapter compatibility authority is missing: $AuthorityPath"
  }
  try {
    $Authority = Get-Content -LiteralPath $AuthorityPath -Raw | ConvertFrom-Json
  } catch {
    throw "OMP adapter compatibility authority is unreadable: $AuthorityPath"
  }
  $Description = "OMP adapter compatibility authority $AuthorityPath"
  $Components = Get-OmpAdapterComponentField -Source $Authority -Name "components" -Description $Description
  return Get-OmpAdapterComponentNormalizedCompatibility -Compatibility ([ordered]@{
      hostApi       = Get-OmpAdapterComponentField -Source $Components -Name "hostApi" -Description $Description
      substrateApi  = Get-OmpAdapterComponentField -Source $Components -Name "substrateApi" -Description $Description
      deliveryApi   = Get-OmpAdapterComponentField -Source $Components -Name "deliveryApi" -Description $Description
      schemaVersion = Get-OmpAdapterComponentField -Source $Authority -Name "schemaVersion" -Description $Description
    })
}

function Get-OmpAdapterComponentSourceMap {
  param([Parameter(Mandatory = $true)][string]$AdapterRoot)

  if (-not (Test-Path -LiteralPath $AdapterRoot -PathType Container)) {
    throw "OMP adapter component source root is missing: $AdapterRoot"
  }
  $Root = (Resolve-Path -LiteralPath $AdapterRoot).Path
  $Allowlist = Get-OmpAdapterComponentRuntimeAllowlist
  $Sources = [ordered]@{}
  foreach ($Name in $Allowlist.files) {
    $Source = Join-Path $Root $Name
    if (-not (Test-Path -LiteralPath $Source -PathType Leaf)) {
      throw "OMP adapter component source is missing: $Source"
    }
    # A component ships bytes that physically live in the adapter tree: a
    # symlink or junction anywhere on the owned chain is refused, not followed.
    $Sources[$Name] = Assert-NativeReleaseTrustedPath -Path $Source -Root $Root `
      -Description "OMP adapter component source '$Name'"
  }
  foreach ($Directory in $Allowlist.directories) {
    $DirectoryRoot = Join-Path $Root $Directory
    if (-not (Test-Path -LiteralPath $DirectoryRoot -PathType Container)) {
      throw "OMP adapter component source directory is missing: $DirectoryRoot"
    }
    Assert-NativeReleaseTrustedPath -Path $DirectoryRoot -Root $Root -Directory `
      -Description "OMP adapter component source directory '$Directory'" | Out-Null
    # Recursion enumerates directories as well as files on purpose: Get-ChildItem
    # never descends through a reparse point, so a linked subtree would
    # otherwise be skipped in silence instead of refused.
    foreach ($Entry in @(Get-ChildItem -LiteralPath $DirectoryRoot -Recurse -Force)) {
      $Relative = [IO.Path]::GetRelativePath($Root, $Entry.FullName).Replace("\", "/")
      if ($Entry -is [IO.DirectoryInfo]) {
        Assert-NativeReleaseTrustedPath -Path $Entry.FullName -Root $Root -Directory `
          -Description "OMP adapter component source directory '$Relative'" | Out-Null
      } else {
        $Sources[$Relative] = Assert-NativeReleaseTrustedPath -Path $Entry.FullName -Root $Root `
          -Description "OMP adapter component source '$Relative'"
      }
    }
  }
  return $Sources
}

function New-OmpAdapterComponentBundle {
  param(
    [Parameter(Mandatory = $true)][string]$RepositoryRoot,
    [Parameter(Mandatory = $true)][string]$Destination,
    [AllowNull()][object]$Compatibility = $null
  )

  if (-not (Test-Path -LiteralPath $RepositoryRoot -PathType Container)) {
    throw "OMP adapter component repository root is missing: $RepositoryRoot"
  }
  $Repository = (Resolve-Path -LiteralPath $RepositoryRoot).Path
  $Version = Get-OmpAdapterComponentVersion -RepositoryRoot $Repository
  $NormalizedCompatibility = if ($null -eq $Compatibility) {
    Get-OmpAdapterComponentCompatibility -RepositoryRoot $Repository
  } else {
    Get-OmpAdapterComponentNormalizedCompatibility -Compatibility $Compatibility
  }
  # adapters/omp itself must be a real directory inside the repository, never a
  # link that would redirect the whole component source tree.
  $AdapterRoot = Assert-NativeReleaseTrustedPath -Path (Join-Path $Repository "adapters/omp") -Root $Repository -Directory `
    -Description "OMP adapter component source root"
  $Sources = Get-OmpAdapterComponentSourceMap -AdapterRoot $AdapterRoot

  # A bundle root holds the manifest and its artifacts and nothing else, so a
  # dirty destination is refused instead of silently shipping stale bytes.
  if (Test-Path -LiteralPath $Destination) {
    if (-not (Test-Path -LiteralPath $Destination -PathType Container)) {
      throw "OMP adapter component destination is not a directory: $Destination"
    }
    if (@(Get-ChildItem -LiteralPath $Destination -Force).Count -gt 0) {
      throw "OMP adapter component destination must be empty: $Destination"
    }
  }
  New-Item $Destination -ItemType Directory -Force | Out-Null
  $Root = (Resolve-Path -LiteralPath $Destination).Path

  $Paths = [Collections.Generic.List[string]]::new()
  foreach ($Relative in $Sources.Keys) {
    Assert-OmpAdapterComponentArtifactPath -Path $Relative
    $Paths.Add($Relative)
  }
  $Paths.Sort([StringComparer]::Ordinal)

  $Artifacts = [Collections.Generic.List[object]]::new()
  foreach ($Relative in $Paths) {
    $Target = Join-Path $Root $Relative
    New-Item ([IO.Path]::GetDirectoryName($Target)) -ItemType Directory -Force | Out-Null
    Copy-Item -LiteralPath $Sources[$Relative] -Destination $Target -Force
    # Hash the staged bytes: the manifest describes what actually ships.
    $Artifacts.Add([ordered]@{
        path   = $Relative
        sha256 = (Get-FileHash -LiteralPath $Target -Algorithm SHA256).Hash.ToLowerInvariant()
        size   = (Get-Item -LiteralPath $Target).Length
      })
  }

  $Manifest = New-OmpAdapterComponentManifest -Version $Version -Compatibility $NormalizedCompatibility -Artifacts $Artifacts.ToArray()
  $ManifestPath = Join-Path $Root "component-manifest.json"
  Set-Content -LiteralPath $ManifestPath -Value ($Manifest | ConvertTo-Json -Depth 8) -Encoding utf8NoBOM

  return [ordered]@{
    Root          = $Root
    ManifestPath  = $ManifestPath
    Version       = $Version
    ReleaseId     = [string]$Manifest.releaseId
    Compatibility = $NormalizedCompatibility
    ArtifactCount = $Artifacts.Count
  }
}
