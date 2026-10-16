<#
.SYNOPSIS
  Builds dist\pKey.exe: ONE self-contained file that runs on Windows 7 SP1, 8, 8.1, 10 and 11
  (32-bit, 64-bit and ARM64), with nothing to install and nothing beside it.

.DESCRIPTION
  * Windows 7 target: stable Rust needs Windows 10, so this builds the `i686-win7-windows-msvc`
    target (tier 3) from source with `-Zbuild-std` on the nightly pinned in win7-toolchain.txt.
    The result imports nothing that Windows 7 lacks (checked below).
  * 32-bit build: a 32-bit program runs on x86, x64 and (through emulation) ARM64 Windows. The
    keyboard hook and SendInput work across bitness, so it types into 64-bit programs too.
  * Static C runtime (see .cargo\config.toml): no Visual C++ Redistributable is needed.
  * Settings are kept per user in %APPDATA%\pKey, so the file can sit anywhere (Desktop, USB).
  The nightly and its rust-src component are installed on demand (rustup).
#>
param(
    # Where pKey.exe and its .sha256 go. A running pKey.exe cannot be overwritten, so a different
    # folder can be given while the app is in use.
    [string]$OutDir
)
$ErrorActionPreference = 'Stop'
$repo = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$triple = 'i686-win7-windows-msvc'
$toolchain = (Get-Content (Join-Path $PSScriptRoot 'win7-toolchain.txt') -TotalCount 1).Trim()

& rustup toolchain install $toolchain --profile minimal -c rust-src
if ($LASTEXITCODE -ne 0) { throw "rustup could not install $toolchain" }

& cargo "+$toolchain" build --manifest-path "$repo\Cargo.toml" -p pkey-app --profile dist `
    --target $triple '-Zbuild-std=std,panic_unwind'
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

$dist = if ($OutDir) { $OutDir } else { Join-Path $repo 'dist' }
New-Item -ItemType Directory -Force $dist | Out-Null
$exe = Join-Path $dist 'pKey.exe'
Copy-Item "$repo\target\$triple\dist\pkey.exe" $exe -Force

# The file must not import anything that is not part of Windows 7 itself (no Visual C++ runtime,
# no api-ms-win-* API sets, no combase.dll).
. (Join-Path $PSScriptRoot 'win7-imports.ps1')
Assert-Win7Imports $exe

# It must start, show its window and tray icon, install its hooks and exit cleanly.
$scratch = Join-Path ([IO.Path]::GetTempPath()) "pkey-selftest-$PID"
$env:PKEY_CONFIG_DIR = $scratch
try {
    $p = Start-Process $exe -ArgumentList '--selftest' -PassThru -Wait
    if ($p.ExitCode -ne 0) { throw "self-test failed with exit code $($p.ExitCode)" }
} finally {
    Remove-Item Env:PKEY_CONFIG_DIR -ErrorAction SilentlyContinue
    Remove-Item -Recurse -Force $scratch -ErrorAction SilentlyContinue
}

$hash = (Get-FileHash $exe -Algorithm SHA256).Hash
"$hash  pKey.exe" | Set-Content "$exe.sha256"
Get-Item $exe | Select-Object FullName, @{ n = 'KB'; e = { [math]::Round($_.Length / 1KB) } }
"SHA256 $hash"
