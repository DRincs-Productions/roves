# Stages the experimental downloadable Windows dev MCP bundle (README.md,
# "Downloadable Windows bundle"): the dev-mcp probe executable, the Node runtime
# that tested it, this sidecar with its locked production dependencies, the
# demo fixture, and the Deno extension sources the executable reads at run time.
param(
  [Parameter(Mandatory)] [string] $Probe,
  [Parameter(Mandatory)] [string] $Out,
  [string] $Node = (Get-Command node -ErrorAction Stop).Source,
  [string] $RegistrySrc = (Join-Path $env:CARGO_HOME 'registry/src'),
  [string] $Lock = (Join-Path $PSScriptRoot '../Cargo.lock')
)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true

if (Test-Path $Out) { throw "$Out already exists" }
New-Item -ItemType Directory $Out | Out-Null
$Out = (Resolve-Path $Out).Path

Copy-Item $Probe (Join-Path $Out 'roves-blitz-v8-probe.exe')

# Node: the exact runtime the CI smoke runs the staged sidecar with, plus its licence.
$nodeDir = New-Item -ItemType Directory (Join-Path $Out 'node')
Copy-Item $Node $nodeDir
$nodeLicense = Join-Path (Split-Path $Node -Parent) 'LICENSE'
if (-not (Test-Path $nodeLicense)) { throw "Node licence not found next to $Node" }
Copy-Item $nodeLicense $nodeDir

# Sidecar sources, demo fixture and locked production dependencies.
$mcp = New-Item -ItemType Directory (Join-Path $Out 'mcp')
foreach ($file in 'server.mjs', 'pixi.mjs', 'smoke.mjs', 'package.json', 'package-lock.json', 'README.md') {
  Copy-Item (Join-Path $PSScriptRoot $file) $mcp
}
Copy-Item -Recurse (Join-Path $PSScriptRoot 'fixture') $mcp
Push-Location $mcp
try { npm ci --omit=dev --ignore-scripts --no-audit --no-fund } finally { Pop-Location }

# Without a startup snapshot, deno_core loads extension JS from the absolute
# crate paths of the build machine (C:/roves-cargo/registry/src/<index>/...).
# Ship the JS/TS files and licences of every locked deno_* crate, keeping the
# <index>/<crate>-<version> layout, so that tree can be mounted at that path.
$crates = [regex]::Matches((Get-Content -Raw $Lock), '(?m)^name = "(deno_[^"]+)"\r?\nversion = "([^"]+)"') |
  ForEach-Object { "$($_.Groups[1].Value)-$($_.Groups[2].Value)" }
if (-not $crates) { throw "no deno_* crates in $Lock" }
$sources = Join-Path $Out 'deno-extension-sources'
$copied = 0
foreach ($index in Get-ChildItem -Directory $RegistrySrc) {
  foreach ($crate in $crates) {
    $root = Join-Path $index.FullName $crate
    if (-not (Test-Path $root)) { continue }
    $files = Get-ChildItem -Recurse -File $root -Include '*.js', '*.mjs', '*.ts', 'LICENSE*'
    foreach ($file in $files) {
      $target = Join-Path $sources (Join-Path $index.Name $file.FullName.Substring($index.FullName.Length + 1))
      New-Item -ItemType Directory -Force (Split-Path $target -Parent) | Out-Null
      Copy-Item $file.FullName $target
      $copied++
    }
  }
}
if (-not $copied) { throw "no deno_* extension sources under $RegistrySrc" }

[ordered]@{
  commit = $env:GITHUB_SHA
  node = (& $Node --version)
  deno_crates = $crates
  extension_source_files = $copied
} | ConvertTo-Json | Set-Content -Encoding utf8 (Join-Path $Out 'bundle-info.json')
Write-Host "staged $Out ($copied extension source files)"
