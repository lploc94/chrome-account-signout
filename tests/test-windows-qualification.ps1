# Synthetic owned state only. No installer/app executable is run, no account data read.
$ErrorActionPreference='Stop'
$root=Join-Path ([IO.Path]::GetTempPath()) ('signout-qualification-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root | Out-Null
$helper=Join-Path $PSScriptRoot '..\scripts\test-windows-installed.ps1'
$installer=Join-Path $root 'synthetic-setup.exe'
[IO.File]::WriteAllText($installer,'synthetic installer never executed')
$hash=(Get-FileHash $installer -Algorithm SHA256).Hash.ToLowerInvariant()
$binaryHash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes('synthetic app'))).ToLowerInvariant()
$manifest=Join-Path $root 'manifest.json'
@{source_commit=('1'*40);assets=@(@{platform='windows';sha256=$hash;binary_sha256=$binaryHash})} | ConvertTo-Json -Depth 4 | Set-Content $manifest
# Keep synthetic cleanup independent of any user install/shortcuts.
function Get-ChildItem {
    param([string]$LiteralPath,[string]$Filter,[switch]$Force,[switch]$File,[switch]$Recurse,$ErrorAction)
    if ($LiteralPath -like 'HKCU:*' -or $LiteralPath -notlike "$root*") { return @() }
    $argsForNative=@{LiteralPath=$LiteralPath}
    if ($Filter) { $argsForNative.Filter=$Filter }
    if ($Force) { $argsForNative.Force=$true }
    if ($File) { $argsForNative.File=$true }
    if ($Recurse) { $argsForNative.Recurse=$true }
    Microsoft.PowerShell.Management\Get-ChildItem @argsForNative
}
$global:signoutQualificationFixture=[pscustomobject]@{Root=$root;Installation=$null;Jobs=@()}
function Start-Process {
    param([string]$FilePath,$ArgumentList,[switch]$Wait,[switch]$PassThru)
    if ($FilePath -ne (Join-Path $global:signoutQualificationFixture.Installation 'uninstall.exe')) { throw 'Unexpected executable launch in fixture' }
    $global:signoutQualificationFixture.Jobs+=Start-Job -ArgumentList $global:signoutQualificationFixture.Installation -ScriptBlock {
        param($directory)
        Start-Sleep -Milliseconds 200
        Remove-Item -LiteralPath (Join-Path $directory 'chrome-account-signout.exe')
        Start-Sleep -Milliseconds 900
        Remove-Item -LiteralPath (Join-Path $directory 'uninstall.exe')
    }
    [pscustomobject]@{ExitCode=0}
}
try {
    foreach ($mode in @('already-removed','asynchronous')) {
        $installation=Join-Path $root $mode
        $global:signoutQualificationFixture.Installation=$installation
        if ($mode -eq 'asynchronous') {
            New-Item -ItemType Directory -Path $installation | Out-Null
            [IO.File]::WriteAllText((Join-Path $installation 'chrome-account-signout.exe'),'synthetic app')
            [IO.File]::WriteAllText((Join-Path $installation 'uninstall.exe'),'synthetic uninstaller never executed')
        }
        $reportPath=Join-Path $root ($mode+'.json')
        # Current PowerShell PID represents a reused PID belonging to a different executable.
        @{source_commit=('1'*40);installer_sha256=$hash;binary_sha256=$binaryHash;install_dir=$installation;ownership='new dedicated installation';installation='passed';launch='passed';gui='passed-user-or-authorized-ui-confirmation';cleanup='running';process_id=$PID;process_started_ticks=0;failure=$null} | ConvertTo-Json | Set-Content $reportPath
        & $helper -Installer $installer -Manifest $manifest -InstallDir $installation -ReportPath $reportPath -Cleanup
        $report=Get-Content $reportPath -Raw | ConvertFrom-Json
        if ($report.cleanup -ne 'passed' -or (Test-Path $installation)) { throw "Cleanup regression: $mode" }
    }
    Write-Output 'Synthetic uninstall retry/PID reuse/asynchronous cleanup PASS'
} finally {
    if ($global:signoutQualificationFixture.Jobs.Count) { $global:signoutQualificationFixture.Jobs | Wait-Job | Receive-Job; $global:signoutQualificationFixture.Jobs | Remove-Job }
    Remove-Variable -Name signoutQualificationFixture -Scope Global
    # This freshly generated temporary fixture is the only recursive deletion.
    Microsoft.PowerShell.Management\Remove-Item -LiteralPath $root -Recurse -Force
}
