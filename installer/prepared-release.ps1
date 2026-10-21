Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot "native-release-contract.ps1")

function Get-PreparedReleaseField {
  param([object]$Source, [string]$Name, [type]$Type)

  if ($Source -isnot [System.Collections.IDictionary] -or -not $Source.Contains($Name) -or $Source[$Name] -isnot $Type) {
    throw "prepared release requires '$Name' with JSON type $($Type.Name)"
  }

  return ,$Source[$Name]
}

function Read-PreparedReleaseJson {
  param([string]$Path)

  $Document = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json -AsHashtable -ErrorAction Stop

  if ($Document -isnot [System.Collections.IDictionary]) {
    throw "prepared release requires a JSON object: $Path"
  }

  return $Document
}

function Assert-PreparedReleaseInteger {
  param([object]$Value, [string]$Description)

  if (($Value -isnot [long] -and $Value -isnot [int]) -or $Value -lt 0) {
    throw "$Description must be a nonnegative JSON integer"
  }
}

function Get-PreparedReleasePayload {
  param([string]$Payload, [string]$Version = "")

  $Payload = [IO.Path]::GetFullPath($Payload)
  $VolumeRoot = [IO.Path]::GetPathRoot($Payload)
  $Payload = Assert-NativeReleaseTrustedPath -Path $Payload -Root $VolumeRoot -Description "prepared payload" -Directory
  $Manifest = Assert-NativeReleaseTrustedPath -Path (Join-Path $Payload "release-manifest.json") -Root $VolumeRoot -Description "prepared release manifest"
  $Document = Read-PreparedReleaseJson -Path $Manifest
  $Format = Get-PreparedReleaseField $Document "format" ([object])
  Assert-PreparedReleaseInteger $Format "release manifest format"

  if ($Format -ne 1 -or (Get-PreparedReleaseField $Document "product" ([string])) -cne "the-athanor" -or
      (Get-PreparedReleaseField $Document "platform" ([string])) -cne "windows-x64") {
    throw "unsupported prepared release manifest"
  }

  $Release = Get-PreparedReleaseField $Document "version" ([string])
  Assert-NativeReleaseProductVersion -Version $Release -Description "prepared release version"

  if ($Version -and $Release -cne $Version) {
    throw "prepared release version does not match the receipt"
  }

  $Artifacts = Get-PreparedReleaseField $Document "artifacts" ([array])
  $Seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)

  foreach ($Artifact in $Artifacts) {
    $Relative = Get-PreparedReleaseField $Artifact "path" ([string])

    if ($Relative -cnotmatch '^[\x20-\x7e]+$' -or $Relative.Contains('\') -or $Relative.Contains(':') -or
        @($Relative.Split('/') | Where-Object { $_ -in @('', '.', '..') -or $_.EndsWith('.') -or $_.EndsWith(' ') -or $_ -match '[<>"|?*]' }).Count -gt 0 -or
        $Relative -ieq "release-manifest.json" -or -not $Seen.Add($Relative)) {
      throw "unsafe or duplicate prepared artifact path: $Relative"
    }

    $File = Assert-NativeReleaseTrustedPath -Path (Join-Path $Payload $Relative) -Root $Payload -Description "prepared artifact '$Relative'"
    $Size = Get-PreparedReleaseField $Artifact "size" ([object])
    Assert-PreparedReleaseInteger $Size "prepared artifact size"
    $Hash = Get-PreparedReleaseField $Artifact "sha256" ([string])

    if ($Hash -cnotmatch '^[0-9a-fA-F]{64}$' -or (Get-Item -LiteralPath $File).Length -ne $Size -or
        (Get-FileHash -LiteralPath $File -Algorithm SHA256).Hash -ine $Hash) {
      throw "prepared artifact size or SHA256 mismatch: $Relative"
    }
  }

  foreach ($Required in @('bin/athanor.exe', 'bin/athanor-substrate.exe', 'bin/athanor-omp-loader.ts', 'components/omp-adapter/component-manifest.json')) {
    if (-not $Seen.Contains($Required)) {
      throw "prepared release is missing required artifact: $Required"
    }
  }

  foreach ($Item in Get-ChildItem -LiteralPath $Payload -Force -Recurse) {
    $null = Assert-NativeReleaseTrustedPath -Path $Item.FullName -Root $Payload -Description "prepared payload entry" -Directory:($Item -is [IO.DirectoryInfo])
    $Relative = [IO.Path]::GetRelativePath($Payload, $Item.FullName).Replace('\', '/')

    if ($Item -isnot [IO.DirectoryInfo] -and $Relative -cne 'release-manifest.json' -and -not $Seen.Contains($Relative)) {
      throw "prepared payload contains an unlisted artifact: $Relative"
    }
  }

  return [pscustomobject]@{
    Release = $Release
    Payload = $Payload
    Manifest = $Manifest
    StagedManager = Join-Path $Payload 'bin/athanor.exe'
    AdapterSource = Join-Path $Payload 'components/omp-adapter'
  }
}

function Read-PreparedRelease {
  param([Parameter(Mandatory = $true)][string]$Path)

  $Path = [IO.Path]::GetFullPath($Path)
  $Path = Assert-NativeReleaseTrustedPath -Path $Path -Root ([IO.Path]::GetPathRoot($Path)) -Description "prepared release receipt"
  $Receipt = Read-PreparedReleaseJson -Path $Path
  $Format = Get-PreparedReleaseField $Receipt "format" ([object])
  Assert-PreparedReleaseInteger $Format "prepared release format"

  if ($Format -ne 1) { throw "unsupported prepared release receipt format" }

  $Version = Get-PreparedReleaseField $Receipt "version" ([string])
  Assert-NativeReleaseProductVersion -Version $Version -Description "prepared receipt version"
  $Hash = Get-PreparedReleaseField $Receipt "manifestSha256" ([string])
  $TestsPassed = Get-PreparedReleaseField $Receipt "testsPassed" ([bool])
  $Payload = Join-Path ([IO.Path]::GetDirectoryName($Path)) 'payload'
  $Prepared = Get-PreparedReleasePayload -Payload $Payload -Version $Version

  if ($Hash -cnotmatch '^[0-9a-fA-F]{64}$' -or (Get-FileHash -LiteralPath $Prepared.Manifest -Algorithm SHA256).Hash -ine $Hash) {
    throw "prepared release manifest SHA256 does not match the receipt"
  }

  $Prepared | Add-Member -NotePropertyName TestsPassed -NotePropertyValue $TestsPassed

  return $Prepared
}

function Write-PreparedRelease {
  param(
    [Parameter(Mandatory = $true)][string]$Payload,
    [Parameter(Mandatory = $true)][bool]$TestsPassed
  )

  $Prepared = Get-PreparedReleasePayload -Payload $Payload

  if ([IO.Path]::GetFileName($Prepared.Payload) -cne 'payload') {
    throw "prepared release payload must be named 'payload' beside its receipt"
  }

  $Path = Join-Path ([IO.Path]::GetDirectoryName($Prepared.Payload)) 'prepared-release.json'

  if (Test-Path -LiteralPath $Path) { throw "prepared release receipt already exists: $Path" }

  $Receipt = [ordered]@{
    format = 1
    version = $Prepared.Release
    manifestSha256 = (Get-FileHash -LiteralPath $Prepared.Manifest -Algorithm SHA256).Hash.ToLowerInvariant()
    testsPassed = $TestsPassed
  }
  $Temporary = "$Path.$([Guid]::NewGuid().ToString('N')).pending"

  try {
    [IO.File]::WriteAllText($Temporary, ($Receipt | ConvertTo-Json), [Text.UTF8Encoding]::new($false))
    [IO.File]::Move($Temporary, $Path, $false)
  } finally {
    if (Test-Path -LiteralPath $Temporary) { Remove-Item -LiteralPath $Temporary -Force }
  }

  return $Path
}
