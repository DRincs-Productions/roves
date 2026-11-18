<#
.SYNOPSIS
  Windows before/after benchmark of Blitz probe game startup and settled-menu CPU.

.DESCRIPTION
  Runs existing Windows probe artifacts (no build) against one game dist in
  alternating before/after pairs (pair 1 before->after, pair 2 after->before, ...).
  Every run gets the same environment: fresh isolated storage under -OutDir,
  DENO_WEBGPU_BACKEND=vulkan, ROVES_PROBE_FRAME_TIMINGS=1, the same JIT mode, and
  no other inherited ROVES_PROBE_*/DENO_WEBGPU_*/WGPU_* variables.

  Per run it records:
    - window_ms / window_cpu_s: wall-clock time from launch until the process has a
      visible top-level window handle, and the probe's CPU seconds at that moment
      (polling resolution ~15 ms). A window handle does not mean the menu is usable.
    - start_ms / start_cpu_s (only with -McpSidecar): time until an enabled element
      whose text is -StartButtonText exists in the DOM, observed through a real
      `server.mjs --attach` connection scoped to this process (per-run port and
      token). Requires probes built with --features dev-mcp; reported separately.
    - idle_cpu_s / idle_cpu_pct: probe CPU over a fixed wall-clock interval
      (-IdleSeconds) that starts -SettleSeconds after the window (or Start button).
    - frame timings: the ROVES_PROBE_FRAME_TIMINGS record printed at window close
      (whole session, last 3,600 samples), including present cause counters.
      presented_frames counts successful surface_present calls, not displayed
      frames; cpu_* values are shell-thread wall-clock, not GPU time.
    - idle_tail_sample (only with -McpSidecar): the probe's performance_sample
      (previous 5 s) taken right after the idle interval.

  The window is closed with WM_CLOSE (CloseMainWindow); only the exact launched
  process is killed if it does not exit within -CloseTimeoutSeconds. No input is
  sent to the game, no elevation or PresentMon is needed.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File support\blitz-v8-probe\bench-startup-idle.ps1 `
    -Before D:\bench\before -After D:\bench\after -GameDist D:\bench\template-dist `
    -OutDir D:\bench\out-1 -RovesBeforeRev 6d2faa14de3 -RovesAfterRev 4a598bd6fdc -TemplateRev 1ac2c1cc

.EXAMPLE
  # dev-mcp packages: also time Start button availability.
  ... -Before D:\bench\mcp-before\roves-blitz-v8-probe.exe -After D:\bench\mcp-after\roves-blitz-v8-probe.exe `
    -McpSidecar D:\bench\mcp-after\mcp\server.mjs -NodeExe D:\bench\mcp-after\node\node.exe
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$Before,
  [Parameter(Mandatory = $true)][string]$After,
  [Parameter(Mandatory = $true)][string]$GameDist,
  [Parameter(Mandatory = $true)][string]$OutDir,
  [Parameter(Mandatory = $true)][string]$RovesBeforeRev,
  [Parameter(Mandatory = $true)][string]$RovesAfterRev,
  [Parameter(Mandatory = $true)][string]$TemplateRev,
  [ValidateRange(1, 20)][int]$Pairs = 3,
  [ValidateRange(0, 600)][double]$SettleSeconds = 10,
  [ValidateRange(1, 600)][double]$IdleSeconds = 20,
  [ValidateRange(1, 600)][double]$WindowTimeoutSeconds = 60,
  [ValidateRange(1, 600)][double]$StartTimeoutSeconds = 120,
  [ValidateRange(1, 300)][double]$CloseTimeoutSeconds = 30,
  [switch]$Jitless,
  [string]$McpSidecar,
  [string]$NodeExe,
  [string]$StartButtonText = 'Start'
)

Set-StrictMode -Version 2
$ErrorActionPreference = 'Stop'

function Resolve-Probe([string]$path) {
  $item = Get-Item -LiteralPath $path
  if ($item.PSIsContainer) {
    $exe = Join-Path $item.FullName 'roves-blitz-v8-probe.exe'
    if (-not (Test-Path -LiteralPath $exe)) { throw "No roves-blitz-v8-probe.exe in $path" }
    return (Get-Item -LiteralPath $exe).FullName
  }
  return $item.FullName
}

function Get-Median($values) {
  $v = @($values | Where-Object { $null -ne $_ } | ForEach-Object { [double]$_ } | Sort-Object)
  if ($v.Count -eq 0) { return $null }
  $mid = [math]::Floor($v.Count / 2)
  if ($v.Count % 2) { return $v[$mid] }
  return ($v[$mid - 1] + $v[$mid]) / 2
}

function Get-FreePort {
  $listener = New-Object System.Net.Sockets.TcpListener ([System.Net.IPAddress]::Loopback), 0
  $listener.Start()
  try { return $listener.LocalEndpoint.Port } finally { $listener.Stop() }
}

# --- minimal MCP stdio client (JSON-RPC lines) over `server.mjs --attach` ---
function Start-McpClient([string]$port, [string]$token) {
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $script:Node
  $psi.Arguments = '"' + $script:Sidecar + '" --attach ' + $port
  $psi.UseShellExecute = $false
  $psi.CreateNoWindow = $true
  $psi.RedirectStandardInput = $true
  $psi.RedirectStandardOutput = $true
  $psi.StandardOutputEncoding = New-Object System.Text.UTF8Encoding $false
  $psi.EnvironmentVariables['ROVES_PROBE_DEV_MCP_TOKEN'] = $token
  $proc = [System.Diagnostics.Process]::Start($psi)
  $client = @{ proc = $proc; id = 0; pending = $null }
  [void](Invoke-Mcp $client 'initialize' @{
      protocolVersion = '2025-11-25'; capabilities = @{}
      clientInfo = @{ name = 'roves-bench-startup-idle'; version = '0.0.0' }
    } 30000)
  Send-Mcp $client @{ jsonrpc = '2.0'; method = 'notifications/initialized' }
  return $client
}

function Send-Mcp($client, $message) {
  $client.proc.StandardInput.WriteLine(($message | ConvertTo-Json -Compress -Depth 10))
  $client.proc.StandardInput.Flush()
}

function Invoke-Mcp($client, [string]$method, $params, [int]$timeoutMs) {
  $client.id++
  $id = $client.id
  Send-Mcp $client @{ jsonrpc = '2.0'; id = $id; method = $method; params = $params }
  $deadline = [DateTime]::UtcNow.AddMilliseconds($timeoutMs)
  while ($true) {
    $left = [int]($deadline - [DateTime]::UtcNow).TotalMilliseconds
    if ($left -le 0) { throw "MCP $method timed out" }
    if (-not $client.pending) { $client.pending = $client.proc.StandardOutput.ReadLineAsync() }
    if (-not $client.pending.Wait($left)) { throw "MCP $method timed out" }
    $line = $client.pending.Result
    $client.pending = $null
    if ($null -eq $line) { throw "MCP sidecar closed while waiting for $method" }
    $reply = $line | ConvertFrom-Json
    if (-not ($reply.PSObject.Properties.Name -contains 'id') -or $reply.id -ne $id) { continue }
    if ($reply.PSObject.Properties.Name -contains 'error') { throw "MCP ${method}: $($reply.error | ConvertTo-Json -Compress)" }
    return $reply.result
  }
}

# Returns @{ ok; value } (ok = $false when the tool reported an error, e.g. the
# endpoint is not listening yet).
function Invoke-McpTool($client, [string]$name, $arguments) {
  $result = Invoke-Mcp $client 'tools/call' @{ name = $name; arguments = $arguments } 15000
  $text = $result.content[0].text
  $isError = ($result.PSObject.Properties.Name -contains 'isError') -and $result.isError
  if ($isError) { return @{ ok = $false; value = $text } }
  return @{ ok = $true; value = ($text | ConvertFrom-Json) }
}

function Stop-McpClient($client) {
  if (-not $client) { return }
  try { $client.proc.StandardInput.Close() } catch { }
  if (-not $client.proc.WaitForExit(10000)) { try { $client.proc.Kill() } catch { } }
}

function Get-ProbeCpu($proc) { $proc.Refresh(); return $proc.TotalProcessorTime.TotalSeconds }

function Invoke-Run([string]$side, [string]$exe, [int]$index) {
  $runDir = Join-Path $script:RunsDir ('{0:D2}-{1}' -f $index, $side)
  $storage = Join-Path $runDir 'storage'
  New-Item -ItemType Directory -Force -Path $storage | Out-Null
  $stdout = Join-Path $runDir 'stdout.log'
  $stderr = Join-Path $runDir 'stderr.log'
  $run = [ordered]@{
    index = $index; side = $side; exit_code = $null; killed = $false; error = $null
    window_ms = $null; window_cpu_s = $null; start_ms = $null; start_cpu_s = $null; start_error = $null
    idle_wall_s = $null; idle_cpu_s = $null; idle_cpu_pct = $null; private_mb_end = $null
    adapter_backend = $null; frame_timings = $null; idle_tail_sample = $null; log_dir = $runDir
  }
  Write-Host ("[{0}] {1} ..." -f $index, $side)

  $port = $null; $token = $null; $mcp = $null
  if ($script:Sidecar) {
    $port = Get-FreePort
    $bytes = New-Object byte[] 16
    $rng = New-Object System.Security.Cryptography.RNGCryptoServiceProvider
    $rng.GetBytes($bytes); $rng.Dispose()
    $token = -join ($bytes | ForEach-Object { '{0:x2}' -f $_ })
  }

  # Same environment for every run: clear inherited probe/wgpu knobs, set ours.
  $saved = @{}
  Get-ChildItem Env: | Where-Object { $_.Name -match '^(ROVES_PROBE_|DENO_WEBGPU_|WGPU_)' } |
    ForEach-Object { $saved[$_.Name] = $_.Value; Remove-Item -LiteralPath ('Env:' + $_.Name) }
  $set = @{
    ROVES_PROBE_STORAGE_DIR = $storage; ROVES_PROBE_FRAME_TIMINGS = '1'
    DENO_WEBGPU_BACKEND = 'vulkan'; ROVES_PROBE_JITLESS = $(if ($Jitless) { '1' } else { '0' })
  }
  if ($port) { $set.ROVES_PROBE_DEV_MCP_ATTACH_PORT = "$port"; $set.ROVES_PROBE_DEV_MCP_TOKEN = $token }
  $proc = $null
  try {
    if ($script:Sidecar) { $mcp = Start-McpClient $port $token }  # does not contact the probe yet
    foreach ($k in $set.Keys) { Set-Item -LiteralPath ('Env:' + $k) -Value $set[$k] }
    $clock = [System.Diagnostics.Stopwatch]::StartNew()
    $proc = Start-Process -FilePath $exe -ArgumentList @('--game', ('"' + $script:Dist + '"'), '--window') `
      -WorkingDirectory (Split-Path $exe) -NoNewWindow -PassThru `
      -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    $null = $proc.Handle  # keep the handle so ExitCode stays readable
  } catch {
    Stop-McpClient $mcp
    throw
  } finally {
    foreach ($k in $set.Keys) { Remove-Item -LiteralPath ('Env:' + $k) -ErrorAction SilentlyContinue }
    foreach ($k in $saved.Keys) { Set-Item -LiteralPath ('Env:' + $k) -Value $saved[$k] }
  }

  try {
    while ($clock.Elapsed.TotalSeconds -lt $WindowTimeoutSeconds -and -not $proc.HasExited) {
      $proc.Refresh()
      if ($proc.MainWindowHandle -ne [IntPtr]::Zero) {
        $run.window_ms = [math]::Round($clock.Elapsed.TotalMilliseconds)
        $run.window_cpu_s = [math]::Round($proc.TotalProcessorTime.TotalSeconds, 3)
        break
      }
      [System.Threading.Thread]::Sleep(2)
    }
    if ($null -eq $run.window_ms) { throw "no window within $WindowTimeoutSeconds s (exited: $($proc.HasExited))" }
    $ready = $run.window_ms

    if ($mcp) {
      $want = ConvertTo-Json ($StartButtonText.Trim().ToLowerInvariant())
      $source = "(() => { const b = Array.from(document.querySelectorAll('button,[role=button],a')).find((e) => (e.textContent || '').trim().toLowerCase() === $want && !e.disabled); return { found: !!b, path: location.pathname }; })()"
      while ($true) {
        $reply = Invoke-McpTool $mcp 'evaluate_script' @{ source = $source }
        if ($reply.ok -and $reply.value.found) {
          $run.start_ms = [math]::Round($clock.Elapsed.TotalMilliseconds)
          $run.start_cpu_s = [math]::Round((Get-ProbeCpu $proc), 3)
          $ready = $run.start_ms
          break
        }
        $last = if ($reply.ok) { $reply.value | ConvertTo-Json -Compress } else { [string]$reply.value }
        if ($proc.HasExited -or $clock.Elapsed.TotalSeconds -ge $StartTimeoutSeconds) {
          $run.start_error = 'Start not available within ' + $StartTimeoutSeconds + ' s; last: ' + $last.Substring(0, [math]::Min(300, $last.Length))
          break
        }
        [System.Threading.Thread]::Sleep(100)
      }
    }

    $settleUntil = $ready / 1000 + $SettleSeconds
    while ($clock.Elapsed.TotalSeconds -lt $settleUntil -and -not $proc.HasExited) { [System.Threading.Thread]::Sleep(50) }
    if ($proc.HasExited) { throw 'probe exited before the idle interval' }
    $cpu0 = Get-ProbeCpu $proc; $t0 = $clock.Elapsed.TotalSeconds
    [System.Threading.Thread]::Sleep([int]($IdleSeconds * 1000))
    $cpu1 = Get-ProbeCpu $proc; $t1 = $clock.Elapsed.TotalSeconds
    if ($proc.HasExited) { throw 'probe exited during the idle interval' }
    $run.idle_wall_s = [math]::Round($t1 - $t0, 3)
    $run.idle_cpu_s = [math]::Round($cpu1 - $cpu0, 3)
    $run.idle_cpu_pct = [math]::Round(100 * ($cpu1 - $cpu0) / ($t1 - $t0), 2)
    $run.private_mb_end = [math]::Round($proc.PrivateMemorySize64 / 1MB, 1)

    if ($mcp) {
      try {
        $sample = Invoke-McpTool $mcp 'performance_sample' @{}
        $run.idle_tail_sample = $sample.value
      } catch { $run.idle_tail_sample = 'error: ' + $_.Exception.Message }
    }
  } catch {
    $run.error = $_.Exception.Message
  } finally {
    Stop-McpClient $mcp
    # Bounded cleanup of exactly the launched process.
    if (-not $proc.HasExited) {
      [void]$proc.CloseMainWindow()
      if (-not $proc.WaitForExit([int]($CloseTimeoutSeconds * 1000))) {
        $run.killed = $true
        try { $proc.Kill() } catch { }
        [void]$proc.WaitForExit(10000)
      }
    }
    if ($proc.HasExited) {
      $proc.WaitForExit()  # flushes the redirected output files
      $run.exit_code = $proc.ExitCode
    } else {
      $run.error = (@($run.error, 'probe still running after kill') | Where-Object { $_ }) -join '; '
    }
    $full = [System.IO.Path]::GetFullPath($storage)
    if ($full.StartsWith($script:RunsDir, [StringComparison]::OrdinalIgnoreCase)) {
      Remove-Item -LiteralPath $full -Recurse -Force -ErrorAction SilentlyContinue
    }
  }

  foreach ($line in [System.IO.File]::ReadAllLines($stdout)) {
    if ($line.StartsWith('ROVES_PROBE_FRAME_TIMINGS ')) {
      $run.frame_timings = $line.Substring(26) | ConvertFrom-Json
    } elseif ($null -eq $run.adapter_backend -and $line -match '"backend"\s*:\s*"([^"]+)"') {
      $run.adapter_backend = $Matches[1]
    }
  }
  if ($run.adapter_backend -and $run.adapter_backend -notmatch 'vulkan') {
    $run.error = (@($run.error, "adapter backend is $($run.adapter_backend), not Vulkan") | Where-Object { $_ }) -join '; '
  }
  if ($script:Sidecar -and $null -eq $run.start_ms) {
    $run.error = (@($run.error, $(if ($run.start_error) { $run.start_error } else { 'Start not measured' })) | Where-Object { $_ }) -join '; '
  }
  if ($run.exit_code -ne 0) {
    $run.error = (@($run.error, "exit code $($run.exit_code)") | Where-Object { $_ }) -join '; '
  }
  if (-not $run.frame_timings) {
    $run.error = (@($run.error, 'no ROVES_PROBE_FRAME_TIMINGS record') | Where-Object { $_ }) -join '; '
  }
  return [pscustomobject]$run
}

function Get-Series($run, [string]$series, [string]$field) {
  $ft = $run.frame_timings
  if (-not $ft -or -not ($ft.PSObject.Properties.Name -contains $series) -or -not $ft.$series) { return $null }
  $s = $ft.$series
  if ($s.PSObject.Properties.Name -contains $field) { return $s.$field }
  return $null
}

function Get-Counter($run, [string]$name) {
  $ft = $run.frame_timings
  if ($ft -and ($ft.PSObject.Properties.Name -contains $name)) { return $ft.$name }
  return $null
}

# --- setup ---
$script:BeforeExe = Resolve-Probe $Before
$script:AfterExe = Resolve-Probe $After
$script:Dist = (Get-Item -LiteralPath $GameDist).FullName
if (-not (Test-Path -LiteralPath (Join-Path $script:Dist 'index.html'))) { throw "No index.html in $GameDist" }
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$OutDir = (Get-Item -LiteralPath $OutDir).FullName
$script:RunsDir = Join-Path $OutDir 'runs'
if (Test-Path -LiteralPath $script:RunsDir) { throw "$script:RunsDir already exists; use a new -OutDir" }
New-Item -ItemType Directory -Path $script:RunsDir | Out-Null
$script:Sidecar = $null; $script:Node = $null
if ($McpSidecar) {
  $script:Sidecar = (Get-Item -LiteralPath $McpSidecar).FullName
  if ($NodeExe) { $script:Node = (Get-Item -LiteralPath $NodeExe).FullName }
  else {
    $bundled = Join-Path (Split-Path (Split-Path $script:Sidecar)) 'node\node.exe'
    if (Test-Path -LiteralPath $bundled) { $script:Node = $bundled }
    else { $script:Node = (Get-Command node -CommandType Application).Source }
  }
}

$gpu = @(Get-CimInstance Win32_VideoController | ForEach-Object { "$($_.Name) ($($_.DriverVersion))" })
$meta = [ordered]@{
  schema = 'roves-bench-startup-idle'; version = 1; started = (Get-Date).ToString('o')
  roves_before_rev = $RovesBeforeRev; roves_after_rev = $RovesAfterRev; template_rev = $TemplateRev
  before_exe = $script:BeforeExe; before_sha256 = (Get-FileHash -LiteralPath $script:BeforeExe).Hash
  after_exe = $script:AfterExe; after_sha256 = (Get-FileHash -LiteralPath $script:AfterExe).Hash
  game_dist = $script:Dist; pairs = $Pairs; jitless = [bool]$Jitless; backend = 'vulkan'
  settle_s = $SettleSeconds; idle_s = $IdleSeconds; mcp_sidecar = $script:Sidecar; node = $script:Node
  start_button_text = $StartButtonText
  machine = [ordered]@{
    cpu = (Get-CimInstance Win32_Processor | Select-Object -First 1).Name
    logical_cpus = [Environment]::ProcessorCount; gpu = $gpu
    os = (Get-CimInstance Win32_OperatingSystem).Version
  }
  notes = @(
    'window_ms: visible top-level window handle, not a usable menu; polling resolution ~15 ms.',
    'start_ms: enabled Start element observed through a scoped dev-mcp attach connection.',
    'idle_cpu_pct: probe CPU / wall clock over the idle interval, 100 = one logical CPU.',
    'frame_timings: whole session up to 3,600 samples; presented_frames are surface_present calls, not displayed frames; cpu_* are wall-clock, not GPU time.'
  )
}

$runs = New-Object System.Collections.Generic.List[object]
$index = 0
for ($pair = 1; $pair -le $Pairs; $pair++) {
  $order = if ($pair % 2) { @('before', 'after') } else { @('after', 'before') }
  foreach ($side in $order) {
    $index++
    $exe = if ($side -eq 'before') { $script:BeforeExe } else { $script:AfterExe }
    $r = Invoke-Run $side $exe $index
    $r | Add-Member -NotePropertyName pair -NotePropertyValue $pair
    $runs.Add($r)
    $status = if ($r.error) { 'ERROR: ' + $r.error } else { 'ok' }
    Write-Host ("    window {0} ms, cpu {1} s, start {2} ms, idle {3}% -> {4}" -f $r.window_ms, $r.window_cpu_s, $r.start_ms, $r.idle_cpu_pct, $status)
  }
}

# --- report ---
$rows = foreach ($r in $runs) {
  $causes = Get-Counter $r 'present_causes'
  [pscustomobject][ordered]@{
    run = $r.index; pair = $r.pair; side = $r.side; window_ms = $r.window_ms; window_cpu_s = $r.window_cpu_s
    start_ms = $r.start_ms; start_cpu_s = $r.start_cpu_s; idle_cpu_pct = $r.idle_cpu_pct; idle_cpu_s = $r.idle_cpu_s
    private_mb = $r.private_mb_end; presented = Get-Counter $r 'presented_frames'
    unchanged = Get-Counter $r 'unchanged_frames'; skipped = Get-Counter $r 'skipped_frames'
    cause_surface = $(if ($causes) { $causes.surface }); cause_scene = $(if ($causes) { $causes.scene })
    cause_canvas = $(if ($causes) { $causes.canvas }); cause_unretained = $(if ($causes) { $causes.unretained })
    uploads = Get-Counter $r 'raster_uploads'
    step_p99_ms = Get-Series $r 'frame_step_interval' 'p99'; step_max_ms = Get-Series $r 'frame_step_interval' 'max'
    step_over_16ms = Get-Series $r 'frame_step_interval' 'over_16_67ms'
    render_p99_ms = Get-Series $r 'cpu_render_present' 'p99'; render_over_16ms = Get-Series $r 'cpu_render_present' 'over_16_67ms'
    exit = $r.exit_code; killed = $r.killed; backend = $r.adapter_backend; error = $r.error
  }
}
$summary = foreach ($side in 'before', 'after') {
  $ok = @($rows | Where-Object { $_.side -eq $side -and -not $_.error })
  [pscustomobject][ordered]@{
    side = $side; ok_runs = $ok.Count
    window_ms = Get-Median @($ok | ForEach-Object { $_.window_ms })
    window_cpu_s = Get-Median @($ok | ForEach-Object { $_.window_cpu_s })
    start_ms = Get-Median @($ok | Where-Object { $null -ne $_.start_ms } | ForEach-Object { $_.start_ms })
    start_cpu_s = Get-Median @($ok | Where-Object { $null -ne $_.start_cpu_s } | ForEach-Object { $_.start_cpu_s })
    idle_cpu_pct = Get-Median @($ok | ForEach-Object { $_.idle_cpu_pct })
    step_p99_ms = Get-Median @($ok | Where-Object { $null -ne $_.step_p99_ms } | ForEach-Object { $_.step_p99_ms })
  }
}

$report = [ordered]@{ meta = $meta; summary_median = $summary; runs = $runs }
$jsonPath = Join-Path $OutDir 'results.json'
[System.IO.File]::WriteAllText($jsonPath, ($report | ConvertTo-Json -Depth 20), (New-Object System.Text.UTF8Encoding $false))
$csvPath = Join-Path $OutDir 'runs.csv'
$rows | Export-Csv -LiteralPath $csvPath -NoTypeInformation -Encoding UTF8

Write-Host ''
Write-Host "Roves before $RovesBeforeRev / after $RovesAfterRev, template $TemplateRev, jitless=$([bool]$Jitless), Vulkan"
$rows | Format-Table run, side, window_ms, window_cpu_s, start_ms, idle_cpu_pct, presented, unchanged, cause_scene, cause_canvas, step_p99_ms, step_over_16ms, exit, killed -AutoSize | Out-Host
Write-Host 'Median of error-free runs:'
$summary | Format-Table -AutoSize | Out-Host
Write-Host "Results: $jsonPath, $csvPath"
if (@($rows | Where-Object { $_.error }).Count) { Write-Warning 'Some runs reported errors; see results.json'; exit 1 }
