# Execute the Windows release artifacts the way a user does, on a native Windows runner, and FAIL when one is
# broken (#312). Run by release.yml `verify-windows` before `publish`; also runnable by hand on Windows.
#
#   scripts/windows-launch-check.ps1 -Dist <dir with the release files> -Out <evidence dir> [-Version X.Y.Z]
#
# What it executes, in order (each failure is `FAIL <tag>: <detail>`, listed in <Out>\result.txt, exit code 1):
#   checksum        every <file>.sha256 in -Dist matches the file
#   cli-version     atlas-<v>-x86_64-pc-windows-msvc.tar.gz unpacks and `atlas.exe --version` runs (names -Version when given)
#   setup-install   Atlas-Setup.exe --silent --no-launch exits 0 and leaves atlas.exe and atlas-desktop.exe in
#                   %LOCALAPPDATA%\Atlas
#   app-*           atlas-desktop.exe (the installed one) opens a window, stays up, paints something (not one flat
#                   colour) and writes nothing to ~\.atlas\desktop\app.log (every line there is an incident)
#   setup-uninstall Atlas-Setup.exe --uninstall --silent --remove-data removes the install folder
#   nsis-install    the plain NSIS installer (`atlas desktop` and `atlas update` run it) /S installs the same files,
#                   and the app opens again
#   nsis-uninstall  its uninstall.exe /S removes them
#
# `-Negative` runs the launch check against a program that exits at once and EXPECTS it to fail: the check can fail.
param(
  [Parameter(Mandatory)] [string] $Dist,
  [Parameter(Mandatory)] [string] $Out,
  [string] $Version = '',
  [switch] $Negative,
  [int] $WindowTimeout = 90,
  [int] $Settle = 8
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms

New-Item -ItemType Directory -Force -Path $Out | Out-Null
$Out = (Resolve-Path $Out).Path
$failures = New-Object System.Collections.Generic.List[string]
function Fail([string] $tag, [string] $detail) {
  $line = "FAIL ${tag}: $detail"
  Write-Host "::error::$line"
  $script:failures.Add($line)
}
function Step([string] $name) { Write-Host "`n=== $name" }

$installDir = Join-Path $env:LOCALAPPDATA 'Atlas'
$atlasHome = Join-Path $env:USERPROFILE '.atlas'
$appLog = Join-Path $atlasHome 'desktop\app.log'

# Every Atlas process this check may have started (the app and its hostd).
function Stop-Atlas {
  Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.ProcessName -in @('atlas-desktop', 'atlas', 'Atlas-Setup') } |
    ForEach-Object { try { $_.Kill() } catch {} }
  Start-Sleep -Seconds 2
}

# One screenshot of the whole desktop; returns the number of distinct colours in a sample of its pixels.
function Take-Screenshot([string] $path) {
  $b = [System.Windows.Forms.SystemInformation]::VirtualScreen
  $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($b.Left, $b.Top, 0, 0, $bmp.Size)
  $seen = New-Object 'System.Collections.Generic.HashSet[int]'
  $stepX = [Math]::Max(1, [int]($bmp.Width / 64)); $stepY = [Math]::Max(1, [int]($bmp.Height / 64))
  for ($y = 0; $y -lt $bmp.Height; $y += $stepY) { for ($x = 0; $x -lt $bmp.Width; $x += $stepX) { [void] $seen.Add($bmp.GetPixel($x, $y).ToArgb()) } }
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  return $seen.Count
}

# Open $exe and check what a first user would see. $tag prefixes the failure tags.
function Check-Launch([string] $tag, [string] $exe) {
  Remove-Item -Force -ErrorAction SilentlyContinue $appLog
  $p = Start-Process -FilePath $exe -PassThru
  $deadline = (Get-Date).AddSeconds($WindowTimeout)
  $mapped = $false
  while ((Get-Date) -lt $deadline) {
    if ($p.HasExited) { Fail "$tag-exited" "$exe ended with code $($p.ExitCode) before a window opened"; return }
    $p.Refresh()
    if ($p.MainWindowHandle -ne 0) { $mapped = $true; break }
    Start-Sleep -Milliseconds 500
  }
  if (-not $mapped) { Fail "$tag-no-window" "no window within ${WindowTimeout}s"; Stop-Atlas; return }
  # The page needs time to paint after the window exists; then the app must stay up and quiet.
  Start-Sleep -Seconds $Settle
  $p.Refresh()
  if ($p.HasExited) { Fail "$tag-exited" "the app ended with code $($p.ExitCode) after its window opened"; return }
  $colours = Take-Screenshot (Join-Path $Out "$tag.png")
  Write-Host "$tag screenshot: $colours distinct colours in the sample"
  $web = @(Get-Process -Name msedgewebview2 -ErrorAction SilentlyContinue).Count
  if ($web -eq 0) { Fail "$tag-no-webview" 'no msedgewebview2 process: the page has no WebView2' }
  if ($colours -lt 8) { Fail "$tag-blank-page" "the screen is $colours colours: the page never painted" }
  if (Test-Path $appLog) {
    Copy-Item $appLog (Join-Path $Out "$tag-app.log")
    $lines = @(Get-Content $appLog | Where-Object { $_.Trim() })
    if ($lines.Count -gt 0) {
      $head = ($lines | Select-Object -First 5 | ForEach-Object { $_.Substring(0, [Math]::Min(240, $_.Length)) }) -join ' | '
      Fail "$tag-app-log" $head
    }
  }
  Stop-Atlas
}

if ($Negative) {
  # A program that exits at once is not an app that opened: the check has to say so.
  Step 'negative control'
  Check-Launch 'negative' (Join-Path $env:SystemRoot 'System32\whoami.exe')
  if ($failures.Count -eq 0) { Write-Host '::error::the launch check passed a program that is not an app'; exit 1 }
  Write-Host "the check failed as it must: $($failures[0])"
  exit 0
}

$v = $Version
$target = 'x86_64-pc-windows-msvc'
$cliTar = Get-ChildItem $Dist -Filter "atlas-*-$target.tar.gz" | Select-Object -First 1
$nsis = Get-ChildItem $Dist -Filter "Atlas-*-$target-setup.exe" | Select-Object -First 1
$setup = Join-Path $Dist 'Atlas-Setup.exe'
foreach ($f in @($cliTar, $nsis)) { if (-not $f) { Write-Host '::error::a release file is missing from the artifact'; exit 1 } }
if (-not (Test-Path $setup)) { Write-Host '::error::Atlas-Setup.exe is missing from the artifact'; exit 1 }

Step 'checksum'
foreach ($sum in Get-ChildItem $Dist -Filter '*.sha256') {
  $want, $name = (Get-Content $sum.FullName -TotalCount 1) -split '\s+', 2
  $file = Join-Path $Dist ($name.TrimStart('*').Trim())
  if (-not (Test-Path $file)) { Fail 'checksum' "$($sum.Name) names a file that is not there"; continue }
  $got = (Get-FileHash $file -Algorithm SHA256).Hash.ToLower()
  if ($got -ne $want.ToLower()) { Fail 'checksum' "$(Split-Path -Leaf $file) is $got, expected $want" }
}

Step 'the CLI archive'
$cliDir = Join-Path $Out 'cli'
New-Item -ItemType Directory -Force -Path $cliDir | Out-Null
tar -xzf $cliTar.FullName -C $cliDir
$ver = (& (Join-Path $cliDir 'atlas.exe') --version) -join ' '
Write-Host $ver
if ($LASTEXITCODE -ne 0) { Fail 'cli-version' "atlas.exe --version exited $LASTEXITCODE" }
elseif ($v -and ($ver -notmatch [regex]::Escape($v))) { Fail 'cli-version' "atlas.exe says '$ver', expected $v" }

function Check-Installed([string] $tag) {
  foreach ($f in 'atlas.exe', 'atlas-desktop.exe') {
    if (-not (Test-Path (Join-Path $installDir $f))) { Fail "$tag-files" "$f is not in $installDir" }
  }
}

Step 'Atlas-Setup.exe, silently'
Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $installDir
$p = Start-Process -FilePath $setup -ArgumentList '--silent', '--no-launch' -Wait -PassThru
if ($p.ExitCode -ne 0) { Fail 'setup-install' "Atlas-Setup.exe --silent exited $($p.ExitCode)" }
Check-Installed 'setup'
if (Test-Path (Join-Path $installDir 'atlas-desktop.exe')) { Check-Launch 'app-setup' (Join-Path $installDir 'atlas-desktop.exe') }

Step 'Atlas-Setup.exe --uninstall'
$uninstaller = Join-Path $installDir 'Atlas Uninstall.exe'
if (-not (Test-Path $uninstaller)) { Fail 'setup-uninstall' "no Atlas Uninstall.exe in $installDir" }
else {
  $p = Start-Process -FilePath $uninstaller -ArgumentList '--uninstall', '--silent', '--remove-data' -Wait -PassThru
  if ($p.ExitCode -ne 0) { Fail 'setup-uninstall' "uninstall exited $($p.ExitCode)" }
  if (Test-Path (Join-Path $installDir 'atlas-desktop.exe')) { Fail 'setup-uninstall' 'atlas-desktop.exe is still installed' }
}

Step 'the NSIS installer, silently (what atlas desktop and atlas update run)'
Stop-Atlas
$p = Start-Process -FilePath $nsis.FullName -ArgumentList '/S' -Wait -PassThru
if ($p.ExitCode -ne 0) { Fail 'nsis-install' "$($nsis.Name) /S exited $($p.ExitCode)" }
Check-Installed 'nsis'
if (Test-Path (Join-Path $installDir 'atlas.exe')) {
  $ver = (& (Join-Path $installDir 'atlas.exe') --version) -join ' '
  Write-Host "installed: $ver"
  if ($v -and ($ver -notmatch [regex]::Escape($v))) { Fail 'nsis-version' "the installed atlas.exe says '$ver', expected $v" }
}
if (Test-Path (Join-Path $installDir 'atlas-desktop.exe')) { Check-Launch 'app-nsis' (Join-Path $installDir 'atlas-desktop.exe') }

Step 'the NSIS uninstaller'
$un = Join-Path $installDir 'uninstall.exe'
if (-not (Test-Path $un)) { Fail 'nsis-uninstall' "no uninstall.exe in $installDir" }
else {
  $p = Start-Process -FilePath $un -ArgumentList '/S' -Wait -PassThru
  if ($p.ExitCode -ne 0) { Fail 'nsis-uninstall' "uninstall.exe /S exited $($p.ExitCode)" }
  # The NSIS uninstaller copies itself and returns early; give it a moment to finish.
  for ($i = 0; $i -lt 30 -and (Test-Path (Join-Path $installDir 'atlas-desktop.exe')); $i++) { Start-Sleep -Seconds 1 }
  if (Test-Path (Join-Path $installDir 'atlas-desktop.exe')) { Fail 'nsis-uninstall' 'atlas-desktop.exe is still installed' }
}

$result = if ($failures.Count -eq 0) { 'PASS' } else { 'FAIL' }
@($result) + $failures | Set-Content (Join-Path $Out 'result.txt')
Write-Host "`n$result"
if ($failures.Count -gt 0) { exit 1 }
