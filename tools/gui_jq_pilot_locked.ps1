# Phase 3D-4 (B2): runs the real application window of the jq-pilot DEBUG build (never a release build) against an ISOLATED data directory and checks
# that the real jq installation is locked: the jq row is listed, cannot be previewed or confirmed, and nothing but the application's own database
# files changes on disk. There is no jq file in this run; nothing is installed; nothing installed would ever be executed.
#
#   powershell -File tools\gui_jq_pilot_locked.ps1 -Exe <path to a debug build made with --features jq-pilot> [-Evidence <dir>]
#
# Safety: ATM_DATA_DIR points at a fresh temporary directory; the only program started is the application itself (and node, for the driver); the only
# port opened by the application is the WebView2 debugging port on 127.0.0.1 of this machine; the application's own code uses no network (its page may only reach its IPC), but the Microsoft WebView2 runtime hosted in the window makes its own outbound connections (docs/phase-3d6/NETWORK-OBSERVATION.md). The application is stopped at the end.
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [string]$Evidence = (Join-Path $env:TEMP ("atm-gui-jq-pilot-" + (Get-Random)))
)
$ErrorActionPreference = "Stop"
$data = Join-Path $env:TEMP ("atm-gui-3d4-" + (Get-Random))
New-Item -ItemType Directory -Path $data | Out-Null
New-Item -ItemType Directory -Path $Evidence -Force | Out-Null
$node = "C:\Program Files\nodejs\node.exe"
$driver = Join-Path $PSScriptRoot "gui_jq_pilot_locked.mjs"
$env:ATM_DATA_DIR = $data
$env:EVIDENCE_DIR = $Evidence
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9222"
$p = Start-Process $Exe -PassThru
Start-Sleep 9
"started pid=$($p.Id) alive=$(-not $p.HasExited)"
& $node $driver
$exit = $LASTEXITCODE
Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
Start-Sleep 2
"EVIDENCE=$Evidence"
"DATA=$data"
exit $exit
