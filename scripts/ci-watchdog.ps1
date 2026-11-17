# ci-watchdog.ps1 -- a resource watchdog for the Windows test step of CI (issue #520).
#
# Why: the job "Rust (Windows, full workspace)" sometimes loses its runner ("The hosted runner lost communication
# with the server") in the middle of `cargo nextest`: no log, no artifact, the step timeout never fires, and the job's
# live log is not kept either (BlobNotFound after the loss, 2026-11-17 PR #541). So this script runs next to the tests
# and sends what it sees OFF the runner while the step runs:
#   - to the step's live log and to a file (`watchdog.log`, uploaded by the job when the runner survives);
#   - to a GitHub check run, "Windows test telemetry (#520)", updated every $PushEvery seconds (and at once on a
#     loud event) with the REST API and the job's GITHUB_TOKEN (`checks: write`, passed as WATCHDOG_TOKEN with the
#     commit as WATCHDOG_SHA). The check run lives on GitHub's servers, so its last update stays when the runner
#     dies. It is created already "completed / neutral" so a lost runner leaves nothing pending. A fork's pull
#     request has a read-only token: the first push fails and the script goes on with the log and the file only.
#
# What it records, every $Interval seconds:
#   - free physical memory, commit charge/limit, page file, handles, processes, CPU, disk queue and busy time, free
#     space of every disk (a full C: starves the runner agent), the top 8 processes by working set and by handles,
#     and the runner's own processes;
#   - the tests running now (nextest has no live list; every test is a process of its test binary run with
#     `--exact <name>`, so the process table names it, with its age), nextest's progress `n/N` from the tee'd
#     nextest.log, and how many rustc/link processes run (the build phase of the step);
#   - a loud line when Runner.Listener or Runner.Worker disappears, and a line for every process outside the
#     test tree (cargo / cargo-nextest and their children) that was alive in an earlier sample and is gone;
#   - the new WARN/ERR lines of the runner's own diagnostic logs (_diag), when they can be found;
#   - a guard rail: a process that runs from the workspace (a test binary, a daemon a test started) and holds more
#     than $KillOverGB of memory, or is the largest of them while free memory is under $KillBelowFreeGB, is
#     killed, so a runaway test fails (nextest reports the crash) instead of starving the runner.
# It samples every 3 s but only reads the process table through WMI, so it costs well under 1% of a CPU. It stops
# when $StopFile appears (the step creates it when the tests end) or after $MaxMinutes.

param(
    [int]$Interval = 15,
    [int]$PushEvery = 30,
    [string]$StopFile = 'watchdog.stop',
    [string]$LogFile = 'watchdog.log',
    [string]$NextestLog = 'nextest.log',
    [int]$MaxMinutes = 40,
    [string]$Workspace = $env:GITHUB_WORKSPACE,
    [double]$KillOverGB = 6,
    [double]$KillBelowFreeGB = 1
)

$ErrorActionPreference = 'Continue'
$tick = 3
$runnerNames = @('Runner.Listener.exe', 'Runner.Worker.exe')
$treeRoots = @('cargo-nextest.exe', 'cargo.exe')
# Images that come and go on a hosted runner all the time (seen in the first run of this script): not signal.
$ignoreGone = @('WmiPrvSE.exe', 'conhost.exe', 'pwsh.exe', 'powershell.exe', 'dllhost.exe', 'RuntimeBroker.exe',
    'MoUsoCoreWorker.exe', 'firefox.exe', 'crashhelper.exe', 'default-browser-agent.exe', 'wsl.exe', 'wslhost.exe',
    'TiWorker.exe', 'wermgr.exe', 'taskhostw.exe', 'backgroundTaskHost.exe', 'SearchProtocolHost.exe', 'SearchFilterHost.exe')
$gb = 1GB
$lines = New-Object 'System.Collections.Generic.List[string]'
$urgent = $false

function Say([string]$text) {
    $line = '{0:HH:mm:ss} [watchdog] {1}' -f (Get-Date), $text
    [Console]::Out.WriteLine($line)
    [Console]::Out.Flush()
    Add-Content -Path $LogFile -Value $line
    $lines.Add($line)
    if ($text.StartsWith('!')) { $script:urgent = $true }
}

function Perf([string]$class, [string]$filter) {
    try {
        if ($filter) { return Get-CimInstance -ClassName $class -Filter $filter -ErrorAction Stop }
        return Get-CimInstance -ClassName $class -ErrorAction Stop
    } catch { return $null }
}

function Gb([double]$bytes) { '{0:N1}' -f ($bytes / $gb) }
function Mb([double]$bytes) { '{0:N0}' -f ($bytes / 1MB) }

function Is-InTree([uint32]$procId, $byId) {
    for ($i = 0; $i -lt 40 -and $byId.ContainsKey($procId); $i++) {
        $p = $byId[$procId]
        if ($treeRoots -contains $p.Name) { return $true }
        $procId = $p.ParentProcessId
    }
    return $false
}

function Short($p, $byId) {
    $parent = if ($byId.ContainsKey([uint32]$p.ParentProcessId)) { $byId[[uint32]$p.ParentProcessId].Name } else { '?' }
    '{0}({1}<{2}:{3}) ws={4}MB h={5}' -f $p.Name, $p.ProcessId, $p.ParentProcessId, $parent, (Mb $p.WorkingSetSize), $p.HandleCount
}

# The runner's diagnostic logs: where the agent itself writes why it lost the server.
function Find-Diag($procs) {
    $dirs = @()
    foreach ($p in $procs | Where-Object { $runnerNames -contains $_.Name -and $_.ExecutablePath }) {
        $bin = Split-Path $p.ExecutablePath
        $dirs += (Join-Path (Split-Path $bin) '_diag'), (Join-Path (Split-Path (Split-Path $bin)) '_diag')
    }
    $dirs += 'C:\actions-runner\_diag', 'C:\actions-runner\cached\_diag'
    foreach ($d in $dirs) { if (Test-Path $d) { return $d } }
    return $null
}

$diagOffsets = @{}
function Tail-Diag([string]$dir) {
    if (-not $dir) { return }
    $shown = 0
    foreach ($f in Get-ChildItem -Path $dir -Filter '*.log' -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 3) {
        $fs = $null
        try {
            $fs = [IO.File]::Open($f.FullName, 'Open', 'Read', 'ReadWrite')
            if (-not $diagOffsets.ContainsKey($f.FullName)) { $diagOffsets[$f.FullName] = [Math]::Max(0, $fs.Length - 4096) }
            [void]$fs.Seek($diagOffsets[$f.FullName], 'Begin')
            $text = (New-Object IO.StreamReader($fs)).ReadToEnd()
            $diagOffsets[$f.FullName] = $fs.Length
        } catch { continue } finally { if ($fs) { $fs.Dispose() } }
        foreach ($l in $text -split "`r?`n") {
            if ($shown -lt 8 -and $l -match '\[(WARN|ERR)\s*\]|Exception|lost|heartbeat|timed out') {
                Say ('diag {0}: {1}' -f $f.Name, $l.Substring(0, [Math]::Min(220, $l.Length)))
                $shown++
            }
        }
    }
}

# The tests running now: each is a process of its test binary (`atlas_host-<hash>.exe crew::x --exact --nocapture`).
function Running-Tests($procs) {
    $now = Get-Date
    $found = @()
    foreach ($p in $procs) {
        if (-not ($Workspace -and $p.ExecutablePath -and $p.ExecutablePath.StartsWith((Join-Path $Workspace 'target'), 'OrdinalIgnoreCase'))) { continue }
        if ($p.Name -notmatch '-[0-9a-f]{16}\.exe$') { continue }
        $test = if ($p.CommandLine -and $p.CommandLine -match '--exact\s+"?([^\s"]+)') { $Matches[1] } elseif ($p.CommandLine -match '^\S+\s+"?([^\s"-][^\s"]*)') { $Matches[1] } else { '?' }
        $found += [pscustomobject]@{ Name = ($p.Name -replace '-[0-9a-f]{16}\.exe$', ''); Test = $test; Age = ($now - $p.CreationDate).TotalSeconds }
    }
    return @($found | Sort-Object Age -Descending)
}

# nextest's progress ("( 974/3057)") and its last line, from the tee'd log.
function Nextest-Progress {
    if (-not (Test-Path $NextestLog)) { return 'no nextest.log yet' }
    $fs = $null
    try {
        $fs = [IO.File]::Open((Resolve-Path $NextestLog).Path, 'Open', 'Read', 'ReadWrite')
        [void]$fs.Seek([Math]::Max(0, $fs.Length - 3000), 'Begin')
        $text = (New-Object IO.StreamReader($fs)).ReadToEnd() -replace "\x1b\[[0-9;]*m", ''
    } catch { return 'nextest.log unreadable' } finally { if ($fs) { $fs.Dispose() } }
    $ls = @($text -split "`r?`n" | Where-Object { $_.Trim() })
    $last = if ($ls.Count) { $ls[-1].Trim() } else { '' }
    $m = [regex]::Matches($text, '\(\s*(\d+)/(\d+)\)')
    $done = if ($m.Count) { '{0}/{1}' -f $m[$m.Count - 1].Groups[1].Value, $m[$m.Count - 1].Groups[2].Value } else { 'none finished yet' }
    '{0} | last line: {1}' -f $done, $last.Substring(0, [Math]::Min(150, $last.Length))
}

# Off the runner: a GitHub check run (see the header). `Fails` counts consecutive failures after it exists; a failure
# to create it (a fork's read-only token, no network) turns the remote off for good.
$api = if ($env:GITHUB_API_URL) { $env:GITHUB_API_URL } else { 'https://api.github.com' }
$remote = @{ Id = $null; Off = (-not ($env:WATCHDOG_TOKEN -and $env:WATCHDOG_SHA -and $env:GITHUB_REPOSITORY)); Fails = 0 }
$lastPush = [datetime]::MinValue
$runUrl = '{0}/{1}/actions/runs/{2}' -f $(if ($env:GITHUB_SERVER_URL) { $env:GITHUB_SERVER_URL } else { 'https://github.com' }), $env:GITHUB_REPOSITORY, $env:GITHUB_RUN_ID

function Remote-Call([string]$method, [string]$path, $body) {
    $h = @{ Authorization = "Bearer $env:WATCHDOG_TOKEN"; Accept = 'application/vnd.github+json'; 'X-GitHub-Api-Version' = '2022-11-28' }
    Invoke-RestMethod -Method $method -Uri "$api/repos/$env:GITHUB_REPOSITORY/$path" -Headers $h -ContentType 'application/json' `
        -Body ($body | ConvertTo-Json -Depth 6 -Compress) -TimeoutSec 8
}

function Push-Remote([string]$title, [string]$summary) {
    $script:lastPush = Get-Date
    $script:urgent = $false
    if ($remote.Off -or $remote.Fails -ge 5) { return }
    $take = @()
    $size = 0
    for ($i = $lines.Count - 1; $i -ge 0 -and $size -lt 60000; $i--) { $take += $lines[$i]; $size += $lines[$i].Length + 1 }
    [array]::Reverse($take)
    $output = @{ title = $title; summary = $summary; text = ('```' + "`n" + ($take -join "`n") + "`n" + '```') }
    try {
        if (-not $remote.Id) {
            $r = Remote-Call 'POST' 'check-runs' @{ name = 'Windows test telemetry (#520)'; head_sha = $env:WATCHDOG_SHA; status = 'completed'; conclusion = 'neutral'; output = $output }
            $remote.Id = $r.id
            Say ('remote: check run {0}' -f $r.html_url)
        } else {
            [void](Remote-Call 'PATCH' ('check-runs/{0}' -f $remote.Id) @{ output = $output })
        }
        $remote.Fails = 0
    } catch {
        if (-not $remote.Id) {
            $remote.Off = $true
            Say ('remote: no check run, live log and file only: {0}' -f $_.Exception.Message)
        } else {
            $remote.Fails++
            Say ('remote: update {0} failed: {1}' -f $remote.Fails, $_.Exception.Message)
        }
    }
}

$started = Get-Date
$os0 = Perf 'Win32_OperatingSystem'
$cpu0 = Perf 'Win32_ComputerSystem'
$all0 = @(Get-CimInstance Win32_Process)
$diagDir = Find-Diag $all0
Say ('start: {0} {1}, {2} logical CPUs, {3} GB RAM, workspace {4}, runner diag {5}, run {6}' -f `
    $os0.Caption, $os0.Version, $cpu0.NumberOfLogicalProcessors, (Gb ($os0.TotalVisibleMemorySize * 1KB)), $Workspace, $(if ($diagDir) { $diagDir } else { 'not found' }), $runUrl)
foreach ($pf in @(Perf 'Win32_PageFileUsage')) { Say ('page file {0}: {1} MB allocated' -f $pf.Name, $pf.AllocatedBaseSize) }
foreach ($d in @(Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=3')) { Say ('disk {0} {1} GB free of {2}' -f $d.DeviceID, (Gb $d.FreeSpace), (Gb $d.Size)) }
Push-Remote 'Windows test telemetry: started' ('Started; run {0}' -f $runUrl)

$prev = @{}
$n = 0
$headline = 'started'
while ($true) {
    if ((Test-Path $StopFile) -or ((Get-Date) - $started).TotalMinutes -ge $MaxMinutes) { break }
    try {
        $procs = @(Get-CimInstance Win32_Process -ErrorAction Stop)
        $byId = @{}
        foreach ($p in $procs) { $byId[[uint32]$p.ProcessId] = $p }
        $os = Perf 'Win32_OperatingSystem'
        $freeBytes = $os.FreePhysicalMemory * 1KB

        # Guard rail: a runaway process of ours fails by itself instead of taking the runner.
        if ($Workspace) {
            $mine = @($procs | Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith($Workspace, 'OrdinalIgnoreCase') } | Sort-Object WorkingSetSize -Descending)
            if ($mine.Count -gt 0) {
                $big = $mine[0]
                if ($big.WorkingSetSize -gt $KillOverGB * $gb -or ($freeBytes -lt $KillBelowFreeGB * $gb -and $big.WorkingSetSize -gt 0.5 * $gb)) {
                    Say ('!! KILL {0}: {1} GB working set, {2} GB physical memory free' -f (Short $big $byId), (Gb $big.WorkingSetSize), (Gb $freeBytes))
                    Stop-Process -Id $big.ProcessId -Force -ErrorAction SilentlyContinue
                }
            }
        }

        # Deaths: the runner's own processes, and long-lived processes outside the test tree.
        $cur = @{}
        foreach ($p in $procs) {
            $key = '{0}:{1}' -f $p.ProcessId, $p.CreationDate.Ticks
            $cur[$key] = @{ Row = $p; Tree = (Is-InTree ([uint32]$p.ProcessId) $byId); Born = $p.CreationDate }
        }
        foreach ($key in @($prev.Keys)) {
            if ($cur.ContainsKey($key)) { continue }
            $old = $prev[$key]
            $name = $old.Row.Name
            if ($runnerNames -contains $name) {
                Say ('!!! RUNNER PROCESS GONE: {0}' -f (Short $old.Row $byId))
            } elseif (-not $old.Tree -and ($ignoreGone -notcontains $name)) {
                Say ('gone outside the test tree: {0} lived {1:N0}s' -f (Short $old.Row $byId), ((Get-Date) - $old.Born).TotalSeconds)
            }
        }
        $prev = $cur

        $n++
        if (($n * $tick) % $Interval -lt $tick) {
            $mem = Perf 'Win32_PerfFormattedData_PerfOS_Memory'
            $sys = Perf 'Win32_PerfFormattedData_PerfOS_System'
            $cpu = Perf 'Win32_PerfFormattedData_PerfOS_Processor' "Name='_Total'"
            $disk = Perf 'Win32_PerfFormattedData_PerfDisk_PhysicalDisk' "Name='_Total'"
            $pf = @(Perf 'Win32_PageFileUsage') | Select-Object -First 1
            $free = (@(Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=3') | ForEach-Object { '{0} {1} GB' -f $_.DeviceID, (Gb $_.FreeSpace) }) -join ', '
            $handles = ($procs | Measure-Object -Property HandleCount -Sum).Sum
            $tests = @(Running-Tests $procs)
            $build = '{0} rustc, {1} link' -f @($procs | Where-Object Name -eq 'rustc.exe').Count, @($procs | Where-Object Name -eq 'link.exe').Count
            Say ('mem free {0}/{1} GB | commit {2}/{3} GB | pagefile {4}/{5} MB | handles {6} | procs {7} thr {8} | cpu {9}% queue {10} | disk queue {11} busy {12}% xfers/s {13} | free {14}' -f `
                (Gb $freeBytes), (Gb ($os.TotalVisibleMemorySize * 1KB)), (Gb $mem.CommittedBytes), (Gb $mem.CommitLimit), `
                $pf.CurrentUsage, $pf.AllocatedBaseSize, $handles, $procs.Count, $sys.Threads, `
                $cpu.PercentProcessorTime, $sys.ProcessorQueueLength, $disk.AvgDiskQueueLength, $disk.PercentDiskTime, $disk.DiskTransfersPersec, $free)
            Say ('nextest: {0} | building: {1}' -f (Nextest-Progress), $build)
            Say ('tests running ({0}): {1}' -f $tests.Count, $(if ($tests) { (($tests | Select-Object -First 10 | ForEach-Object { '{0} {1} {2:N0}s' -f $_.Name, $_.Test, $_.Age }) -join '; ') } else { 'none' }))
            $runner = @($procs | Where-Object { $runnerNames -contains $_.Name })
            Say ('runner: {0}' -f $(if ($runner) { ($runner | ForEach-Object { Short $_ $byId }) -join '; ' } else { 'NO RUNNER PROCESS' }))
            Say ('top ws: {0}' -f (($procs | Sort-Object WorkingSetSize -Descending | Select-Object -First 8 | ForEach-Object { Short $_ $byId }) -join '; '))
            Say ('top handles: {0}' -f (($procs | Sort-Object HandleCount -Descending | Select-Object -First 8 | ForEach-Object { Short $_ $byId }) -join '; '))
            Tail-Diag $diagDir
            $headline = 'sample {0:HH:mm:ss}: mem free {1} GB, disk busy {2}%, {3} tests running, nextest {4}' -f (Get-Date), (Gb $freeBytes), $disk.PercentDiskTime, $tests.Count, ((Nextest-Progress) -split ' ')[0]
        }
        if ($urgent -or ((Get-Date) - $lastPush).TotalSeconds -ge $PushEvery) {
            Push-Remote ('Windows test telemetry: ' + $headline) ('{0}. Last update {1:HH:mm:ss} UTC, run {2}. The log below is the tail of `watchdog.log`.' -f $headline, (Get-Date).ToUniversalTime(), $runUrl)
        }
    } catch {
        Say ('sample failed: {0}' -f $_.Exception.Message)
    }
    Start-Sleep -Seconds $tick
}
Say ('stop after {0:N0}s' -f ((Get-Date) - $started).TotalSeconds)
Push-Remote ('Windows test telemetry: finished, last ' + $headline) ('Finished (the tests ended or the time ran out). Last: {0}. Run {1}.' -f $headline, $runUrl)
