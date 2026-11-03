# Builds the Windows package on a PC. On a Mac or Linux, use
# package-windows.sh instead.
#   package-windows.ps1 [-WithGroup] [-Out folder]
# Writes a CopyBlip folder and a zip of it. By default they carry no group,
# and setup asks to join or start one. -WithGroup joins them to this PC's
# group and relay, so they hold the group key; delete them once they're on
# the other PC.
param(
    [switch]$WithGroup,
    [string]$Out = (Join-Path $PSScriptRoot '..\target\CopyBlip-Windows')
)
$ErrorActionPreference = 'Stop'
$Out = [IO.Path]::GetFullPath($Out)
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
Push-Location $root
try {
    cargo build --quiet --release
    if ($LASTEXITCODE) { throw 'cargo build failed' }
    $exe = Join-Path $root 'target\release\copyblip.exe'
    $stage = Join-Path $Out 'CopyBlip'
    # Empties the folder rather than deleting it, in case Explorer has it open.
    if (Test-Path $stage) { Get-ChildItem $stage | Remove-Item -Recurse -Force }
    New-Item -ItemType Directory -Force $stage | Out-Null
    Copy-Item $exe $stage
    # The window's web view loads this at start; without it, Windows won't
    # run copyblip.exe at all.
    $loader = Get-ChildItem (Join-Path $root 'target\release\build') -Recurse -Filter WebView2Loader.dll |
        Where-Object { $_.Directory.Name -eq 'x64' } |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if (-not $loader) { throw "can't find WebView2Loader.dll in the build" }
    Copy-Item $loader.FullName $stage
    if ($WithGroup) {
        # Joins with this PC's invite link, so the other PC gets its own
        # device ID.
        $link = & $exe invite | Where-Object { $_ -like 'copyblip://*' }
        if (-not $link) { throw 'this PC has no group; run setup or copyblip init first' }
        $OutputEncoding = New-Object Text.UTF8Encoding $false
        $link | & $exe --config-dir $stage join | Out-Null
        if ($LASTEXITCODE) { throw 'joining the package to this group failed' }
    }
    foreach ($file in 'setup.cmd', 'invite.cmd', 'uninstall.cmd', 'README.txt') {
        # Windows wants CRLF line endings in batch files.
        $text = [IO.File]::ReadAllText((Join-Path $PSScriptRoot "windows\$file"))
        [IO.File]::WriteAllText((Join-Path $stage $file), ($text -replace '\r?\n', "`r`n"))
    }
    $zip = "$Out.zip"
    if (Test-Path $zip) { Remove-Item -Force $zip }
    Compress-Archive -Path $stage -DestinationPath $zip
    Write-Output "Wrote $stage and $zip"
} finally {
    Pop-Location
}
