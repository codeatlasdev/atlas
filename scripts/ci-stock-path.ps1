# ci-stock-path.ps1 -- prints PATH as a stock Windows has it: without any directory that holds sh, bash or sleep
# (Git's usr\bin and bin, MSYS2, the runner image's own copies). Issue #260: the Rust tests must pass from a plain
# PowerShell, where `sh` and `sleep` do not exist; a test that spawns them bare fails there with "program not found",
# and on the runner (Git Bash first on PATH) it would never show. The Windows test step runs nextest with this PATH.
$unix = 'sh.exe', 'bash.exe', 'sleep.exe'
$keep = foreach ($dir in ($env:PATH -split ';')) {
    if (-not $dir) { continue }
    if ($unix | Where-Object { Test-Path -LiteralPath (Join-Path $dir $_) }) { continue }
    $dir
}
$stock = $keep -join ';'
# Fail loudly if the filter did not do its job, so the proof cannot pass by accident.
$env:PATH = $stock
foreach ($tool in 'sh', 'bash', 'sleep') {
    if (Get-Command $tool -CommandType Application -ErrorAction SilentlyContinue) {
        [Console]::Error.WriteLine("ci-stock-path: '$tool' still resolves on the stock PATH")
        exit 1
    }
}
[Console]::Out.Write($stock)
