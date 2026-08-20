Set-StrictMode -Version Latest

# The completion marker every cache check reads; one authority so the reparse
# guard covers exactly the file Test-NativeBuildCache trusts.
$script:NativeBuildCacheMarkerFile = ".complete.json"

function Get-NativeBuildCacheKey {
  param([Parameter(Mandatory = $true)][string[]]$Parts)

  $Material = ($Parts | ForEach-Object { "$($_.Length):$_" }) -join "|"
  $Sha256 = [Security.Cryptography.SHA256]::Create()
  try {
    $Bytes = [Text.Encoding]::UTF8.GetBytes($Material)
    return -join ($Sha256.ComputeHash($Bytes) | ForEach-Object { $_.ToString("x2") })
  } finally {
    $Sha256.Dispose()
  }
}

function Test-NativeBuildCache {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$Key,
    [Parameter(Mandatory = $true)][string[]]$RequiredFiles
  )

  $MarkerPath = Join-Path $Path $script:NativeBuildCacheMarkerFile
  if (-not (Test-Path $MarkerPath -PathType Leaf)) { return $false }
  try {
    $Marker = Get-Content $MarkerPath -Raw | ConvertFrom-Json
  } catch {
    return $false
  }
  if ($Marker.format -ne 1 -or $Marker.key -cne $Key) { return $false }
  foreach ($Relative in $RequiredFiles) {
    if ([IO.Path]::IsPathRooted($Relative) -or $Relative -split "[\\/]" -contains "..") {
      throw "native build cache required paths must stay relative: $Relative"
    }
    if (-not (Test-Path (Join-Path $Path $Relative) -PathType Leaf)) { return $false }
  }
  return $true
}

function Complete-NativeBuildCache {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$Key,
    [Parameter(Mandatory = $true)][string[]]$RequiredFiles
  )

  foreach ($Relative in $RequiredFiles) {
    if ([IO.Path]::IsPathRooted($Relative) -or $Relative -split "[\\/]" -contains "..") {
      throw "native build cache required paths must stay relative: $Relative"
    }
    if (-not (Test-Path (Join-Path $Path $Relative) -PathType Leaf)) {
      throw "native build cache is incomplete: $Relative"
    }
  }
  $MarkerPath = Join-Path $Path $script:NativeBuildCacheMarkerFile
  $TemporaryMarker = "$MarkerPath.$PID.tmp"
  [ordered]@{
    format = 1
    key = $Key
    requiredFiles = $RequiredFiles
    completedAt = [DateTimeOffset]::UtcNow.ToString("o")
  } | ConvertTo-Json -Depth 3 | Set-Content $TemporaryMarker -Encoding utf8NoBOM
  Move-Item $TemporaryMarker $MarkerPath -Force
}

# Keyed cache directory names are the hex SHA-256 produced by
# Get-NativeBuildCacheKey; a build in flight parks its work in a sibling
# "<key>.pending-<pid>" directory. Nothing else in the cache root is ours to
# delete.
$script:NativeBuildCacheKeyPattern = "^[0-9a-f]{64}$"
$script:NativeBuildCachePrunablePattern = "^[0-9a-f]{64}(\.pending-[0-9]+)?$"
$script:NativeBuildCacheLockFile = ".operation.lock"

function Assert-NativeBuildCacheKey {
  param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Key)

  if ($Key -notmatch $script:NativeBuildCacheKeyPattern) {
    throw "native build cache key is not a safe directory name: $Key"
  }
}

function Assert-NativeBuildCacheLexicalPathIsReal {
  param([Parameter(Mandatory = $true)][string]$Path)

  # Walk the path exactly as it was written, before any resolution: a junction
  # or symlink among its own ancestors means the "cache root" we are about to
  # trust, and delete inside, can be anywhere on the machine.
  $Full = [IO.Path]::GetFullPath($Path)
  $PathRoot = [IO.Path]::GetPathRoot($Full)
  if ([string]::IsNullOrEmpty($PathRoot)) {
    throw "native build cache paths must be rooted: $Path"
  }
  $Walk = $PathRoot
  foreach ($Segment in ($Full.Substring($PathRoot.Length) -split "[\\/]" | Where-Object { $_ -ne "" })) {
    $Walk = Join-Path $Walk $Segment
    if (-not (Test-Path -LiteralPath $Walk)) { continue }
    if ([IO.File]::GetAttributes($Walk).HasFlag([IO.FileAttributes]::ReparsePoint)) {
      throw "refusing to trust a native build cache path through a reparse point: $Walk"
    }
  }
}

function Resolve-NativeBuildCacheRoot {
  param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Root)

  if ([string]::IsNullOrWhiteSpace($Root)) { throw "native build cache root is required" }
  Assert-NativeBuildCacheLexicalPathIsReal $Root
  if (-not (Test-Path -LiteralPath $Root -PathType Container)) {
    throw "native build cache root does not exist: $Root"
  }
  $Separators = @([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
  $Full = [IO.Path]::GetFullPath((Resolve-Path -LiteralPath $Root).Path).TrimEnd($Separators)
  if ([string]::IsNullOrEmpty($Full) -or $Full -eq [IO.Path]::GetPathRoot($Full).TrimEnd($Separators)) {
    throw "refusing to use a filesystem root as a native build cache root: $Root"
  }
  return $Full
}

function Assert-NativeBuildCacheChainIsReal {
  param(
    [Parameter(Mandatory = $true)][string]$RootFull,
    [Parameter(Mandatory = $true)][string]$Key,
    [Parameter(Mandatory = $true)][string[]]$RequiredFiles
  )

  # The root's own ancestors are covered lexically; here the keyed cache, its
  # completion marker, and every required artifact must also be real, so no
  # marker is ever read and no sibling ever deleted on the strength of a link.
  $Active = Join-Path $RootFull $Key
  $Chain = [Collections.Generic.List[string]]::new()
  $Chain.Add($Active)
  $Chain.Add((Join-Path $Active $script:NativeBuildCacheMarkerFile))
  foreach ($Relative in $RequiredFiles) {
    if ([IO.Path]::IsPathRooted($Relative) -or $Relative -split "[\\/]" -contains "..") {
      throw "native build cache required paths must stay relative: $Relative"
    }
    $Walk = $Active
    foreach ($Segment in ($Relative -split "[\\/]" | Where-Object { $_ -ne "" })) {
      $Walk = Join-Path $Walk $Segment
      $Chain.Add($Walk)
    }
  }
  foreach ($Entry in $Chain) {
    if (-not (Test-Path -LiteralPath $Entry)) { continue }
    if ([IO.File]::GetAttributes($Entry).HasFlag([IO.FileAttributes]::ReparsePoint)) {
      throw "refusing to trust a native build cache through a reparse point: $Entry"
    }
  }
}

function Enter-NativeBuildCacheOperationLock {
  param(
    [Parameter(Mandatory = $true)][string]$Root,
    [int]$TimeoutMilliseconds = 7200000
  )

  # One exclusive lock covers a build's entire cache lifetime: selection,
  # production, publication, use, and pruning. Native release builds serialize
  # on it instead of racing, which is what makes pruning safe without any
  # guesswork about who owns what.
  $RootFull = Resolve-NativeBuildCacheRoot $Root
  $LockPath = Join-Path $RootFull $script:NativeBuildCacheLockFile
  $Deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMilliseconds)
  # A lock path that is itself a link would put this build's exclusion, and the
  # pruning it authorizes, under someone else's control.
  if ((Test-Path -LiteralPath $LockPath) -and [IO.File]::GetAttributes($LockPath).HasFlag([IO.FileAttributes]::ReparsePoint)) {
    throw "refusing to use a native build cache lock through a reparse point: $LockPath"
  }
  while ($true) {
    try {
      $Handle = [IO.File]::Open($LockPath, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
      return [pscustomobject]@{ Root = $RootFull; Path = $LockPath; Handle = $Handle }
    } catch [IO.IOException] {
      if ([DateTime]::UtcNow -ge $Deadline) {
        throw "timed out waiting for the native build cache root lock: $LockPath"
      }
      Start-Sleep -Milliseconds 200
    }
  }
}

function Exit-NativeBuildCacheOperationLock {
  param([Parameter(Mandatory = $true)]$Lock)

  if ($null -ne $Lock.Handle) { $Lock.Handle.Dispose() }
}

function Assert-NativeBuildCacheOperationLock {
  param(
    [AllowNull()]$Lock,
    [Parameter(Mandatory = $true)][string]$RootFull
  )

  if ($null -eq $Lock) {
    throw "native build cache operations require the cache root operation lock: $RootFull"
  }
  $Properties = $Lock.PSObject.Properties.Name
  if ($Properties -notcontains "Root" -or $Properties -notcontains "Handle") {
    throw "native build cache operations require a cache root operation lock, got $($Lock.GetType().FullName)"
  }
  if ($Lock.Root -ne $RootFull) {
    throw "the operation lock belongs to a different cache root: $($Lock.Root)"
  }
  if ($null -eq $Lock.Handle -or -not $Lock.Handle.CanWrite) {
    throw "the native build cache root operation lock is no longer held: $RootFull"
  }
}

function Clear-NativeBuildCacheForRebuild {
  param(
    [Parameter(Mandatory = $true)][string]$Root,
    [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Key,
    [Parameter(Mandatory = $true)][string[]]$RequiredFiles,
    [Parameter(Mandatory = $true)][AllowNull()]$Lock
  )

  # Returns $true when the keyed cache is complete and must be used as it
  # stands, $false when the caller owns the rebuild. Under the operation lock a
  # cache another build finished earlier is recognized, never deleted.
  Assert-NativeBuildCacheKey $Key
  $RootFull = Resolve-NativeBuildCacheRoot $Root
  Assert-NativeBuildCacheOperationLock $Lock $RootFull
  Assert-NativeBuildCacheChainIsReal $RootFull $Key $RequiredFiles
  $Active = Join-Path $RootFull $Key
  if (Test-NativeBuildCache $Active $Key $RequiredFiles) { return $true }
  if (Test-Path -LiteralPath $Active) { Remove-Item -LiteralPath $Active -Recurse -Force }
  return $false
}

function Remove-StaleNativeBuildCaches {
  param(
    [Parameter(Mandatory = $true)][string]$Root,
    [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Key,
    [Parameter(Mandatory = $true)][string[]]$RequiredFiles,
    [Parameter(Mandatory = $true)][AllowNull()]$Lock
  )

  Assert-NativeBuildCacheKey $Key
  $RootFull = Resolve-NativeBuildCacheRoot $Root
  Assert-NativeBuildCacheOperationLock $Lock $RootFull
  Assert-NativeBuildCacheChainIsReal $RootFull $Key $RequiredFiles

  # The active cache must be complete before anything else is removed: a
  # half-built cache must never cost us the last working one.
  $Active = Join-Path $RootFull $Key
  if (-not (Test-NativeBuildCache $Active $Key $RequiredFiles)) {
    throw "refusing to prune native build caches: $Key is not verified complete"
  }

  # The caller holds the operation lock, so no other build is selecting,
  # producing, publishing, or reading a cache in this root right now. Every
  # keyed sibling is superseded and every pending directory is abandoned.
  $Removed = @()
  foreach ($Candidate in (Get-ChildItem -LiteralPath $RootFull -Directory -Force)) {
    if ($Candidate.Name -eq $Key) { continue }
    if ($Candidate.Name -notmatch $script:NativeBuildCachePrunablePattern) { continue }
    # Never follow a junction or symlink out of the cache root.
    if ($Candidate.Attributes.HasFlag([IO.FileAttributes]::ReparsePoint)) { continue }
    $CandidateFull = [IO.Path]::GetFullPath($Candidate.FullName)
    if (-not $CandidateFull.StartsWith($RootFull + [IO.Path]::DirectorySeparatorChar, [StringComparison]::Ordinal)) { continue }
    Remove-Item -LiteralPath $CandidateFull -Recurse -Force
    $Removed += $CandidateFull
  }
  return $Removed
}
