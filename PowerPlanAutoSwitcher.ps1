Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
[System.Windows.Forms.Application]::EnableVisualStyles()

$ErrorActionPreference = "Stop"

$mutexName = "Global\PowerPlanAutoSwitcher"
$createdNew = $false
$mutex = New-Object System.Threading.Mutex($true, $mutexName, [ref]$createdNew)
if (-not $createdNew) {
    exit
}

$Config = [ordered]@{
    PollIntervalMilliseconds = 500
    HoldPerformanceSeconds = 25
    SanityCheckSeconds = 10
    BalancedPlanGuid = "381b4222-f694-41f0-9685-ff5bb260df2e"
    PerformancePlanGuid = "248d6269-383f-4dd1-964a-90c3cfb36e8f"
    BalancedPlanName = "Balanced"
    PerformancePlanName = "Ultra Performance"
    TargetProcesses = @(
        "cmake.exe",
        "msbuild.exe",
        "devenv.exe",
        "ninja.exe",
        "cl.exe",
        "link.exe",
        "dotnet.exe",
        "wow.exe"
    )
    LogPath = "$env:ProgramData\PowerPlanAutoSwitcher\switcher.log"
    ShowBalloonOnPerformanceSwitch = $true
    ShowBalloonOnBalancedSwitch = $false
}

$logDir = Split-Path $Config.LogPath -Parent
if (-not (Test-Path $logDir)) {
    New-Item -ItemType Directory -Path $logDir -Force | Out-Null
}

function Write-Log {
    param([string]$Message)
    $line = "[{0}] {1}" -f (Get-Date).ToString("yyyy-MM-dd HH:mm:ss.fff"), $Message
    Add-Content -Path $Config.LogPath -Value $line
}

function Get-ActivePowerSchemeGuid {
    $output = & powercfg /getactivescheme 2>$null
    if ($LASTEXITCODE -ne 0) {
        return $null
    }
    if (($output -join ' ') -match '([A-Fa-f0-9\-]{36})') {
        return $matches[1].ToLower()
    }
    return $null
}

function Get-PlanDisplayName {
    param([string]$Guid)

    if (-not $Guid) {
        return "Unknown"
    }

    $guidLower = $Guid.ToLower()

    if ($guidLower -eq $Config.BalancedPlanGuid.ToLower()) {
        return $Config.BalancedPlanName
    }

    if ($guidLower -eq $Config.PerformancePlanGuid.ToLower()) {
        return $Config.PerformancePlanName
    }

    $listOutput = & powercfg /list 2>$null
    foreach ($line in $listOutput) {
        if ($line -match 'Power Scheme GUID:\s*([A-Fa-f0-9\-]{36})\s+\((.*?)\)') {
            $foundGuid = $matches[1].ToLower()
            $foundName = $matches[2]
            if ($foundGuid -eq $guidLower) {
                return $foundName
            }
        }
    }

    return $Guid
}

function Set-ActivePlan {
    param(
        [string]$PlanGuid,
        [string]$Reason
    )

    & powercfg /setactive $PlanGuid | Out-Null
    Start-Sleep -Milliseconds 150

    $newCurrent = Get-ActivePowerSchemeGuid
    if ($newCurrent -eq $PlanGuid.ToLower()) {
        $planName = Get-PlanDisplayName $PlanGuid
        Write-Log "Switched to $planName [$PlanGuid] because $Reason"
        return $true
    }

    Write-Log "Failed to switch to [$PlanGuid] because $Reason"
    return $false
}

function Get-MatchingProcesses {
    $running = @{}
    foreach ($proc in (Get-Process -ErrorAction SilentlyContinue)) {
        $running["$($proc.ProcessName).exe".ToLower()] = $true
    }

    $matches = @()
    foreach ($target in $Config.TargetProcesses) {
        if ($running.ContainsKey($target.ToLower())) {
            $matches += $target.ToLower()
        }
    }

    return $matches | Sort-Object -Unique
}

function Show-Balloon {
    param(
        [string]$Title,
        [string]$Text,
        [int]$Timeout = 1800
    )

    $script:NotifyIcon.BalloonTipTitle = $Title
    $script:NotifyIcon.BalloonTipText = $Text
    $script:NotifyIcon.ShowBalloonTip($Timeout)
}

$script:ShouldExit = $false
$script:CurrentMode = ""
$script:LastMatches = @()
$script:LastPerformanceSeenAt = $null
$script:LastLoggedMatchState = ""
$script:LastTooltip = ""
$script:LastSanityCheckAt = Get-Date

$script:AppContext = New-Object System.Windows.Forms.ApplicationContext

$notifyIcon = New-Object System.Windows.Forms.NotifyIcon
$notifyIcon.Icon = [System.Drawing.SystemIcons]::Information
$notifyIcon.Visible = $true
$notifyIcon.Text = "Power Plan Auto Switcher"
$script:NotifyIcon = $notifyIcon

$contextMenu = New-Object System.Windows.Forms.ContextMenuStrip
$script:StatusItem = $contextMenu.Items.Add("Starting...")
$contextMenu.Items.Add("Watching: " + ($Config.TargetProcesses -join ", ")) | Out-Null
$script:CurrentPlanItem = $contextMenu.Items.Add("Current Plan: Detecting...")
$script:LastSeenItem = $contextMenu.Items.Add("Last Match: None")
$contextMenu.Items.Add("-") | Out-Null
$showStatusItem = $contextMenu.Items.Add("Show status")
$openLogItem = $contextMenu.Items.Add("Open log")
$contextMenu.Items.Add("-") | Out-Null
$switchBalancedItem = $contextMenu.Items.Add("Switch to Balanced now")
$switchPerformanceItem = $contextMenu.Items.Add("Switch to Ultra Performance now")
$contextMenu.Items.Add("-") | Out-Null
$exitItem = $contextMenu.Items.Add("Exit")
$notifyIcon.ContextMenuStrip = $contextMenu

function Update-Ui {
    param(
        [string]$ActivePlanName,
        [string[]]$Matches
    )

    if ($Matches.Count -gt 0) {
        $matchText = $Matches -join ", "
    } else {
        $matchText = "none"
    }

    if ($script:LastMatches.Count -gt 0) {
        $lastMatchText = $script:LastMatches -join ", "
    } else {
        $lastMatchText = "None"
    }

    $tooltip = "Plan: $ActivePlanName | Active: $matchText"
    if ($tooltip.Length -gt 63) {
        $tooltip = $tooltip.Substring(0, 63)
    }

    if ($tooltip -ne $script:LastTooltip) {
        $script:NotifyIcon.Text = $tooltip
        $script:StatusItem.Text = "Status: $tooltip"
        $script:LastTooltip = $tooltip
    }

    $script:CurrentPlanItem.Text = "Current Plan: $ActivePlanName"
    $script:LastSeenItem.Text = "Last Match: $lastMatchText"
}

$showStatusItem.add_Click({
    $activeGuid = Get-ActivePowerSchemeGuid
    $activePlanName = Get-PlanDisplayName $activeGuid

    if ($script:LastMatches.Count -gt 0) {
        $lastMatchText = $script:LastMatches -join ", "
    } else {
        $lastMatchText = "None"
    }

    $msg = @(
        "Current plan: $activePlanName"
        "Balanced: $($Config.BalancedPlanGuid)"
        "Ultra Performance: $($Config.PerformancePlanGuid)"
        ""
        "Watching: " + ($Config.TargetProcesses -join ", ")
        "Last match: $lastMatchText"
        ""
        "Poll interval: $($Config.PollIntervalMilliseconds) ms"
        "Hold performance: $($Config.HoldPerformanceSeconds) seconds"
        "Log: $($Config.LogPath)"
    ) -join "`n"

    [System.Windows.Forms.MessageBox]::Show($msg, "Power Plan Auto Switcher") | Out-Null
})

$openLogItem.add_Click({
    if (Test-Path $Config.LogPath) {
        Start-Process notepad.exe $Config.LogPath
    }
})

$switchBalancedItem.add_Click({
    if (Set-ActivePlan -PlanGuid $Config.BalancedPlanGuid -Reason "manual tray request") {
        $script:CurrentMode = $Config.BalancedPlanName
        if ($Config.ShowBalloonOnBalancedSwitch) {
            Show-Balloon -Title "Switching power plan" -Text $Config.BalancedPlanName
        }
    }
})

$switchPerformanceItem.add_Click({
    if (Set-ActivePlan -PlanGuid $Config.PerformancePlanGuid -Reason "manual tray request") {
        $script:CurrentMode = $Config.PerformancePlanName
        $script:LastPerformanceSeenAt = Get-Date
        if ($Config.ShowBalloonOnPerformanceSwitch) {
            Show-Balloon -Title "Switching power plan" -Text $Config.PerformancePlanName
        }
    }
})

$exitItem.add_Click({
    $script:ShouldExit = $true
})

$notifyIcon.add_DoubleClick({
    $showStatusItem.PerformClick()
})

$timer = New-Object System.Windows.Forms.Timer
$timer.Interval = $Config.PollIntervalMilliseconds

$timer.add_Tick({
    try {
        $matches = Get-MatchingProcesses
        $now = Get-Date
        $hasPerfProcess = $matches.Count -gt 0

        if ($matches.Count -gt 0) {
            $matchState = $matches -join ","
        } else {
            $matchState = "none"
        }

        if ($matchState -ne $script:LastLoggedMatchState) {
            Write-Log "Matched processes: $matchState"
            $script:LastLoggedMatchState = $matchState
        }

        if ($hasPerfProcess) {
            $script:LastPerformanceSeenAt = $now
            $script:LastMatches = $matches
        }

        $shouldBePerformance = $false

        if ($hasPerfProcess) {
            $shouldBePerformance = $true
        } elseif ($script:LastPerformanceSeenAt) {
            $secondsSinceSeen = ($now - $script:LastPerformanceSeenAt).TotalSeconds
            if ($secondsSinceSeen -lt $Config.HoldPerformanceSeconds) {
                $shouldBePerformance = $true
            }
        }

        if ($shouldBePerformance) {
            if ($script:CurrentMode -ne $Config.PerformancePlanName) {
                if ($hasPerfProcess) {
                    $reason = "matched process detected: " + ($matches -join ", ")
                } else {
                    $reason = "within hold period after last match"
                }

                if (Set-ActivePlan -PlanGuid $Config.PerformancePlanGuid -Reason $reason) {
                    $script:CurrentMode = $Config.PerformancePlanName

                    if ($Config.ShowBalloonOnPerformanceSwitch) {
                        if ($hasPerfProcess) {
                            $balloonText = "$($Config.PerformancePlanName) via " + ($matches -join ", ")
                        } else {
                            $balloonText = $Config.PerformancePlanName
                        }
                        Show-Balloon -Title "Switching power plan" -Text $balloonText
                    }
                }
            }
        } else {
            if ($script:CurrentMode -ne $Config.BalancedPlanName) {
                if (Set-ActivePlan -PlanGuid $Config.BalancedPlanGuid -Reason "no matching processes remain and hold expired") {
                    $script:CurrentMode = $Config.BalancedPlanName
                    if ($Config.ShowBalloonOnBalancedSwitch) {
                        Show-Balloon -Title "Switching power plan" -Text $Config.BalancedPlanName
                    }
                }
            }
        }

        if ((($now - $script:LastSanityCheckAt).TotalSeconds) -ge $Config.SanityCheckSeconds) {
            $actualGuid = Get-ActivePowerSchemeGuid
            if ($actualGuid) {
                if ($actualGuid -eq $Config.PerformancePlanGuid.ToLower()) {
                    $actualMode = $Config.PerformancePlanName
                } elseif ($actualGuid -eq $Config.BalancedPlanGuid.ToLower()) {
                    $actualMode = $Config.BalancedPlanName
                } else {
                    $actualMode = Get-PlanDisplayName $actualGuid
                }

                if ($actualMode -ne $script:CurrentMode) {
                    Write-Log "Detected external plan change. Internal mode was '$($script:CurrentMode)', actual plan is '$actualMode'."
                    $script:CurrentMode = $actualMode
                }
            }
            $script:LastSanityCheckAt = $now
        }

        Update-Ui -ActivePlanName $script:CurrentMode -Matches $matches

        if ($script:ShouldExit) {
            $timer.Stop()
            $script:NotifyIcon.Visible = $false
            $script:NotifyIcon.Dispose()
            Write-Log "Exiting utility."
            $script:AppContext.ExitThread()
        }
    } catch {
        Write-Log "Loop error: $($_.Exception.Message)"
    }
})

$currentGuid = Get-ActivePowerSchemeGuid
$currentGuidLower = if ($currentGuid) { $currentGuid.ToLower() } else { "" }

if ($currentGuidLower -eq $Config.PerformancePlanGuid.ToLower()) {
    $script:CurrentMode = $Config.PerformancePlanName
} elseif ($currentGuidLower -eq $Config.BalancedPlanGuid.ToLower()) {
    $script:CurrentMode = $Config.BalancedPlanName
} else {
    $script:CurrentMode = Get-PlanDisplayName $currentGuid
}

Write-Log "Starting utility."
Write-Log "Balanced plan: $($Config.BalancedPlanName) [$($Config.BalancedPlanGuid)]"
Write-Log "Performance plan: $($Config.PerformancePlanName) [$($Config.PerformancePlanGuid)]"
Write-Log "Watching processes: $($Config.TargetProcesses -join ', ')"
Write-Log "Poll interval: $($Config.PollIntervalMilliseconds) ms"
Write-Log "Hold performance seconds: $($Config.HoldPerformanceSeconds)"
Write-Log "Sanity check seconds: $($Config.SanityCheckSeconds)"
Write-Log "Initial mode: $($script:CurrentMode)"

$timer.Start()
[System.Windows.Forms.Application]::Run($script:AppContext)