param(
    [Parameter(Mandatory=$true)][string]$Installer,
    [Parameter(Mandatory=$true)][string]$Manifest,
    [Parameter(Mandatory=$true)][string]$InstallDir,
    [Parameter(Mandatory=$true)][string]$ReportPath,
    [switch]$Cleanup,
    [switch]$GuiConfirmed
)
# Run in the current user's interactive desktop. GuiConfirmed may be supplied only
# after a human or an authorized UI tool observes this exact installed app.
$ErrorActionPreference = 'Stop'
$Installer = [IO.Path]::GetFullPath($Installer)
$InstallDir = [IO.Path]::GetFullPath($InstallDir)
$ReportPath = [IO.Path]::GetFullPath($ReportPath)
$manifestData = Get-Content -LiteralPath $Manifest -Raw | ConvertFrom-Json
$asset = @($manifestData.assets | Where-Object { $_.platform -eq 'windows' })
if ($asset.Count -ne 1) { throw 'Expected one Windows asset' }
$installerHash = (Get-FileHash -LiteralPath $Installer -Algorithm SHA256).Hash.ToLowerInvariant()
if ($installerHash -ne $asset[0].sha256) { throw 'Installer hash mismatch' }
$uninstallRoot = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall'
function Registrations {
    @(Get-ChildItem -LiteralPath $uninstallRoot -ErrorAction SilentlyContinue | ForEach-Object {
        $v = Get-ItemProperty -LiteralPath $_.PSPath
        if ($v.DisplayName -eq 'Chrome Account Signout') { $_.Name }
    })
}
function Shortcuts {
    @([Environment]::GetFolderPath('Desktop'), [Environment]::GetFolderPath('Programs')) | ForEach-Object {
        if ($_ -and (Test-Path -LiteralPath $_)) {
            Get-ChildItem -LiteralPath $_ -Filter '*Chrome*Account*Signout*.lnk' -Recurse -File | Select-Object -ExpandProperty FullName
        }
    }
}
function Save-Report { $script:report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $ReportPath -Encoding utf8 }
$prior = if (Test-Path -LiteralPath $ReportPath) { Get-Content -LiteralPath $ReportPath -Raw | ConvertFrom-Json -AsHashtable } else { $null }
if ($prior -and $prior.cleanup -ne 'passed') {
    if ($prior.install_dir -ne $InstallDir -or $prior.installer_sha256 -ne $installerHash -or $prior.source_commit -ne $manifestData.source_commit) { throw 'Conflicting owned test report' }
    $report = $prior
} else {
    if ((Test-Path -LiteralPath $InstallDir) -or @(Registrations).Count -or @(Shortcuts).Count) { throw 'Existing installation or shortcut; refusing test' }
    New-Item -ItemType Directory -Path (Split-Path -Parent $ReportPath) -Force | Out-Null
    $report = [ordered]@{ source_commit=$manifestData.source_commit; installer_sha256=$installerHash; binary_sha256=$asset[0].binary_sha256; install_dir=$InstallDir; ownership='new dedicated installation'; installation='pending'; launch='pending'; gui='unverified'; cleanup='pending'; process_id=$null; process_started_ticks=0; failure=$null; started_utc=[DateTime]::UtcNow.ToString('o') }
    Save-Report
}
try {
    if ($report.installation -ne 'passed') {
        # NSIS requires /D to be the final argument; do not add flags after it.
        $setup = Start-Process -FilePath $Installer -ArgumentList @('/S', "/D=$InstallDir") -PassThru -Wait
        if ($setup.ExitCode -ne 0) { throw 'Installer failed' }
        $report.installation = 'passed'; Save-Report
    }
    $binary = Join-Path $InstallDir 'chrome-account-signout.exe'
    if (-not (Test-Path -LiteralPath $binary) -or (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant() -ne $asset[0].binary_sha256) { throw 'Installed executable mismatch' }
    if ($report.launch -ne 'passed') {
        $app = Start-Process -FilePath $binary -PassThru
        $report.process_id = $app.Id
        $report.process_started_ticks = $app.StartTime.ToUniversalTime().Ticks
        $report.launch = 'passed'; Save-Report
    }
    if ($GuiConfirmed) { $report.gui = 'passed-user-or-authorized-ui-confirmation'; Save-Report }
    # Leave the owned installation/window available for observation until GUI is
    # confirmed. A second invocation resumes this report, never reinstalls blindly.
    if ($Cleanup -and $report.gui -eq 'passed-user-or-authorized-ui-confirmation') {
        $app = Get-Process -Id $report.process_id -ErrorAction SilentlyContinue
        if ($app) {
            if ($app.Path -ne $binary -or $app.StartTime.ToUniversalTime().Ticks -ne $report.process_started_ticks) { throw 'Process identity changed' }
            if (-not $app.CloseMainWindow()) { throw 'Could not request app close' }
            if (-not $app.WaitForExit(15000)) { throw 'App still running; no forced termination' }
        }
        $uninstallers = @(Get-ChildItem -LiteralPath $InstallDir -Filter '*uninstall*.exe' -File)
        if ($uninstallers.Count -ne 1) { throw 'Owned uninstaller unavailable' }
        $uninstaller = Start-Process -FilePath $uninstallers[0].FullName -ArgumentList '/S' -Wait -PassThru
        if ($uninstaller.ExitCode -ne 0) { throw 'Uninstaller failed' }
        $until = [DateTime]::UtcNow.AddSeconds(20)
        do { Start-Sleep -Milliseconds 250 } while ((Test-Path -LiteralPath $binary) -and [DateTime]::UtcNow -lt $until)
        if ((Test-Path -LiteralPath $binary) -or @(Registrations).Count -or @(Shortcuts).Count) { throw 'Installation cleanup incomplete' }
        if ((Test-Path -LiteralPath $InstallDir) -and @(Get-ChildItem -LiteralPath $InstallDir -Force).Count) { throw 'Installation directory still contains files' }
        if (Test-Path -LiteralPath $InstallDir) { Remove-Item -LiteralPath $InstallDir }
        $report.cleanup = 'passed'; Save-Report
    }
    Write-Output "Installed-app report: installation=$($report.installation), launch=$($report.launch), gui=$($report.gui), cleanup=$($report.cleanup)"
} catch {
    $report.failure = 'Native qualification failed; inspect owned installation and retry its uninstaller'
    Save-Report
    throw 'Native qualification failed; report preserved without account data'
}
