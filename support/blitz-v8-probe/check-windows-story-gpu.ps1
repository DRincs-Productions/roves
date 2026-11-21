param(
    [Parameter(Mandatory)][string]$Probe,
    [Parameter(Mandatory)][string]$Game,
    [Parameter(Mandatory)][string]$OutputDirectory
)

$ErrorActionPreference = 'Stop'
$Probe = (Resolve-Path -LiteralPath $Probe).Path
$Game = (Resolve-Path -LiteralPath $Game).Path
$root = (New-Item -ItemType Directory -Path $OutputDirectory).FullName
$env:DENO_WEBGPU_BACKEND = 'vulkan'

function Require-Result($Reports, $Name, $Status) {
    $matches = @($Reports | Where-Object check -eq $Name)
    if ($matches.Count -ne 1 -or $matches[0].status -ne $Status) {
        throw "Expected one $Name report with status $Status. Logs: $root"
    }
    return $matches[0]
}

foreach ($mode in 'normal', 'jitless') {
    $jitless = $mode -eq 'jitless'
    $env:ROVES_PROBE_JITLESS = [string][int]$jitless
    foreach ($test in 'story', 'story-skip') {
        $name = "$mode-$test"
        $env:ROVES_PROBE_STORAGE_DIR = Join-Path $root "$name-storage"
        $log = Join-Path $root "$name.log"
        $process = Start-Process -FilePath $Probe -ArgumentList @('--game', ('"{0}"' -f $Game), "--$test") -WindowStyle Hidden -RedirectStandardOutput $log -RedirectStandardError (Join-Path $root "$name.err") -PassThru
        if (-not $process.WaitForExit(600000)) {
            $process.Kill()
            throw "$name timed out. Logs: $root"
        }
        if ($process.ExitCode -ne 0) { throw "$name exited with $($process.ExitCode). Logs: $root" }
        $prefix = 'ROVES_PROBE_RESULT '
        $reports = @(Get-Content -LiteralPath $log | Where-Object { $_.StartsWith($prefix) } | ForEach-Object {
            $_.Substring($prefix.Length) | ConvertFrom-Json
        })
        $gpu = Require-Result $reports 'gpu_adapter' 'ran'
        if ($gpu.adapter.device_type -notin 'DiscreteGpu', 'IntegratedGpu' -or $gpu.submits -le 0) {
            throw "$name did not submit work to a hardware GPU. Logs: $root"
        }
        $null = Require-Result $reports 'gpu_teardown' 'passed'
        if ($test -eq 'story') {
            $null = Require-Result $reports 'game_story' 'passed'
            $null = Require-Result $reports 'game_quick_save' 'passed'
            $null = Require-Result $reports 'game_hud_save_load' 'passed'
        } else {
            $skip = Require-Result $reports 'game_story_skip' 'passed'
            if ($skip.jitless -isnot [bool] -or $skip.jitless -ne $jitless) { throw "$name ran with the wrong V8 mode" }
        }
        Write-Output "$name passed on $($gpu.adapter.name)"
    }
}
