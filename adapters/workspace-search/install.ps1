param(
  [string]$ToolRoot = (Join-Path $HOME '.omp/tools/athanor-workspace-search')
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$Node = (Get-Command node -CommandType Application).Source
$Npm = (Get-Command npm.cmd -CommandType Application).Source
$ArchiveRoot = Join-Path $ToolRoot 'archives'
New-Item -ItemType Directory -Force $ArchiveRoot | Out-Null
Push-Location $PSScriptRoot
try {
  & $Npm ci --include=dev --include=optional --no-audit --no-fund
  if ($LASTEXITCODE -ne 0) { throw 'The dependency install failed.' }
  & $Npm run build
  if ($LASTEXITCODE -ne 0) { throw 'The adapter build failed.' }
  $PackedText = & $Npm pack --ignore-scripts --json --pack-destination $ArchiveRoot
  if ($LASTEXITCODE -ne 0) { throw 'The package archive failed.' }
  $Packed = ($PackedText -join "`n" | ConvertFrom-Json)[0]
  $Archive = Join-Path $ArchiveRoot $Packed.filename
  $Digest = (Get-FileHash -Algorithm SHA256 $Archive).Hash.ToLowerInvariant()
  $RetainedArchive = Join-Path $ArchiveRoot "$Digest.tgz"
  if (Test-Path -LiteralPath $RetainedArchive) {
    Remove-Item -LiteralPath $Archive
  } else {
    Move-Item -LiteralPath $Archive -Destination $RetainedArchive
  }
  & $Npm install --prefix $ToolRoot --include=optional --no-audit --no-fund $RetainedArchive
  if ($LASTEXITCODE -ne 0) { throw 'The tool install failed. The MCP configuration is unchanged.' }
} finally {
  Pop-Location
}
$Entry = Join-Path $ToolRoot 'node_modules/@solarisael/athanor-workspace-search/dist/cli.js'
& $Node $Entry status --root $PSScriptRoot
if ($LASTEXITCODE -ne 0) { throw 'The installed status check failed. The MCP configuration is unchanged.' }
$ConfigPath = Join-Path $HOME '.omp/agent/mcp.json'
$Config = if (Test-Path -LiteralPath $ConfigPath) {
  Get-Content -LiteralPath $ConfigPath -Raw | ConvertFrom-Json -AsHashtable
} else { @{} }
if (-not $Config.ContainsKey('mcpServers')) { $Config.mcpServers = @{} }
$Config.mcpServers.zvec_grep = @{
  type = 'stdio'
  command = $Node
  args = @($Entry, 'server')
  timeout = 300000
}
$TemporaryConfig = "$ConfigPath.workspace-search-$PID.tmp"
try {
  [IO.File]::WriteAllText($TemporaryConfig, ($Config | ConvertTo-Json -Depth 30) + "`n")
  Move-Item -LiteralPath $TemporaryConfig -Destination $ConfigPath -Force
} finally {
  if (Test-Path -LiteralPath $TemporaryConfig) { Remove-Item -LiteralPath $TemporaryConfig }
}
Write-Host "Installed workspace search: $Digest"
Write-Host 'Restart OMP to load the new MCP server.'
