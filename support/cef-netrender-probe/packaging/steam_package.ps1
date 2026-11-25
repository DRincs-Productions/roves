# Derives the directly playable Steam package from the plain playable package (README "Steam",
# "Releases"): the same play.exe (icon-patched), CEF runtime, game\, icons and provenance, with
# play.dll replaced by the `steam` build and Valve's steam_api64.dll redistributable (from
# steamworks-sys, copied by the bundle binary) added next to play.exe. Used by
# .github/workflows/cef-netrender-probe.yml after the Steam bundle is built.
#
# Gates: apart from the cdylib, steam_api64.dll and .pdb files every file of the Steam bundle
# must be byte-identical in the plain bundle (so reusing the plain package's files is exact); every
# DLL that the Steam play.dll and steam_api64.dll import resolves in the package or Windows;
# the required files exist. PROVENANCE.txt gets a "Steam variant" section, provenance\
# imports.txt and rust-dependencies.txt describe the Steam build, SHA256SUMS.txt is rebuilt.
#
# Usage: steam_package.ps1 -Plain <plain package dir> -PlainBundle <cef-nr-bundle>
#          -SteamBundle <cef-nr-bundle-steam> -ProbeDir <support/cef-netrender-probe> -Out <new dir>
# The package folder is <Out>\visual-novel-template-roves-cef-steam; exits 1 on any failure.
param(
    [Parameter(Mandatory)] [string] $Plain,
    [Parameter(Mandatory)] [string] $PlainBundle,
    [Parameter(Mandatory)] [string] $SteamBundle,
    [Parameter(Mandatory)] [string] $ProbeDir,
    [Parameter(Mandatory)] [string] $Out
)
$ErrorActionPreference = 'Stop'
$failed = $false
$cdylib = 'roves_cef_netrender_probe.dll'

# 1. The Steam bundle adds nothing to the plain bundle but the cdylib and steam_api64.dll.
$relFiles = {
    param($dir)
    $full = (Resolve-Path -LiteralPath $dir).Path
    Get-ChildItem -LiteralPath $full -Recurse -File | Where-Object Extension -ne '.pdb' |
        ForEach-Object { $_.FullName.Substring($full.Length + 1) } | Sort-Object
}
$steamFiles = @(& $relFiles $SteamBundle | Where-Object { $_ -notin @($cdylib, 'steam_api64.dll') })
$plainFiles = @(& $relFiles $PlainBundle | Where-Object { $_ -ne $cdylib })
# Steam-only files would be missing from the derived package; plain-only files (anything an
# earlier run left next to the plain bundle's executable) are already in the plain package.
$steamOnly = @($steamFiles | Where-Object { $_ -notin $plainFiles })
if ($steamOnly.Count) {
    $steamOnly | ForEach-Object { Write-Output "::error::the Steam bundle has $_, the plain bundle does not" }
    exit 1
}
$plainOnly = @($plainFiles | Where-Object { $_ -notin $steamFiles })
if ($plainOnly.Count) { Write-Output "::notice::only in the plain bundle (kept from the plain package): $($plainOnly -join ', ')" }
foreach ($rel in $steamFiles) {
    $a = (Get-FileHash -LiteralPath (Join-Path $SteamBundle $rel)).Hash
    $b = (Get-FileHash -LiteralPath (Join-Path $PlainBundle $rel)).Hash
    if ($a -ne $b) { Write-Output "::error::Steam bundle $rel differs from the plain bundle"; $failed = $true }
}
foreach ($required in @($cdylib, 'steam_api64.dll')) {
    if (-not (Test-Path -LiteralPath (Join-Path $SteamBundle $required))) { Write-Output "::error::Steam bundle lacks $required"; $failed = $true }
}
if ($failed) { exit 1 }

# 2. The package: the plain package with the Steam play.dll and steam_api64.dll.
if (Test-Path -LiteralPath $Out) { Write-Output "::error::$Out already exists"; exit 1 }
New-Item -ItemType Directory -Force $Out | Out-Null
$pkg = Join-Path $Out 'visual-novel-template-roves-cef-steam'
Copy-Item -Recurse -LiteralPath $Plain -Destination $pkg
if (Test-Path -LiteralPath (Join-Path $pkg 'steam_api64.dll')) { Write-Output '::error::the plain package already contains steam_api64.dll'; exit 1 }
Copy-Item -LiteralPath (Join-Path $SteamBundle $cdylib) -Destination (Join-Path $pkg 'play.dll') -Force
Copy-Item -LiteralPath (Join-Path $SteamBundle 'steam_api64.dll') -Destination $pkg

# 3. Imports of the two new modules (the plain package's report covers the rest).
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$dumpbin = & $vswhere -latest -products * -find 'VC\Tools\MSVC\**\bin\Hostx64\x64\dumpbin.exe' | Select-Object -First 1
if (-not $dumpbin) { Write-Output '::error::dumpbin.exe not found'; exit 1 }
$imports = Join-Path $pkg 'provenance\imports.txt'
$report = New-Object System.Collections.Generic.List[string]
Get-Content -LiteralPath $imports | Where-Object { $_ -notlike 'play.dll -> *' } | ForEach-Object { $report.Add($_) }
foreach ($module in @('play.dll', 'steam_api64.dll')) {
    $delay = $false
    foreach ($line in (& $dumpbin /nologo /dependents (Join-Path $pkg $module))) {
        if ($line -match 'following delay load dependencies') { $delay = $true; continue }
        if ($line -match 'following dependencies') { $delay = $false; continue }
        if ($line -notmatch '^\s{4}(\S+\.dll)\s*$') { continue }
        $dll = $Matches[1]
        if (Test-Path -LiteralPath (Join-Path $pkg $dll)) { $where = 'package' }
        elseif ($dll -match '^(api|ext)-ms-') { $where = 'Windows API set' }
        elseif (Test-Path -LiteralPath (Join-Path "$env:SystemRoot\System32" $dll)) { $where = 'Windows System32' }
        elseif ($delay) { $where = 'not found (delay-loaded, optional)' }
        else { $where = 'MISSING'; $failed = $true; Write-Output "::error::$module imports $dll : MISSING" }
        $report.Add("$module -> $dll$(if ($delay) { ' (delay)' }) : $where")
    }
}
$report | Set-Content -Encoding utf8 $imports

# 4. Provenance of the Steam build.
Push-Location $ProbeDir
cargo tree --locked --target x86_64-pc-windows-msvc -e normal --features steam --prefix none --format '{p} {l}' |
    ForEach-Object { $_ -replace ' \(\*\)$', '' } | Sort-Object -Unique |
    Set-Content -Encoding utf8 (Join-Path $pkg 'provenance\rust-dependencies.txt')
$cargoTree = $LASTEXITCODE
Pop-Location
if ($cargoTree -ne 0) { Write-Output '::error::cargo tree --features steam failed'; $failed = $true }
$dllHash = (Get-FileHash -LiteralPath (Join-Path $pkg 'play.dll')).Hash.ToLowerInvariant()
$steamHash = (Get-FileHash -LiteralPath (Join-Path $pkg 'steam_api64.dll')).Hash.ToLowerInvariant()
@"

Steam variant (built with --features steam)
  play.dll is the player built with the steam feature (steamworks 0.13.1, steamworks-sys
  0.13.0; MIT/Apache-2.0), sha256 $dllHash.
  steam_api64.dll is Valve's Steamworks SDK redistributable as shipped by steamworks-sys
  (redistribution under Valve's Steamworks SDK terms), sha256
  $steamHash. play.dll delay-loads it: without the Steam client the game plays normally
  and the log reports "steam unavailable". The App ID comes from the Steam launch, or from
  a development steam_appid.txt next to play.exe.
  provenance\rust-dependencies.txt and provenance\imports.txt describe this Steam build.
"@ | Add-Content -Encoding utf8 (Join-Path $pkg 'PROVENANCE.txt')

# 5. Checksums and required files.
Push-Location $pkg
Get-ChildItem -Recurse -File | Where-Object Name -ne 'SHA256SUMS.txt' | ForEach-Object {
    "$((Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash.ToLowerInvariant())  $((Resolve-Path -Relative $_.FullName).Substring(2).Replace('\', '/'))"
} | Sort-Object { $_.Substring(66) } | Set-Content -Encoding utf8 'SHA256SUMS.txt'
Pop-Location
foreach ($required in @('play.exe', 'play.dll', 'play.exe.manifest', 'libcef.dll', 'steam_api64.dll', 'game\index.html', 'roves.ico', 'PROVENANCE.txt', 'SHA256SUMS.txt')) {
    if (-not (Test-Path -LiteralPath (Join-Path $pkg $required))) { Write-Output "::error::Steam package lacks $required"; $failed = $true }
}
if (Test-Path -LiteralPath (Join-Path $pkg 'mcp')) { Write-Output '::error::Steam package contains mcp\'; $failed = $true }
if ($failed) { exit 1 }
Write-Output "::notice::Steam package $pkg : play.dll $($dllHash.Substring(0, 16)), steam_api64.dll $($steamHash.Substring(0, 16))"
exit 0
