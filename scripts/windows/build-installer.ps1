<#
.SYNOPSIS
  Builds target\installer\pKey-Setup-<version>.exe (per-user installer, unsigned).
  Needs Inno Setup 6 (`winget install JRSoftware.InnoSetup`) and the x86 Rust target.
#>
$ErrorActionPreference = 'Stop'
$repo = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$version = (Select-String -Path "$repo\Cargo.toml" -Pattern '^version = "([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value

$iscc = @(
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) { throw 'Inno Setup 6 not found (winget install JRSoftware.InnoSetup).' }

$stage = "$repo\target\installer"
Remove-Item -Recurse -Force "$stage\x64", "$stage\x86" -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force "$stage\x64", "$stage\x86" | Out-Null

foreach ($t in @(@('x64', 'x86_64-pc-windows-msvc'), @('x86', 'i686-pc-windows-msvc'))) {
    & cargo build --manifest-path "$repo\Cargo.toml" -p pkey-tsf --release --target $t[1]
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed for $($t[1])" }
    Copy-Item "$repo\target\$($t[1])\release\pkey_tsf.dll" "$stage\$($t[0])\" -Force
}
& cargo build --manifest-path "$repo\Cargo.toml" -p ime-cli --release
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed for the CLI' }
Copy-Item "$repo\target\release\pkey-cli.exe" "$stage\x64\" -Force
& cargo build --manifest-path "$repo\Cargo.toml" -p pkey-app --release
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed for the app' }
Copy-Item "$repo\target\release\pkey.exe" "$stage\x64\" -Force

& $iscc "/DAppVersion=$version" "/DBuildDir=$stage" "$repo\installer\windows\pkey.iss"
if ($LASTEXITCODE -ne 0) { throw 'ISCC failed' }

$out = "$stage\pKey-Setup-$version.exe"
Get-FileHash $out -Algorithm SHA256 | ForEach-Object { "$($_.Hash)  $(Split-Path $out -Leaf)" } | Set-Content "$out.sha256"
Get-Item $out | Select-Object FullName, Length
