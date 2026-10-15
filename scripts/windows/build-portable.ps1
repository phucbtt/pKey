<#
.SYNOPSIS
  Builds dist\pKey.exe: ONE self-contained file that runs on any Windows 10/11 PC
  (32-bit, 64-bit and ARM64), with nothing to install and nothing beside it.

.DESCRIPTION
  * 32-bit build: a 32-bit program runs on x86, x64 and (through emulation) ARM64 Windows. The
    keyboard hook and SendInput work across bitness, so it types into 64-bit programs too.
  * Static C runtime (see .cargo\config.toml): no Visual C++ Redistributable is needed.
  * Settings are kept per user in %APPDATA%\pKey, so the file can sit anywhere (Desktop, USB).
  Needs the 32-bit Rust target:  rustup target add i686-pc-windows-msvc
#>
$ErrorActionPreference = 'Stop'
$repo = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$triple = 'i686-pc-windows-msvc'

& cargo build --manifest-path "$repo\Cargo.toml" -p pkey-app --profile dist --target $triple
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

$dist = Join-Path $repo 'dist'
New-Item -ItemType Directory -Force $dist | Out-Null
$exe = Join-Path $dist 'pKey.exe'
Copy-Item "$repo\target\$triple\dist\pkey.exe" $exe -Force

# The file must not depend on anything that is not part of Windows itself.
$text = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($exe))
foreach ($bad in 'VCRUNTIME', 'MSVCP', 'api-ms-win-crt', 'ucrtbase', 'MSVCR') {
    if ($text -match [regex]::Escape($bad)) { throw "pKey.exe depends on $bad (the C runtime is not static)" }
}

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
