<#
.SYNOPSIS
  Developer install / uninstall of the pKey text service for the current user (no admin rights).

.DESCRIPTION
  install    builds the x64 and x86 DLLs, copies them to %LOCALAPPDATA%\pKey\bin\{x64,x86} and
             registers them with `regsvr32 /n /i:user` (HKCU only). Afterwards add the keyboard in
             Settings > Time & language > Language & region > Vietnamese > Language options >
             Add a keyboard > pKey.
  uninstall  unregisters both DLLs and deletes the copied files. Settings, config.toml and
             macros.json under %APPDATA%\pKey are left alone.
  status     shows what is registered.

  The all-users installer (HKLM, signed MSI) is a separate deliverable (REL-001a).

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\windows\pkey-dev.ps1 install
#>
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [ValidateSet('install', 'uninstall', 'status')]
    [string]$Action
)

$ErrorActionPreference = 'Stop'
$repo = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$dest = Join-Path $env:LOCALAPPDATA 'pKey\bin'
$clsid = '{434E620C-D5E7-4347-B6BC-BE6BE00C417D}'
$targets = @(
    @{ Name = 'x64'; Triple = 'x86_64-pc-windows-msvc'; Regsvr = "$env:WINDIR\System32\regsvr32.exe" },
    @{ Name = 'x86'; Triple = 'i686-pc-windows-msvc'; Regsvr = "$env:WINDIR\SysWOW64\regsvr32.exe" }
)

function Invoke-Regsvr([string]$exe, [string[]]$arguments) {
    $p = Start-Process -FilePath $exe -ArgumentList $arguments -Wait -PassThru -WindowStyle Hidden
    if ($p.ExitCode -ne 0) { throw "$exe $($arguments -join ' ') failed with exit code $($p.ExitCode)" }
}

switch ($Action) {
    'install' {
        foreach ($t in $targets) {
            Write-Host "Building $($t.Name) ..."
            & cargo build --manifest-path "$repo\Cargo.toml" -p pkey-tsf --release --target $t.Triple
            if ($LASTEXITCODE -ne 0) { throw "cargo build failed for $($t.Triple)" }
        }
        foreach ($t in $targets) {
            $dir = Join-Path $dest $t.Name
            New-Item -ItemType Directory -Force $dir | Out-Null
            $dll = Join-Path $dir 'pkey_tsf.dll'
            # A DLL that an application has loaded cannot be overwritten: unregister first.
            if (Test-Path $dll) { try { Invoke-Regsvr $t.Regsvr @('/s', '/u', '/n', '/i:user', "`"$dll`"") } catch { } }
            Copy-Item "$repo\target\$($t.Triple)\release\pkey_tsf.dll" $dll -Force
            Invoke-Regsvr $t.Regsvr @('/s', '/n', '/i:user', "`"$dll`"")
            Write-Host "Registered $($t.Name): $dll"
        }
        Write-Host ''
        Write-Host 'Done. Add the keyboard in Settings > Time & language > Language & region >'
        Write-Host 'Vietnamese > Language options > Add a keyboard > pKey.'
        Write-Host 'Applications started before now must be restarted to pick the service up.'
    }
    'uninstall' {
        foreach ($t in $targets) {
            $dll = Join-Path (Join-Path $dest $t.Name) 'pkey_tsf.dll'
            if (Test-Path $dll) {
                try { Invoke-Regsvr $t.Regsvr @('/s', '/u', '/n', '/i:user', "`"$dll`""); Write-Host "Unregistered $($t.Name)" }
                catch { Write-Warning $_ }
            }
        }
        # Files may stay locked while applications that loaded the DLL are running.
        try { Remove-Item -Recurse -Force $dest -ErrorAction Stop; Write-Host "Removed $dest" }
        catch { Write-Warning "Could not remove $dest (in use). Restart the applications or sign out, then delete it." }
    }
    'status' {
        foreach ($view in @('64', '32')) {
            $base = [Microsoft.Win32.RegistryKey]::OpenBaseKey('CurrentUser', "Registry$view")
            $key = $base.OpenSubKey("Software\Classes\CLSID\$clsid\InprocServer32")
            if ($key) { Write-Host "$view-bit view: $($key.GetValue(''))" } else { Write-Host "$view-bit view: not registered" }
        }
    }
}
