<powershell>
# User data for the Windows Server image bake.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
Start-Transcript C:\swofty-bake.log

# Defender as the hosted image sets it (runner-images Configure-WindowsDefender.ps1): no real-time scanning of
# the build, with both drives excluded.
Set-MpPreference -DisableRealtimeMonitoring $true -DisableBehaviorMonitoring $true -DisableIOAVProtection $true `
  -DisableScriptScanning $true -DisableArchiveScanning $true -ScanAvgCPULoadFactor 5
Add-MpPreference -ExclusionPath 'C:\', 'D:\'

New-ItemProperty HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem -Name LongPathsEnabled -Value 1 -PropertyType DWORD -Force

function Get-Asset($repo, $pattern) {
  (Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest").assets |
    Where-Object name -like $pattern | Select-Object -First 1 -ExpandProperty browser_download_url
}

Invoke-WebRequest https://aka.ms/vs/17/release/vs_buildtools.exe -OutFile C:\vs_buildtools.exe
$vs = Start-Process C:\vs_buildtools.exe -Wait -PassThru -ArgumentList '--quiet', '--wait', '--norestart', '--nocache',
  '--add', 'Microsoft.VisualStudio.Workload.VCTools', '--includeRecommended'
if ($vs.ExitCode -notin 0, 3010) { throw "vs_buildtools exited $($vs.ExitCode)" }

Invoke-WebRequest (Get-Asset 'git-for-windows/git' 'Git-*-64-bit.exe') -OutFile C:\git.exe
# The hosted image's options: its path carries Git's Unix tools, which tests spawn (true, sleep).
Start-Process C:\git.exe -Wait -ArgumentList '/VERYSILENT', '/NORESTART', '/NOCANCEL', '/SP-', '/SUPPRESSMSGBOXES',
  '/o:PathOption=CmdTools', '/o:BashTerminalOption=ConHost', '/o:EnableSymlinks=Enabled', '/COMPONENTS=gitlfs'
& 'C:\Program Files\Git\cmd\git.exe' config --system --add safe.directory '*'
[Environment]::SetEnvironmentVariable('GCM_INTERACTIVE', 'Never', 'Machine')

# Actions' composite steps run bash, and actions/cache compresses with zstd; the hosted image has both on the path.
Invoke-WebRequest (Get-Asset 'facebook/zstd' 'zstd-v*-win64.zip') -OutFile C:\zstd.zip
Expand-Archive C:\zstd.zip C:\zstd-extract
New-Item -ItemType Directory 'C:\Program Files\zstd' -Force | Out-Null
Get-ChildItem C:\zstd-extract -Recurse -Filter zstd.exe | Select-Object -First 1 | Copy-Item -Destination 'C:\Program Files\zstd'
Remove-Item C:\zstd.zip, C:\zstd-extract -Recurse -Force

Invoke-WebRequest (Get-Asset 'PowerShell/PowerShell' 'PowerShell-*-win-x64.msi') -OutFile C:\pwsh.msi
Start-Process msiexec.exe -Wait -ArgumentList '/i', 'C:\pwsh.msi', '/quiet', '/norestart'

# The hosted runners' paths, so Rust caches move between them and these instances.
$rustup = 'C:\Users\runneradmin\.rustup'
$cargo = 'C:\Users\runneradmin\.cargo'
[Environment]::SetEnvironmentVariable('RUSTUP_HOME', $rustup, 'Machine')
[Environment]::SetEnvironmentVariable('CARGO_HOME', $cargo, 'Machine')
$env:RUSTUP_HOME = $rustup
$env:CARGO_HOME = $cargo
$toolchain = ((Invoke-WebRequest https://raw.githubusercontent.com/Swofty-Developments/SwoftyLauncher/main/rust-toolchain.toml -UseBasicParsing).Content |
  Select-String 'channel = "(.*)"').Matches[0].Groups[1].Value
Invoke-WebRequest https://win.rustup.rs/x86_64 -OutFile C:\rustup-init.exe
Start-Process C:\rustup-init.exe -Wait -ArgumentList '-y', '--profile', 'minimal', '--default-toolchain', $toolchain, '-c', 'rustfmt', '-c', 'clippy', '--no-modify-path'
$path = [Environment]::GetEnvironmentVariable('Path', 'Machine')
[Environment]::SetEnvironmentVariable('Path', "$cargo\bin;C:\Program Files\Git\bin;C:\Program Files\zstd;$path", 'Machine')

$version = (Invoke-RestMethod https://api.github.com/repos/actions/runner/releases/latest).tag_name.TrimStart('v')
Invoke-WebRequest "https://github.com/actions/runner/releases/download/v$version/actions-runner-win-x64-$version.zip" -OutFile C:\runner.zip
Expand-Archive C:\runner.zip C:\actions-runner
New-Item -ItemType Directory C:\hostedtoolcache\windows -Force | Out-Null
Set-Content C:\actions-runner\.env "ImageOS=win25`nRUNNER_TOOL_CACHE=C:\hostedtoolcache\windows`nAGENT_TOOLSDIRECTORY=C:\hostedtoolcache\windows`nRUSTUP_HOME=$rustup`nCARGO_HOME=$cargo"

Set-Content C:\actions-runner\swofty-run.ps1 @'
# Runs the one job its just-in-time config allows, then powers off; the instance terminates on shutdown.
# An instance that has not started a job 15 minutes in (its run was cancelled) powers off too.
# Jobs work in D:\a on the blank second volume, the hosted runners' folder.
shutdown.exe /s /t 5400 /f
Get-Disk | Where-Object PartitionStyle -eq RAW | Select-Object -First 1 | Initialize-Disk -PartitionStyle GPT -PassThru |
  New-Partition -DriveLetter D -UseMaximumSize | Format-Volume -FileSystem NTFS -Confirm:$false | Out-Null
Start-Job { Start-Sleep 900; if (-not (Test-Path C:\actions-runner\_diag\Worker_*)) { shutdown.exe /a; shutdown.exe /s /t 0 /f } } | Out-Null
$env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine')
& C:\actions-runner\run.cmd --jitconfig (Get-Content C:\actions-runner\.jitconfig -Raw).Trim()
shutdown.exe /a
shutdown.exe /s /t 0 /f
'@

Remove-Item C:\vs_buildtools.exe, C:\git.exe, C:\pwsh.msi, C:\rustup-init.exe, C:\runner.zip -Force
Set-Content C:\swofty-bake-done 'done'
Stop-Transcript
& 'C:\Program Files\Amazon\EC2Launch\EC2Launch.exe' reset --clean --block
Stop-Computer -Force
</powershell>
