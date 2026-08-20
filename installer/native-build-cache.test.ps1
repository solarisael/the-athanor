$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot "native-build-cache.ps1")

function Assert-True([bool]$Condition, [string]$Message) {
  if (-not $Condition) { throw $Message }
}

$Root = Join-Path ([IO.Path]::GetTempPath()) "athanor-native-cache-test-$PID-$([Guid]::NewGuid().ToString('N'))"
$Required = @("postgresql/bin/postgres.exe", "postgresql/lib/vector.dll", "godot/athanor-gui.exe")
$Key = Get-NativeBuildCacheKey @("cache-v1", "postgres-sha", "pgvector-sha", "godot-sha")
try {
  New-Item $Root -ItemType Directory -Force | Out-Null
  Assert-True (-not (Test-NativeBuildCache $Root $Key $Required)) "an unmarked cache must be rejected"

  foreach ($Relative in $Required) {
    $Target = Join-Path $Root $Relative
    New-Item ([IO.Path]::GetDirectoryName($Target)) -ItemType Directory -Force | Out-Null
    Set-Content $Target "fixture" -Encoding ascii
  }
  Assert-True (-not (Test-NativeBuildCache $Root $Key $Required)) "files without a completion marker must be rejected"

  Complete-NativeBuildCache $Root $Key $Required
  Assert-True (Test-NativeBuildCache $Root $Key $Required) "a complete cache with the matching key must be accepted"
  Assert-True (-not (Test-NativeBuildCache $Root "wrong-key" $Required)) "a cache built from different inputs must be rejected"

  Remove-Item (Join-Path $Root $Required[1]) -Force
  Assert-True (-not (Test-NativeBuildCache $Root $Key $Required)) "a cache missing a required artifact must be rejected"

  $Escaped = $false
  try { Test-NativeBuildCache $Root $Key @("../outside") | Out-Null } catch { $Escaped = $true }
  Assert-True $Escaped "required cache paths must not escape the cache root"

  Write-Host "native build cache contract passed"
} finally {
  Remove-Item $Root -Recurse -Force -ErrorAction SilentlyContinue
}

# Retention: repeated key changes must not leave one dependency payload behind
# per key, an unverified cache must never delete a working one, and every
# mutation must happen under the cache root operation lock.
$PruneRoot = Join-Path ([IO.Path]::GetTempPath()) "athanor-native-cache-prune-$PID-$([Guid]::NewGuid().ToString('N'))"
function New-CacheFixture([string]$Path, [string]$FixtureKey, [string[]]$Files, [bool]$Complete) {
  foreach ($Relative in $Files) {
    $Target = Join-Path $Path $Relative
    New-Item ([IO.Path]::GetDirectoryName($Target)) -ItemType Directory -Force | Out-Null
    Set-Content $Target "fixture" -Encoding ascii
  }
  if ($Complete) { Complete-NativeBuildCache $Path $FixtureKey $Files }
}

# The marker and lock guards need a real reparse point. File symlinks are the
# exact shape an attacker or a careless mirror script produces; where the
# environment refuses to create one, a junction is the same reparse attribute.
function New-ReparseLink([string]$Path, [string]$FileTarget, [string]$DirectoryTarget) {
  try {
    New-Item -ItemType SymbolicLink -Path $Path -Target $FileTarget -ErrorAction Stop | Out-Null
    return "symlink"
  } catch {
    New-Item -ItemType Junction -Path $Path -Target $DirectoryTarget -ErrorAction Stop | Out-Null
    return "junction"
  }
}
$Lock = $null
try {
  New-Item $PruneRoot -ItemType Directory -Force | Out-Null
  $ActiveKey = Get-NativeBuildCacheKey @("prune-v1", "active")
  $StaleKey = Get-NativeBuildCacheKey @("prune-v1", "stale")
  $ActivePath = Join-Path $PruneRoot $ActiveKey
  $StalePath = Join-Path $PruneRoot $StaleKey
  $PendingPath = "$StalePath.pending-4242"
  $OwnPendingPath = "$ActivePath.pending-$PID"
  $UnrelatedFile = Join-Path $PruneRoot "downloads.log"
  $UnrelatedDirectory = Join-Path $PruneRoot "scratch"

  New-CacheFixture $ActivePath $ActiveKey $Required $true
  New-CacheFixture $StalePath $StaleKey $Required $true
  New-CacheFixture $PendingPath $StaleKey $Required $false
  New-CacheFixture $OwnPendingPath $ActiveKey $Required $false
  Set-Content $UnrelatedFile "keep me" -Encoding ascii
  New-Item $UnrelatedDirectory -ItemType Directory -Force | Out-Null

  # Every mutating call requires the operation lock this build holds.
  $Unlocked = $false
  try { Remove-StaleNativeBuildCaches $PruneRoot $ActiveKey $Required $null | Out-Null } catch { $Unlocked = $true }
  Assert-True $Unlocked "pruning without the operation lock must be refused"
  Assert-True (Test-Path $StalePath -PathType Container) "a refused unlocked prune must delete nothing"
  $UnlockedClear = $false
  try { Clear-NativeBuildCacheForRebuild $PruneRoot $ActiveKey $Required $null | Out-Null } catch { $UnlockedClear = $true }
  Assert-True $UnlockedClear "rebuild selection without the operation lock must be refused"

  $Lock = Enter-NativeBuildCacheOperationLock $PruneRoot
  Assert-True (Test-Path $Lock.Path -PathType Leaf) "the operation lock must be a real file in the cache root"

  # A lock taken on a different root is not this root's authority.
  $ForeignRoot = Join-Path ([IO.Path]::GetTempPath()) "athanor-native-cache-foreign-$PID-$([Guid]::NewGuid().ToString('N'))"
  New-Item $ForeignRoot -ItemType Directory -Force | Out-Null
  $ForeignLock = Enter-NativeBuildCacheOperationLock $ForeignRoot
  try {
    $WrongRoot = $false
    try { Remove-StaleNativeBuildCaches $PruneRoot $ActiveKey $Required $ForeignLock | Out-Null } catch { $WrongRoot = $true }
    Assert-True $WrongRoot "a lock held on another cache root must be refused"
  } finally {
    Exit-NativeBuildCacheOperationLock $ForeignLock
    Remove-Item $ForeignRoot -Recurse -Force -ErrorAction SilentlyContinue
  }
  $AbsentRootLock = $false
  try { Enter-NativeBuildCacheOperationLock $ForeignRoot -TimeoutMilliseconds 200 | Out-Null } catch { $AbsentRootLock = $true }
  Assert-True $AbsentRootLock "a lock cannot be taken on a cache root that no longer exists"

  $Stale = $false
  $StaleHandleLock = [pscustomobject]@{ Root = (Resolve-Path $PruneRoot).Path; Path = $Lock.Path; Handle = $null }
  try { Remove-StaleNativeBuildCaches $PruneRoot $ActiveKey $Required $StaleHandleLock | Out-Null } catch { $Stale = $true }
  Assert-True $Stale "a lock object without a held handle must be refused"

  $RemovedPaths = @(Remove-StaleNativeBuildCaches $PruneRoot $ActiveKey $Required $Lock)

  Assert-True (Test-Path $ActivePath -PathType Container) "the active cache key must be preserved"
  Assert-True (Test-NativeBuildCache $ActivePath $ActiveKey $Required) "the active cache must stay verifiable after pruning"
  Assert-True (-not (Test-Path $StalePath)) "a stale keyed cache must be removed"
  Assert-True (-not (Test-Path $PendingPath)) "an abandoned pending cache must be removed"
  Assert-True (-not (Test-Path $OwnPendingPath)) "a pending cache for the active key must be removed too"
  Assert-True (Test-Path $UnrelatedFile -PathType Leaf) "unrelated files in the cache root must be preserved"
  Assert-True (Test-Path $UnrelatedDirectory -PathType Container) "directories that are not cache keys must be preserved"
  Assert-True (Test-Path $Lock.Path -PathType Leaf) "the operation lock file must survive pruning"
  Assert-True ($RemovedPaths.Count -eq 3) "pruning must report exactly the removed cache directories"

  # Idempotent: pruning a root that holds only the active cache removes nothing.
  Assert-True (@(Remove-StaleNativeBuildCaches $PruneRoot $ActiveKey $Required $Lock).Count -eq 0) "pruning must be idempotent"

  # A second builder cannot hold the same root while this one holds it.
  $Contended = $false
  try { Enter-NativeBuildCacheOperationLock $PruneRoot -TimeoutMilliseconds 300 | Out-Null } catch { $Contended = $true }
  Assert-True $Contended "a second operation lock on a held root must time out"

  # An incomplete new cache must refuse instead of deleting the prior complete one.
  $IncompleteKey = Get-NativeBuildCacheKey @("prune-v1", "incomplete")
  $IncompletePath = Join-Path $PruneRoot $IncompleteKey
  New-CacheFixture $IncompletePath $IncompleteKey $Required $false
  $Refused = $false
  try { Remove-StaleNativeBuildCaches $PruneRoot $IncompleteKey $Required $Lock | Out-Null } catch { $Refused = $true }
  Assert-True $Refused "an unverified cache must refuse to prune"
  Assert-True (Test-Path $ActivePath -PathType Container) "a refused prune must leave the prior complete cache intact"

  # Same-key convergence: selection recognizes a complete cache instead of
  # clearing it, and clears only an incomplete one.
  Assert-True (Clear-NativeBuildCacheForRebuild $PruneRoot $ActiveKey $Required $Lock) "a complete cache must be reported as ready"
  Assert-True (Test-NativeBuildCache $ActivePath $ActiveKey $Required) "a complete cache must survive rebuild selection"
  Assert-True (-not (Clear-NativeBuildCacheForRebuild $PruneRoot $IncompleteKey $Required $Lock)) "an incomplete cache must hand the rebuild to the caller"
  Assert-True (-not (Test-Path $IncompletePath)) "an incomplete cache must be cleared for rebuild"
  Assert-True (-not (Clear-NativeBuildCacheForRebuild $PruneRoot $IncompleteKey $Required $Lock)) "an absent cache must hand the rebuild to the caller"

  # Reparse points: refused before any completion marker is trusted, whether the
  # keyed cache directory, the cache root, or an ancestor of the root is a link.
  $Outside = Join-Path ([IO.Path]::GetTempPath()) "athanor-native-cache-outside-$PID-$([Guid]::NewGuid().ToString('N'))"
  $JunctionKey = Get-NativeBuildCacheKey @("prune-v1", "junction")
  $JunctionPath = Join-Path $PruneRoot $JunctionKey
  $SurvivorKey = Get-NativeBuildCacheKey @("prune-v1", "survivor")
  $SurvivorPath = Join-Path $PruneRoot $SurvivorKey
  New-CacheFixture $SurvivorPath $SurvivorKey $Required $true
  try {
    # The external target is a complete cache root holding a complete keyed
    # cache, so nothing but the reparse guard can explain a refusal.
    New-Item $Outside -ItemType Directory -Force | Out-Null
    New-CacheFixture $Outside $JunctionKey $Required $true
    New-CacheFixture (Join-Path $Outside $JunctionKey) $JunctionKey $Required $true
    New-Item -ItemType Junction -Path $JunctionPath -Target $Outside | Out-Null
    Assert-True (Test-NativeBuildCache $JunctionPath $JunctionKey $Required) "the junction fixture must look complete to the plain check"

    $JunctionFailure = ""
    try { Remove-StaleNativeBuildCaches $PruneRoot $JunctionKey $Required $Lock | Out-Null } catch { $JunctionFailure = $_.Exception.Message }
    Assert-True ($JunctionFailure -like "*reparse point*") "a cache directory that is a reparse point must be refused as a reparse point, got: $JunctionFailure"
    Assert-True (Test-Path $SurvivorPath -PathType Container) "a refused reparse prune must delete nothing"

    # The junction as the cache root itself: its keyed cache is complete and a
    # stale sibling is present, so only the lexical guard can refuse.
    $OutsideStale = Join-Path $Outside $StaleKey
    New-CacheFixture $OutsideStale $StaleKey $Required $true
    $RootFailure = ""
    try { Remove-StaleNativeBuildCaches $JunctionPath $JunctionKey $Required $Lock | Out-Null } catch { $RootFailure = $_.Exception.Message }
    Assert-True ($RootFailure -like "*reparse point*") "a cache root reached through a reparse point must be refused as a reparse point, got: $RootFailure"
    Assert-True (Test-Path $OutsideStale -PathType Container) "a refused root reparse prune must delete nothing"

    # An ancestor of the root is a link: rejected lexically, before resolution.
    $NestedRoot = Join-Path $JunctionPath $JunctionKey
    $AncestorFailure = ""
    try { Enter-NativeBuildCacheOperationLock $NestedRoot -TimeoutMilliseconds 200 | Out-Null } catch { $AncestorFailure = $_.Exception.Message }
    Assert-True ($AncestorFailure -like "*reparse point*") "a cache root under a reparse ancestor must be refused before resolution, got: $AncestorFailure"
    Assert-True (-not (Test-Path (Join-Path $NestedRoot ".operation.lock"))) "a lexically refused root must not be touched at all"
  } finally {
    Remove-Item $JunctionPath -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item $Outside -Recurse -Force -ErrorAction SilentlyContinue
  }

  $InnerKey = Get-NativeBuildCacheKey @("prune-v1", "inner")
  $InnerPath = Join-Path $PruneRoot $InnerKey
  $InnerOutside = Join-Path ([IO.Path]::GetTempPath()) "athanor-native-cache-inner-$PID-$([Guid]::NewGuid().ToString('N'))"
  try {
    New-CacheFixture $InnerPath $InnerKey $Required $true
    New-Item $InnerOutside -ItemType Directory -Force | Out-Null
    Copy-Item (Join-Path $InnerPath "postgresql/*") $InnerOutside -Recurse -Force
    Remove-Item (Join-Path $InnerPath "postgresql") -Recurse -Force
    New-Item -ItemType Junction -Path (Join-Path $InnerPath "postgresql") -Target $InnerOutside | Out-Null
    Assert-True (Test-NativeBuildCache $InnerPath $InnerKey $Required) "the inner junction fixture must look complete to the plain check"
    $InnerFailure = ""
    try { Remove-StaleNativeBuildCaches $PruneRoot $InnerKey $Required $Lock | Out-Null } catch { $InnerFailure = $_.Exception.Message }
    Assert-True ($InnerFailure -like "*reparse point*") "a required artifact reached through a reparse point must be refused as a reparse point, got: $InnerFailure"
    Assert-True (Test-Path $SurvivorPath -PathType Container) "a refused inner reparse prune must delete nothing"
  } finally {
    Remove-Item (Join-Path $InnerPath "postgresql") -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item $InnerPath -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item $InnerOutside -Recurse -Force -ErrorAction SilentlyContinue
  }

  # The completion marker itself is a link to a matching external marker: the
  # required artifacts are all real and present, so only the marker guard can
  # explain the refusal, and the message must name the marker.
  $MarkerKey = Get-NativeBuildCacheKey @("prune-v1", "marker")
  $MarkerPath = Join-Path $PruneRoot $MarkerKey
  $MarkerOutside = Join-Path ([IO.Path]::GetTempPath()) "athanor-native-cache-marker-$PID-$([Guid]::NewGuid().ToString('N'))"
  try {
    New-CacheFixture $MarkerPath $MarkerKey $Required $true
    New-CacheFixture $MarkerOutside $MarkerKey $Required $true
    $ExternalMarker = Join-Path $MarkerOutside ".complete.json"
    Assert-True (Test-Path $ExternalMarker -PathType Leaf) "the external marker fixture must exist"
    $LinkedMarker = Join-Path $MarkerPath ".complete.json"
    Remove-Item $LinkedMarker -Force
    $MarkerLinkKind = New-ReparseLink $LinkedMarker $ExternalMarker $MarkerOutside
    Assert-True ([IO.File]::GetAttributes($LinkedMarker).HasFlag([IO.FileAttributes]::ReparsePoint)) "the marker fixture must really be a reparse point ($MarkerLinkKind)"
    if ($MarkerLinkKind -eq "symlink") {
      Assert-True (Test-NativeBuildCache $MarkerPath $MarkerKey $Required) "the linked marker fixture must look complete to the plain check"
    }
    $MarkerFailure = ""
    try { Remove-StaleNativeBuildCaches $PruneRoot $MarkerKey $Required $Lock | Out-Null } catch { $MarkerFailure = $_.Exception.Message }
    Assert-True ($MarkerFailure -like "*reparse point*") "a completion marker reached through a reparse point must be refused as a reparse point, got: $MarkerFailure"
    Assert-True ($MarkerFailure -like "*.complete.json*") "the refusal must name the completion marker, got: $MarkerFailure"
    Assert-True (Test-Path $ExternalMarker -PathType Leaf) "a refused marker prune must not touch the external marker"
    $MarkerClear = ""
    try { Clear-NativeBuildCacheForRebuild $PruneRoot $MarkerKey $Required $Lock | Out-Null } catch { $MarkerClear = $_.Exception.Message }
    Assert-True ($MarkerClear -like "*reparse point*") "rebuild selection must refuse a linked marker too, got: $MarkerClear"
    Assert-True (Test-Path $MarkerPath -PathType Container) "a refused selection must leave the linked cache in place"
  } finally {
    Remove-Item $LinkedMarker -Force -ErrorAction SilentlyContinue
    Remove-Item $MarkerPath -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item $MarkerOutside -Recurse -Force -ErrorAction SilentlyContinue
  }

  # A lock path that is already a link is refused before it is ever opened.
  $LockRoot = Join-Path ([IO.Path]::GetTempPath()) "athanor-native-cache-lockroot-$PID-$([Guid]::NewGuid().ToString('N'))"
  $LockOutside = Join-Path ([IO.Path]::GetTempPath()) "athanor-native-cache-lockfile-$PID-$([Guid]::NewGuid().ToString('N'))"
  try {
    New-Item $LockRoot -ItemType Directory -Force | Out-Null
    New-Item $LockOutside -ItemType Directory -Force | Out-Null
    $ExternalLockFile = Join-Path $LockOutside "borrowed.lock"
    Set-Content $ExternalLockFile "borrowed" -Encoding ascii
    New-ReparseLink (Join-Path $LockRoot ".operation.lock") $ExternalLockFile $LockOutside | Out-Null
    $LockFailure = ""
    try { Enter-NativeBuildCacheOperationLock $LockRoot -TimeoutMilliseconds 200 | Out-Null } catch { $LockFailure = $_.Exception.Message }
    Assert-True ($LockFailure -like "*reparse point*") "a lock path that is a reparse point must be refused, got: $LockFailure"
    Assert-True ((Get-Content $ExternalLockFile -Raw).Trim() -eq "borrowed") "a refused lock must not be written through the link"
  } finally {
    Remove-Item (Join-Path $LockRoot ".operation.lock") -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item $LockRoot -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item $LockOutside -Recurse -Force -ErrorAction SilentlyContinue
  }
  Remove-Item $SurvivorPath -Recurse -Force

  # Unsafe names and roots are refused outright.
  $RefusedName = $false
  try { Remove-StaleNativeBuildCaches $PruneRoot "../escape" $Required $Lock | Out-Null } catch { $RefusedName = $true }
  Assert-True $RefusedName "an unsafe cache key name must be refused"

  $RefusedRoot = $false
  try { Remove-StaleNativeBuildCaches ([IO.Path]::GetPathRoot($PruneRoot)) $ActiveKey $Required $Lock | Out-Null } catch { $RefusedRoot = $true }
  Assert-True $RefusedRoot "a filesystem root must never be pruned"

  $RefusedMissing = $false
  try { Remove-StaleNativeBuildCaches (Join-Path $PruneRoot "absent") $ActiveKey $Required $Lock | Out-Null } catch { $RefusedMissing = $true }
  Assert-True $RefusedMissing "a missing cache root must be refused"

  # The whole production cycle the release script performs while holding one
  # lock: select, build into a pending sibling, publish by move, then prune the
  # key it supersedes. This is what bounds the cache at one payload.
  $CycleKey = Get-NativeBuildCacheKey @("prune-v1", "cycle")
  $CyclePath = Join-Path $PruneRoot $CycleKey
  $CyclePending = "$CyclePath.pending-$PID"
  Assert-True (-not (Clear-NativeBuildCacheForRebuild $PruneRoot $CycleKey $Required $Lock)) "a new key must hand the rebuild to the caller"
  New-CacheFixture $CyclePending $CycleKey $Required $true
  Move-Item $CyclePending $CyclePath
  Assert-True (Test-NativeBuildCache $CyclePath $CycleKey $Required) "the published cache must verify"
  $CycleRemoved = @(Remove-StaleNativeBuildCaches $PruneRoot $CycleKey $Required $Lock)
  Assert-True ($CycleRemoved -contains [IO.Path]::GetFullPath($ActivePath)) "the superseded key must be pruned once the new key is published"
  Assert-True ($CycleRemoved.Count -eq 1) "the cycle must prune exactly the superseded key"
  Assert-True (Test-NativeBuildCache $CyclePath $CycleKey $Required) "the newly published cache must survive its own prune"
  Assert-True (Test-Path $UnrelatedFile -PathType Leaf) "the cycle must leave unrelated files alone"

  # Releasing the lock lets the next build take it, in this process or another.
  Exit-NativeBuildCacheOperationLock $Lock
  $Lock = Enter-NativeBuildCacheOperationLock $PruneRoot -TimeoutMilliseconds 1000
  Assert-True (@(Remove-StaleNativeBuildCaches $PruneRoot $CycleKey $Required $Lock).Count -eq 0) "a reacquired lock must authorize pruning again"

  Write-Host "native build cache retention contract passed"
} finally {
  if ($null -ne $Lock) { Exit-NativeBuildCacheOperationLock $Lock }
  Remove-Item $PruneRoot -Recurse -Force -ErrorAction SilentlyContinue
}
