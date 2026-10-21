# Phase 3D-6: a READ-ONLY observer around one real-window run (tools/gui_jq_pilot_locked.ps1). It changes nothing: it only reads.
#
#   powershell -File observe-gui-run.ps1 -Exe <debug jq-pilot build> -Evidence <dir>
#
# Before and after the run it records (and compares): the user and machine PATH, the Run / Environment / App Paths registry keys, the Smart App Control
# policy state, the Defender exclusion and monitoring preferences (when readable without elevation), the number of Start-menu and Desktop shortcuts,
# and the hash of the agent settings file. During the run it samples, every 400 ms, the TCP/UDP endpoints owned by the application and its WebView2
# processes and records every REMOTE address that is not this machine. It never starts anything but the existing GUI driver, never writes to the registry,
# PATH or any setting, and never touches the pilot data directory (the run uses its own temporary directory).
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$Evidence
)
$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Path $Evidence -Force | Out-Null

function Read-Key([string]$path) {
    try { (Get-ItemProperty -LiteralPath $path -ErrorAction Stop | Select-Object * -ExcludeProperty PS* | ConvertTo-Json -Depth 3 -Compress) } catch { "unreadable-or-absent" }
}
function Snapshot {
    $o = [ordered]@{}
    $o.userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $o.machinePath = [Environment]::GetEnvironmentVariable("Path", "Machine")
    foreach ($k in 'HKCU:\Environment', 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run', 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run', 'HKCU:\Software\Microsoft\Windows\CurrentVersion\App Paths', 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths') {
        $o["reg:$k"] = Read-Key $k
    }
    $o.smartAppControlPolicyState = try { (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy' -ErrorAction Stop).VerifiedAndReputablePolicyState } catch { "unreadable" }
    $o.defender = try { Get-MpPreference -ErrorAction Stop | Select-Object ExclusionPath, ExclusionProcess, ExclusionExtension, DisableRealtimeMonitoring, MAPSReporting, SubmitSamplesConsent | ConvertTo-Json -Compress } catch { "unreadable-without-elevation" }
    $o.startMenuShortcuts = @(Get-ChildItem "$env:APPDATA\Microsoft\Windows\Start Menu\Programs" -Recurse -Filter *.lnk -ErrorAction SilentlyContinue).Count
    $o.desktopShortcuts = @(Get-ChildItem ([Environment]::GetFolderPath("Desktop")) -Filter *.lnk -ErrorAction SilentlyContinue).Count
    $agent = Join-Path $env:USERPROFILE ".claude\settings.json"
    $o.agentSettingsSha256 = if (Test-Path -LiteralPath $agent) { (Get-FileHash -LiteralPath $agent -Algorithm SHA256).Hash.ToLower() } else { "absent" }
    $o.jqOnPath = [bool](Get-Command jq, jq.exe -ErrorAction SilentlyContinue)
    $o
}

$before = Snapshot
($before | ConvertTo-Json -Depth 4) | Set-Content -Path (Join-Path $Evidence "environment-before.json") -Encoding UTF8

$stop = Join-Path $Evidence ".stop-sampler"
Remove-Item $stop -ErrorAction SilentlyContinue
$netOut = Join-Path $Evidence "network-samples.txt"
$sampler = Start-Job -ArgumentList $stop, $netOut, $Exe -ScriptBlock {
    param($stop, $netOut, $exe)
    $seen = @{}
    while (-not (Test-Path $stop)) {
        # ONLY this application and the processes below it (its own WebView2 tree); other programs' WebView2 processes on the machine are not ours
        $roots = @(Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exe } | ForEach-Object { $_.Id })
        $pids = @($roots)
        if ($roots.Count) {
            $all = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | Select-Object ProcessId, ParentProcessId, Name, CommandLine)
            do { $add = @($all | Where-Object { $pids -contains $_.ParentProcessId -and $pids -notcontains $_.ProcessId } | ForEach-Object { $_.ProcessId }); $pids += $add } while ($add.Count)
        }
        if ($pids.Count) {
            $role = @{}
            foreach ($x in @($all | Where-Object { $pids -contains $_.ProcessId })) {
                $cl = [string]$x.CommandLine
                $role[[int]$x.ProcessId] = if ($x.Name -match '^ai-toolchain-manager') { 'app' }
                    elseif ($cl -match '--utility-sub-type=network\.mojom\.NetworkService') { 'webview2-network-service' }
                    elseif ($cl -match '--type=renderer') { 'webview2-renderer' }
                    elseif ($cl -match '--type=([a-z-]+)') { 'webview2-' + $Matches[1] }
                    elseif ($x.Name -match 'msedgewebview2') { 'webview2-browser' }
                    else { $x.Name }
            }
            foreach ($c in @(Get-NetTCPConnection -ErrorAction SilentlyContinue | Where-Object { $pids -contains $_.OwningProcess })) {
                $name = $role[[int]$c.OwningProcess]
                $key = "tcp $name $($c.State) local=$($c.LocalAddress):$($c.LocalPort) remote=$($c.RemoteAddress):$($c.RemotePort)"
                $seen[$key] = 1
            }
            foreach ($u in @(Get-NetUDPEndpoint -ErrorAction SilentlyContinue | Where-Object { $pids -contains $_.OwningProcess })) {
                $name = $role[[int]$u.OwningProcess]
                $seen["udp $name local=$($u.LocalAddress):$($u.LocalPort)"] = 1
            }
        }
        Start-Sleep -Milliseconds 400
    }
    $seen.Keys | Sort-Object | Set-Content -Path $netOut -Encoding UTF8
}

$driver = Join-Path (Split-Path (Split-Path (Split-Path $PSScriptRoot -Parent) -Parent) -Parent) "tools\gui_jq_pilot_locked.ps1"
$log = & powershell -NoProfile -File $driver -Exe $Exe -Evidence (Join-Path $Evidence "gui") 2>&1 | ForEach-Object { [string]$_ }
$guiExit = $LASTEXITCODE
$log | Set-Content -Path (Join-Path $Evidence "gui-run-log.txt") -Encoding UTF8
$log | Select-Object -Last 8

New-Item -ItemType File -Path $stop -Force | Out-Null
Wait-Job $sampler -Timeout 20 | Out-Null
Remove-Job $sampler -Force
Remove-Item $stop -ErrorAction SilentlyContinue

$after = Snapshot
($after | ConvertTo-Json -Depth 4) | Set-Content -Path (Join-Path $Evidence "environment-after.json") -Encoding UTF8
$same = (($before | ConvertTo-Json -Depth 4) -eq ($after | ConvertTo-Json -Depth 4))
$samples = if (Test-Path $netOut) { @(Get-Content $netOut | ForEach-Object { [string]$_ }) } else { @() }
$remote = @($samples | Where-Object { $_ -match '^tcp ' -and $_ -notmatch 'remote=(127\.0\.0\.1|::1|0\.0\.0\.0|::):' } | ForEach-Object { [string]$_ })
$byRole = [ordered]@{
    appProcess = @($remote | Where-Object { $_ -match '^tcp app ' })
    webview2Renderer = @($remote | Where-Object { $_ -match '^tcp webview2-renderer ' })
    webview2NetworkService = @($remote | Where-Object { $_ -match '^tcp webview2-network-service ' })
    webview2BrowserAndOtherRuntimeProcesses = @($remote | Where-Object { $_ -notmatch '^tcp (app|webview2-renderer|webview2-network-service) ' })
}
[ordered]@{
    guiExit = $guiExit
    environmentIdenticalBeforeAndAfter = $same
    networkSamples = $samples.Count
    remoteTcpEndpointsObservedByRole = $byRole
    note = "The application's own process and the renderer that hosts its page must show none. Connections of the Microsoft WebView2 runtime processes (browser, network service) are the runtime's own, not initiated by the application's code (the CSP allows only ipc; the sources contain no network API); they are recorded, not hidden."
} | ConvertTo-Json -Depth 3 | Set-Content -Path (Join-Path $Evidence "observation-summary.json") -Encoding UTF8
Get-Content (Join-Path $Evidence "observation-summary.json")
exit $guiExit
