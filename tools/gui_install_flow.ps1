# Phase 3D-2: runs the real application window through the SYNTHETIC installation workflow and checks the isolated data directory.
#
#   powershell -File tools\gui_install_flow.ps1 -Exe <path to a debug build made with --features synthetic-pilot> [-Evidence <dir>]
#
# Safety: the application is started with an ISOLATED data directory (ATM_DATA_DIR); the only program started is the application itself;
# nothing installed in the workflow is ever executed; the only port opened is the WebView2 debugging port on 127.0.0.1 of this machine;
# no network is used. The application is stopped at the end of each phase (the first phase is ended by killing the process, on purpose).
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [string]$Evidence = (Join-Path $env:TEMP ("atm-gui-evidence-" + (Get-Random)))
)
$ErrorActionPreference = "Stop"
$data = Join-Path $env:TEMP ("atm-gui-3d2-" + (Get-Random))
New-Item -ItemType Directory -Path $data | Out-Null
New-Item -ItemType Directory -Path $Evidence -Force | Out-Null
$node = "C:\Program Files\nodejs\node.exe"
$driver = Join-Path $PSScriptRoot "gui_install_flow.mjs"
$env:ATM_DATA_DIR = $data
$env:EVIDENCE_DIR = $Evidence
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9222"
$exit = 0
foreach ($phase in @("first", "after-restart")) {
    $env:PHASE = $phase
    $p = Start-Process $Exe -PassThru
    Start-Sleep 9
    "[$phase] started pid=$($p.Id) alive=$(-not $p.HasExited)"
    & $node $driver
    if ($LASTEXITCODE -ne 0) { $exit = 1 }
    # the first phase ends by a hard kill (no clean shutdown), the second by the same: both leave the data directory as it is
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
    Start-Sleep 2
}
"EVIDENCE=$Evidence"
"DATA=$data"
exit $exit
