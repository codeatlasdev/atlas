# ci-watchdog.ps1 -- a resource watchdog for the Windows test step of CI (issue #520).
#
# Why: the job "Rust (Windows, full workspace)" sometimes loses its runner ("The hosted runner lost communication
# with the server") in the middle of `cargo nextest`: no log, no artifact, the step timeout never fires. This
# script runs next to the tests and prints to the step's LIVE log (which usually survives a lost runner better than
# an artifact) and to a file:
#   - every $Interval seconds: free physical memory, commit charge/limit, page file, handles, processes, CPU, disk
#     queue, the top 8 processes by working set and by handles, and the runner's own processes;
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
    [string]$StopFile = 'watchdog.stop',
    [string]$LogFile = 'watchdog.log',
    [int]$MaxMinutes = 40,
    [string]$Workspace = $env:GITHUB_WORKSPACE,
    [double]$KillOverGB = 6,
    [double]$KillBelowFreeGB = 1
)

$ErrorActionPreference = 'Continue'
$tick = 3
$runnerNames = @('Runner.Listener.exe', 'Runner.Worker.exe')
$treeRoots = @('cargo-nextest.exe', 'cargo.exe')
$ignoreGone = @('WmiPrvSE.exe', 'conhost.exe', 'pwsh.exe', 'powershell.exe')
$gb = 1GB

function Say([string]$text) {
    $line = '{0:HH:mm:ss} [watchdog] {1}' -f (Get-Date), $text
    [Console]::Out.WriteLine($line)
    [Console]::Out.Flush()
    Add-Content -Path $LogFile -Value $line
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

$started = Get-Date
$os0 = Perf 'Win32_OperatingSystem'
$cpu0 = Perf 'Win32_ComputerSystem'
$all0 = @(Get-CimInstance Win32_Process)
$diagDir = Find-Diag $all0
Say ('start: {0}, {1} logical CPUs, {2} GB RAM, workspace {3}, runner diag {4}' -f `
    $os0.Caption, $cpu0.NumberOfLogicalProcessors, (Gb ($os0.TotalVisibleMemorySize * 1KB)), $Workspace, $(if ($diagDir) { $diagDir } else { 'not found' }))
foreach ($pf in @(Perf 'Win32_PageFileUsage')) { Say ('page file {0}: {1} MB allocated' -f $pf.Name, $pf.AllocatedBaseSize) }
foreach ($d in @(Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=3')) { Say ('disk {0} {1} GB free of {2}' -f $d.DeviceID, (Gb $d.FreeSpace), (Gb $d.Size)) }

$prev = @{}
$n = 0
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
            $handles = ($procs | Measure-Object -Property HandleCount -Sum).Sum
            Say ('mem free {0}/{1} GB | commit {2}/{3} GB | pagefile {4}/{5} MB | handles {6} | procs {7} thr {8} | cpu {9}% queue {10} | disk queue {11} busy {12}% xfers/s {13}' -f `
                (Gb $freeBytes), (Gb ($os.TotalVisibleMemorySize * 1KB)), (Gb $mem.CommittedBytes), (Gb $mem.CommitLimit), `
                $pf.CurrentUsage, $pf.AllocatedBaseSize, $handles, $procs.Count, $sys.Threads, `
                $cpu.PercentProcessorTime, $sys.ProcessorQueueLength, $disk.AvgDiskQueueLength, $disk.PercentDiskTime, $disk.DiskTransfersPersec)
            $runner = @($procs | Where-Object { $runnerNames -contains $_.Name })
            Say ('runner: {0}' -f $(if ($runner) { ($runner | ForEach-Object { Short $_ $byId }) -join '; ' } else { 'NO RUNNER PROCESS' }))
            Say ('top ws: {0}' -f (($procs | Sort-Object WorkingSetSize -Descending | Select-Object -First 8 | ForEach-Object { Short $_ $byId }) -join '; '))
            Say ('top handles: {0}' -f (($procs | Sort-Object HandleCount -Descending | Select-Object -First 8 | ForEach-Object { Short $_ $byId }) -join '; '))
            Tail-Diag $diagDir
        }
    } catch {
        Say ('sample failed: {0}' -f $_.Exception.Message)
    }
    Start-Sleep -Seconds $tick
}
Say ('stop after {0:N0}s' -f ((Get-Date) - $started).TotalSeconds)
