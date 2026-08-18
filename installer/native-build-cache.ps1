Set-StrictMode -Version Latest

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

  $MarkerPath = Join-Path $Path ".complete.json"
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
  $MarkerPath = Join-Path $Path ".complete.json"
  $TemporaryMarker = "$MarkerPath.$PID.tmp"
  [ordered]@{
    format = 1
    key = $Key
    requiredFiles = $RequiredFiles
    completedAt = [DateTimeOffset]::UtcNow.ToString("o")
  } | ConvertTo-Json -Depth 3 | Set-Content $TemporaryMarker -Encoding utf8NoBOM
  Move-Item $TemporaryMarker $MarkerPath -Force
}
