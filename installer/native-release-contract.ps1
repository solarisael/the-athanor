Set-StrictMode -Version Latest

# Shared contract for the native Windows release pipeline: the single product
# version authority, stable command/toolchain identity, the early toolchain
# refusal, and the one stage timing report every stage owner appends to.

function Get-NativeReleaseStageNames {
  return @(
    "toolchain-preflight",
    "download-verification",
    "dependency-preparation",
    "cargo-build",
    "payload-materialization",
    "godot-import",
    "manifest-hashing",
    "output-copy",
    "inno-packaging"
  )
}

function Assert-NativeReleaseProductVersion {
  param(
    [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Version,
    [Parameter(Mandatory = $true)][string]$Description
  )

  # The Athanor product-version grammar, deliberately not SemVer: retained
  # product lineage carries a fourth numeric revision (0.9.7.1, 0.9.7.2), so a
  # release version is three or four numeric components with at most one
  # '-' prerelease or '+' build suffix whose first byte is ASCII alphanumeric
  # (0.9.6.2-rc2). The installed release identity rule also holds: printable
  # ASCII only, never a '..' sequence, never more than 128 bytes.
  if ($Version -cnotmatch '^[0-9]+(\.[0-9]+){2,3}([-+][0-9A-Za-z][0-9A-Za-z-]*(\.[0-9A-Za-z-]+)*)?$') {
    throw "$Description is not an Athanor product version of three or four numeric components with an optional -prerelease or +build suffix: '$Version'"
  }
  if ($Version.Length -gt 128) {
    throw "$Description exceeds the 128-byte release identity limit: '$Version'"
  }
}

function Get-NativeReleaseVersion {
  param(
    [Parameter(Mandatory = $true)][string]$RepositoryRoot,
    [string]$Requested = ""
  )

  $ManifestPath = Join-Path $RepositoryRoot "package.json"
  if (-not (Test-Path $ManifestPath -PathType Leaf)) {
    throw "native release version authority is missing: $ManifestPath"
  }
  try {
    $Manifest = Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json
  } catch {
    throw "native release version authority is unreadable: $ManifestPath"
  }
  if (-not $Manifest.PSObject.Properties.Match("version").Count) {
    throw "native release version authority declares no version: $ManifestPath"
  }
  $Authority = ([string]$Manifest.version).Trim()
  if ([string]::IsNullOrWhiteSpace($Authority)) {
    throw "native release version authority declares an empty version: $ManifestPath"
  }
  Assert-NativeReleaseProductVersion -Version $Authority -Description "native release version authority $ManifestPath"
  if (-not [string]::IsNullOrWhiteSpace($Requested)) {
    $Normalized = $Requested.Trim()
    # Shape before equality: a malformed request must be refused as malformed,
    # never compared and never carried into a build or a package name.
    Assert-NativeReleaseProductVersion -Version $Normalized -Description "requested release version"
    if ($Normalized -cne $Authority) {
      throw "requested release version '$Normalized' does not match the root package.json authority '$Authority'"
    }
  }
  return $Authority
}

function Get-NativeReleaseTimingReportPath {
  param([Parameter(Mandatory = $true)][string]$OutDir)

  return (Join-Path $OutDir "native-release-timings.jsonl")
}

function Test-NativeReleaseReparsePoint {
  param([Parameter(Mandatory = $true)][IO.FileSystemInfo]$Item)

  return ([int]($Item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)
}

function Get-NativeReleaseTrustedPathRefusal {
  param(
    [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Path,
    [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Root,
    [Parameter(Mandatory = $true)][string]$Description,
    [switch]$Directory,
    [switch]$AllowLinkedLeaf
  )

  # A trusted build input is a regular file that physically lives under its
  # declared root. Lexical containment is not enough: a symlink, a junction, or
  # any other Windows reparse point on the owned chain redirects the bytes
  # outside the root while the path still reads as inside it. The whole chain
  # from the input up through the declared root is checked, so a redirected
  # ancestor is refused as loudly as a redirected file.
  if ([string]::IsNullOrWhiteSpace($Path)) { return "$Description has no path" }
  if ([string]::IsNullOrWhiteSpace($Root)) { return "$Description has no declared root" }
  $Separators = [char[]]@([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
  $FullRoot = [IO.Path]::GetFullPath($Root).TrimEnd($Separators)
  $FullPath = [IO.Path]::GetFullPath($Path).TrimEnd($Separators)
  if (-not $FullPath.StartsWith($FullRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    return "$Description resolved outside its declared root ${FullRoot}: $FullPath"
  }
  $Item = Get-Item -LiteralPath $FullPath -Force -ErrorAction SilentlyContinue
  if ($null -eq $Item) { return "$Description does not exist: $FullPath" }
  if ($Directory.IsPresent -ne ($Item -is [IO.DirectoryInfo])) {
    $Expected = if ($Directory.IsPresent) { "directory" } else { "regular file" }
    return "$Description is not a ${Expected}: $FullPath"
  }
  $Cursor = $Item
  if ($AllowLinkedLeaf.IsPresent) {
    # One deliberate exemption, for rustup's own installation shape: rustup
    # publishes cargo and rustc in <cargo home>/bin as links to rustup.exe
    # (symlinks with Developer Mode, hardlinks without). Such a proxy is name
    # resolution evidence, never a build input: nothing hashes it and nothing
    # executes it, because the build runs the selected pinned toolchain binary.
    # Its owned directory chain is still required to be physical.
    $Cursor = if ($Item -is [IO.DirectoryInfo]) { $Item.Parent } else { $Item.Directory }
  }
  while ($null -ne $Cursor) {
    if (Test-NativeReleaseReparsePoint -Item $Cursor) {
      return "$Description crosses a Windows reparse point (symlink or junction): $($Cursor.FullName.TrimEnd($Separators))"
    }
    if ($Cursor.FullName.TrimEnd($Separators).Equals($FullRoot, [StringComparison]::OrdinalIgnoreCase)) { return "" }
    $Cursor = if ($Cursor -is [IO.DirectoryInfo]) { $Cursor.Parent } else { $Cursor.Directory }
  }
  return "$Description left its declared root $FullRoot before it could be verified: $FullPath"
}

function Assert-NativeReleaseTrustedPath {
  param(
    [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Path,
    [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Root,
    [Parameter(Mandatory = $true)][string]$Description,
    [switch]$Directory,
    [switch]$AllowLinkedLeaf
  )

  $Refusal = Get-NativeReleaseTrustedPathRefusal -Path $Path -Root $Root -Description $Description `
    -Directory:$Directory -AllowLinkedLeaf:$AllowLinkedLeaf
  if (-not [string]::IsNullOrEmpty($Refusal)) { throw $Refusal }
  return [IO.Path]::GetFullPath($Path).TrimEnd([char[]]@([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar))
}

function Get-NativeReleaseFileIdentity {
  param([Parameter(Mandatory = $true)][string]$Path)

  $Item = Get-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
  if ($null -eq $Item -or $Item -is [IO.DirectoryInfo]) {
    throw "native release file identity requires an existing file: $Path"
  }
  if (Test-NativeReleaseReparsePoint -Item $Item) {
    throw "native release file identity refuses a Windows reparse point: $($Item.FullName)"
  }
  $Hash = (Get-FileHash -LiteralPath $Item.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
  return "sha256=$Hash;size=$($Item.Length)"
}

function Get-NativeReleaseCommandIdentity {
  param(
    [Parameter(Mandatory = $true)][string]$Name,
    [string]$Path = "",
    [string[]]$VersionArguments = @()
  )

  # An explicit -Path takes the identity of exactly those bytes; without one the
  # command is resolved from PATH.
  $Executable = $Path
  if ([string]::IsNullOrWhiteSpace($Executable)) {
    $Command = Get-Command $Name -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($null -eq $Command) {
      throw "required native build command is unavailable: $Name"
    }
    $Executable = $Command.Source
  }
  $File = Get-NativeReleaseFileIdentity $Executable
  $Version = ""
  if ($VersionArguments.Count -gt 0) {
    $global:LASTEXITCODE = 0
    $Probe = (& $Executable @VersionArguments 2>&1 | Out-String)
    if ($global:LASTEXITCODE -ne 0) {
      throw "required native build command failed its identity probe: $Name (exit $($global:LASTEXITCODE))"
    }
    $FirstLine = ($Probe -split "`r?`n" | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -First 1)
    if ($null -ne $FirstLine) { $Version = ([string]$FirstLine).Trim() }
  }
  # The key deliberately excludes the resolved path so cache keys stay stable
  # across machines and runner working directories.
  return [pscustomobject][ordered]@{
    name = $Name
    path = $Executable
    file = $File
    version = $Version
    key = "$Name|$File|$Version"
  }
}

function Get-NativeReleaseRustChannel {
  param([Parameter(Mandatory = $true)][string]$RepositoryRoot)

  # rust-toolchain.toml is the one Rust pin authority. A floating channel is
  # refused: a release must name the exact toolchain its bytes came from.
  $PinPath = Join-Path $RepositoryRoot "rust-toolchain.toml"
  if (-not (Test-Path -LiteralPath $PinPath -PathType Leaf)) {
    throw "native release Rust toolchain pin is missing: $PinPath"
  }
  $Match = [regex]::Match((Get-Content -LiteralPath $PinPath -Raw), '(?m)^\s*channel\s*=\s*"([^"]+)"\s*$')
  if (-not $Match.Success) {
    throw "native release Rust toolchain pin declares no channel: $PinPath"
  }
  $Channel = $Match.Groups[1].Value.Trim()
  if ($Channel -cnotmatch '^[0-9]+\.[0-9]+\.[0-9]+$') {
    throw "native release requires an exactly pinned Rust channel; $PinPath declares '$Channel'"
  }
  return $Channel
}

function Get-NativeReleaseRustupRoots {
  # rustup is the supported Rust authority: cargo and rustc must be its proxies
  # under <cargo home>/bin, and every selected toolchain binary must live under
  # <rustup home>/toolchains. Both roots follow rustup's own environment.
  $CargoHome = if ([string]::IsNullOrWhiteSpace($env:CARGO_HOME)) {
    if ([string]::IsNullOrWhiteSpace($env:USERPROFILE)) { "" } else { Join-Path ([string]$env:USERPROFILE).Trim() ".cargo" }
  } else { ([string]$env:CARGO_HOME).Trim() }
  $RustupHome = if ([string]::IsNullOrWhiteSpace($env:RUSTUP_HOME)) {
    if ([string]::IsNullOrWhiteSpace($env:USERPROFILE)) { "" } else { Join-Path ([string]$env:USERPROFILE).Trim() ".rustup" }
  } else { ([string]$env:RUSTUP_HOME).Trim() }
  return [pscustomobject][ordered]@{
    proxyRoot = if ([string]::IsNullOrWhiteSpace($CargoHome)) { "" } else { (Join-Path $CargoHome "bin") }
    toolchainsRoot = if ([string]::IsNullOrWhiteSpace($RustupHome)) { "" } else { (Join-Path $RustupHome "toolchains") }
  }
}

function Assert-NativeReleaseToolchain {
  param([Parameter(Mandatory = $true)][string]$RepositoryRoot)

  $Specification = @(
    @{ Name = "rustup"; VersionArguments = @("--version") },
    @{ Name = "cargo"; VersionArguments = @("--version") },
    @{ Name = "rustc"; VersionArguments = @("--version") },
    @{ Name = "cl"; VersionArguments = @() },
    @{ Name = "link"; VersionArguments = @() },
    @{ Name = "nmake"; VersionArguments = @() }
  )

  $ResolvedCommands = @{}
  $Failures = @()
  foreach ($Entry in $Specification) {
    $Command = Get-Command $Entry.Name -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($null -eq $Command) {
      $Failures += "required native build command is unavailable: $($Entry.Name)"
    } else {
      $ResolvedCommands[$Entry.Name] = $Command.Source
    }
  }

  $VisualCppVersion = if ([string]::IsNullOrWhiteSpace($env:VCToolsVersion)) { "" } else { ([string]$env:VCToolsVersion).Trim() }
  if ([string]::IsNullOrWhiteSpace($VisualCppVersion)) {
    $Failures += "required Visual Studio toolset identity is missing: VCToolsVersion is not set"
  }
  $VisualCppRoot = if ([string]::IsNullOrWhiteSpace($env:VCToolsInstallDir)) { "" } else { ([string]$env:VCToolsInstallDir).Trim() }
  if ([string]::IsNullOrWhiteSpace($VisualCppRoot) -or -not (Test-Path $VisualCppRoot -PathType Container)) {
    $Failures += "required Visual Studio toolset root is missing: VCToolsInstallDir is not an existing directory"
  }
  $WindowsSdkVersion = if ([string]::IsNullOrWhiteSpace($env:WindowsSDKVersion)) { "" } else { ([string]$env:WindowsSDKVersion).Trim().TrimEnd("\", "/") }
  if ([string]::IsNullOrWhiteSpace($WindowsSdkVersion)) {
    $Failures += "required Windows SDK identity is missing: WindowsSDKVersion is not set"
  }
  $TargetArchitecture = if ([string]::IsNullOrWhiteSpace($env:VSCMD_ARG_TGT_ARCH)) { "" } else { ([string]$env:VSCMD_ARG_TGT_ARCH).Trim().ToLowerInvariant() }
  if ($TargetArchitecture -ne "x64") {
    $Reported = if ([string]::IsNullOrWhiteSpace($TargetArchitecture)) { "not set" } else { $TargetArchitecture }
    $Failures += "native release requires an x64 Visual Studio target architecture; VSCMD_ARG_TGT_ARCH is $Reported"
  }
  if (-not [string]::IsNullOrWhiteSpace($VisualCppRoot) -and (Test-Path $VisualCppRoot -PathType Container)) {
    foreach ($Name in @("cl", "link", "nmake")) {
      if ($ResolvedCommands.ContainsKey($Name)) {
        $Refusal = Get-NativeReleaseTrustedPathRefusal -Path $ResolvedCommands[$Name] -Root $VisualCppRoot `
          -Description "Visual Studio command '$Name' under VCToolsInstallDir"
        if (-not [string]::IsNullOrEmpty($Refusal)) { $Failures += $Refusal }
      }
    }
  }

  $RustChannel = ""
  try {
    $RustChannel = Get-NativeReleaseRustChannel -RepositoryRoot $RepositoryRoot
  } catch {
    $Failures += [string]$_.Exception.Message
  }
  $RustupRoots = Get-NativeReleaseRustupRoots
  if ([string]::IsNullOrWhiteSpace($RustupRoots.proxyRoot) -or -not (Test-Path -LiteralPath $RustupRoots.proxyRoot -PathType Container)) {
    $Failures += "required rustup proxy root is missing: CARGO_HOME or USERPROFILE must name an existing <cargo home>/bin directory"
  } else {
    foreach ($Name in @("rustup", "cargo", "rustc")) {
      if ($ResolvedCommands.ContainsKey($Name)) {
        # rustup itself is executed here, so it must be real bytes; its cargo and
        # rustc proxies may be the links rustup installed them as.
        $Refusal = Get-NativeReleaseTrustedPathRefusal -Path $ResolvedCommands[$Name] -Root $RustupRoots.proxyRoot `
          -Description "rustup proxy command '$Name'" -AllowLinkedLeaf:($Name -cne "rustup")
        if (-not [string]::IsNullOrEmpty($Refusal)) { $Failures += $Refusal }
      }
    }
  }
  if ([string]::IsNullOrWhiteSpace($RustupRoots.toolchainsRoot) -or -not (Test-Path -LiteralPath $RustupRoots.toolchainsRoot -PathType Container)) {
    $Failures += "required rustup toolchains root is missing: RUSTUP_HOME or USERPROFILE must name an existing <rustup home>/toolchains directory"
  }

  if ($Failures.Count -gt 0) {
    throw ("native toolchain preflight refused this environment:" + [Environment]::NewLine + ($Failures -join [Environment]::NewLine))
  }

  # Every check above is cheap: no build tool has run yet, so a refused
  # environment never pays for a version probe. From here the pinned toolchain
  # is asked, once, which binaries it actually selects.
  $Selected = [ordered]@{}
  foreach ($Name in @("cargo", "rustc")) {
    $global:LASTEXITCODE = 0
    $Answer = (& $ResolvedCommands["rustup"] "which" "--toolchain" $RustChannel $Name 2>&1 | Out-String)
    if ($global:LASTEXITCODE -ne 0) {
      throw "rustup could not select $Name from the pinned toolchain ${RustChannel}: $($Answer.Trim())"
    }
    $Line = ($Answer -split "`r?`n" | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -First 1)
    if ($null -eq $Line) {
      throw "rustup reported no $Name path for the pinned toolchain $RustChannel"
    }
    $Selected[$Name] = Assert-NativeReleaseTrustedPath -Path ([string]$Line).Trim() -Root $RustupRoots.toolchainsRoot `
      -Description "pinned Rust $RustChannel $Name"
  }

  # cargo and rustc take the identity of the selected pinned binaries, never of
  # whichever proxy or shadow happened to sit first on PATH.
  $Identities = @($Specification | ForEach-Object {
    $Name = [string]$_.Name
    $Executable = if ($Selected.Contains($Name)) { [string]$Selected[$Name] } else { [string]$ResolvedCommands[$Name] }
    Get-NativeReleaseCommandIdentity -Name $Name -Path $Executable -VersionArguments $_.VersionArguments
  })
  foreach ($Name in @("cargo", "rustc")) {
    $Reported = [string](($Identities | Where-Object { $_.name -ceq $Name }).version)
    if (-not $Reported.StartsWith("$Name $RustChannel", [StringComparison]::Ordinal)) {
      throw "the selected $Name is not the pinned Rust toolchain $RustChannel; it reports '$Reported'"
    }
  }
  Write-Host "toolchain preflight accepted: vctools $VisualCppVersion, winsdk $WindowsSdkVersion, target $TargetArchitecture, rust $RustChannel"
  return [pscustomobject][ordered]@{
    format = 2
    commands = $Identities
    visualCppToolsVersion = $VisualCppVersion
    visualCppToolsRoot = $VisualCppRoot
    windowsSdkVersion = $WindowsSdkVersion
    targetArchitecture = $TargetArchitecture
    rustChannel = $RustChannel
    rustupProxyRoot = $RustupRoots.proxyRoot
    cargoPath = [string]$Selected["cargo"]
    rustcPath = [string]$Selected["rustc"]
    cacheKeyMaterial = @(
      @($Identities | ForEach-Object { $_.key }) +
      @("vctools=$VisualCppVersion", "winsdk=$WindowsSdkVersion", "arch=$TargetArchitecture", "rust=$RustChannel")
    )
  }
}

function Add-NativeReleaseStageTiming {
  param(
    [Parameter(Mandatory = $true)][string]$Name,
    [Parameter(Mandatory = $true)][string]$OutDir,
    [Parameter(Mandatory = $true)][ValidateSet("success", "failure")][string]$Status,
    [Parameter(Mandatory = $true)][DateTime]$StartedAt,
    [Parameter(Mandatory = $true)][DateTime]$CompletedAt,
    [Parameter(Mandatory = $true)][long]$ElapsedMilliseconds,
    [string]$ErrorMessage = ""
  )

  New-Item $OutDir -ItemType Directory -Force | Out-Null
  $Record = [ordered]@{
    schema = "athanor.native-release.stage-timing"
    schemaVersion = 1
    stage = $Name
    status = $Status
    startedAt = $StartedAt.ToUniversalTime().ToString("o")
    completedAt = $CompletedAt.ToUniversalTime().ToString("o")
    elapsedMs = $ElapsedMilliseconds
    error = $ErrorMessage
  }
  $Line = [pscustomobject]$Record | ConvertTo-Json -Depth 3 -Compress
  Add-Content -LiteralPath (Get-NativeReleaseTimingReportPath $OutDir) -Value $Line -Encoding utf8NoBOM
}

function Invoke-NativeReleaseStage {
  param(
    [Parameter(Mandatory = $true)][string]$Name,
    [Parameter(Mandatory = $true)][string]$OutDir,
    [Parameter(Mandatory = $true)][scriptblock]$Action
  )

  if ((Get-NativeReleaseStageNames) -cnotcontains $Name) {
    throw "unknown native release stage: $Name"
  }
  $StartedAt = [DateTime]::UtcNow
  $Stopwatch = [Diagnostics.Stopwatch]::StartNew()
  try {
    & $Action
    $Stopwatch.Stop()
    Add-NativeReleaseStageTiming -Name $Name -OutDir $OutDir -Status "success" -StartedAt $StartedAt `
      -CompletedAt ([DateTime]::UtcNow) -ElapsedMilliseconds ([long][Math]::Round($Stopwatch.Elapsed.TotalMilliseconds))
  } catch {
    $Stopwatch.Stop()
    Add-NativeReleaseStageTiming -Name $Name -OutDir $OutDir -Status "failure" -StartedAt $StartedAt `
      -CompletedAt ([DateTime]::UtcNow) -ElapsedMilliseconds ([long][Math]::Round($Stopwatch.Elapsed.TotalMilliseconds)) `
      -ErrorMessage ([string]$_.Exception.Message)
    throw
  }
}
