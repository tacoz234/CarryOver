# Builds the Windows installer natively; Git Bash is not required.
# Usage: .\desktop\scripts\package-windows.ps1 [-Out file.exe]
param([string]$Out = (Join-Path $PSScriptRoot '..\target\Carryover-Setup.exe'))
$ErrorActionPreference = 'Stop'
$Out = [IO.Path]::GetFullPath($Out)
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$previousExe = $env:CARRYOVER_EXE
$previousLoader = $env:CARRYOVER_LOADER
Push-Location $root
try {
    $rustInfo = & rustc -vV
    if ($LASTEXITCODE) { throw 'rustc -vV failed' }
    $hostLine = $rustInfo | Where-Object { $_ -like 'host:*' }
    $target = ($hostLine -replace '^host:\s*', '').Trim()
    if ($target -notmatch 'windows') { throw 'Run this script on Windows; use package-windows.sh on a Mac.' }
    $loaderArchitecture = switch -Regex ($target) {
        '^aarch64-' { 'arm64'; break }
        '^i686-' { 'x86'; break }
        default { 'x64' }
    }

    & cargo build --quiet --locked --release --target $target
    if ($LASTEXITCODE) { throw 'Building Carryover failed' }
    $metadataJson = & cargo metadata --locked --no-deps --format-version 1
    if ($LASTEXITCODE) { throw 'Reading Carryover build metadata failed' }
    $metadata = $metadataJson | ConvertFrom-Json
    $release = Join-Path $metadata.target_directory "$target\release"
    $env:CARRYOVER_EXE = Join-Path $release 'copyblip.exe'
    $loader = Get-ChildItem -Path (Join-Path $release "build\webview2-com-sys-*\out\$loaderArchitecture\WebView2Loader.dll") -File |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if (-not $loader) { throw "WebView2Loader.dll wasn't produced by the Carryover build" }
    $env:CARRYOVER_LOADER = $loader.FullName

    & cargo build --quiet --locked --release --target $target --manifest-path installer/Cargo.toml
    if ($LASTEXITCODE) { throw 'Building the installer failed' }
    $installerJson = & cargo metadata --locked --no-deps --format-version 1 --manifest-path installer/Cargo.toml
    if ($LASTEXITCODE) { throw 'Reading installer build metadata failed' }
    $installerMetadata = $installerJson | ConvertFrom-Json
    $installer = Join-Path $installerMetadata.target_directory "$target\release\Carryover-Setup.exe"
    New-Item -ItemType Directory -Force (Split-Path -Parent $Out) | Out-Null
    Copy-Item -LiteralPath $installer -Destination $Out -Force
    Write-Output "Wrote $Out"
} finally {
    $env:CARRYOVER_EXE = $previousExe
    $env:CARRYOVER_LOADER = $previousLoader
    Pop-Location
}
