# Builds Carryover-Setup.exe on a PC. See package-windows.sh, which this
# runs through Git for Windows' sh.
#   package-windows.ps1 [-Out file.exe]
param([string]$Out = (Join-Path $PSScriptRoot '..\target\Carryover-Setup.exe'))
$ErrorActionPreference = 'Stop'
$Out = [IO.Path]::GetFullPath($Out)
sh (Join-Path $PSScriptRoot 'package-windows.sh') $Out.Replace('\', '/')
if ($LASTEXITCODE) { throw 'packaging failed' }
