<#
.SYNOPSIS
  Builds target\installer\pKey-Setup-<version>.exe (per-user installer, unsigned) for
  Windows 7 SP1 and newer, 32-bit and 64-bit.
  Needs Inno Setup 6 (`winget install JRSoftware.InnoSetup`). The Windows 7 Rust targets are built
  from source on the nightly pinned in win7-toolchain.txt, which is installed on demand.
#>
$ErrorActionPreference = 'Stop'
$repo = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$version = (Select-String -Path "$repo\Cargo.toml" -Pattern '^version = "([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$toolchain = (Get-Content (Join-Path $PSScriptRoot 'win7-toolchain.txt') -TotalCount 1).Trim()

$iscc = @(
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) { throw 'Inno Setup 6 not found (winget install JRSoftware.InnoSetup).' }

& rustup toolchain install $toolchain --profile minimal -c rust-src
if ($LASTEXITCODE -ne 0) { throw "rustup could not install $toolchain" }

function Build([string]$package, [string]$triple) {
    & cargo "+$toolchain" build --manifest-path "$repo\Cargo.toml" -p $package --release `
        --target $triple '-Zbuild-std=std,panic_unwind'
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed for $package ($triple)" }
}

. (Join-Path $PSScriptRoot 'win7-imports.ps1')

$stage = "$repo\target\installer"
Remove-Item -Recurse -Force "$stage\x64", "$stage\x86" -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force "$stage\x64", "$stage\x86" | Out-Null

# The keyboard DLL must match the bitness of each application, so both are built. The app and the
# CLI are 32-bit only: that runs on every Windows (x86, x64, ARM64) and types into 64-bit programs.
Build 'pkey-tsf' 'x86_64-win7-windows-msvc'
Copy-Item "$repo\target\x86_64-win7-windows-msvc\release\pkey_tsf.dll" "$stage\x64\" -Force
Build 'pkey-tsf' 'i686-win7-windows-msvc'
Copy-Item "$repo\target\i686-win7-windows-msvc\release\pkey_tsf.dll" "$stage\x86\" -Force
Build 'ime-cli' 'i686-win7-windows-msvc'
Copy-Item "$repo\target\i686-win7-windows-msvc\release\pkey-cli.exe" "$stage\x86\" -Force
Build 'pkey-app' 'i686-win7-windows-msvc'
Copy-Item "$repo\target\i686-win7-windows-msvc\release\pkey.exe" "$stage\x86\" -Force

# Nothing that is staged may import a DLL Windows 7 lacks (the keyboard DLLs need a small repair).
foreach ($dll in Get-ChildItem "$stage\*\pkey_tsf.dll") { [void](Repair-Win7Imports $dll.FullName) }
foreach ($file in Get-ChildItem "$stage\*\*" -Include *.exe, *.dll) { Assert-Win7Imports $file.FullName }

& $iscc "/DAppVersion=$version" "/DBuildDir=$stage" "$repo\installer\windows\pkey.iss"
if ($LASTEXITCODE -ne 0) { throw 'ISCC failed' }

$out = "$stage\pKey-Setup-$version.exe"
Get-FileHash $out -Algorithm SHA256 | ForEach-Object { "$($_.Hash)  $(Split-Path $out -Leaf)" } | Set-Content "$out.sha256"
Get-Item $out | Select-Object FullName, Length
