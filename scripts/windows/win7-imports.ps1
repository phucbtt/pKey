<#
.SYNOPSIS
  Helpers (dot-source this file) that make a Windows 7 build verifiable without a Windows 7 PC.

.DESCRIPTION
  Get-PeImports   lists the DLLs a PE file imports (and where their names sit in the file).
  Repair-Win7Imports  rewrites imports of `combase.dll` to `ole32.dll`.
  Assert-Win7Imports  throws if the file imports a DLL that does not exist on Windows 7.

  Why: the `windows` crate (0.62) links CoTaskMemFree and CoCreateFreeThreadedMarshaler from
  `combase.dll`, which Windows has only since 8. Both functions are in `ole32.dll` on Windows 7 and
  `ole32.dll` forwards them on newer systems, so pointing the import there works everywhere. The
  name `ole32.dll` is shorter than `combase.dll`, so the import table is patched in place.
#>

function Get-PeImports([string]$Path) {
    $b = [IO.File]::ReadAllBytes($Path)
    $pe = [BitConverter]::ToInt32($b, 0x3C)
    if ([BitConverter]::ToUInt32($b, $pe) -ne 0x00004550) { throw "$Path is not a PE file" }
    $sectionCount = [BitConverter]::ToUInt16($b, $pe + 6)
    $optionalSize = [BitConverter]::ToUInt16($b, $pe + 20)
    $opt = $pe + 24
    $dirs = if ([BitConverter]::ToUInt16($b, $opt) -eq 0x20B) { $opt + 112 } else { $opt + 96 }
    $importRva = [BitConverter]::ToUInt32($b, $dirs + 8)   # data directory 1 = imports
    $sections = $opt + $optionalSize

    $toOffset = {
        param([uint32]$rva)
        for ($i = 0; $i -lt $sectionCount; $i++) {
            $s = $sections + 40 * $i
            $virtualSize = [BitConverter]::ToUInt32($b, $s + 8)
            $virtualAddress = [BitConverter]::ToUInt32($b, $s + 12)
            $rawSize = [BitConverter]::ToUInt32($b, $s + 16)
            $rawPointer = [BitConverter]::ToUInt32($b, $s + 20)
            if ($rva -ge $virtualAddress -and $rva -lt $virtualAddress + [Math]::Max($virtualSize, $rawSize)) {
                return [int]($rva - $virtualAddress + $rawPointer)
            }
        }
        throw "RVA $rva is not inside any section of $Path"
    }

    $imports = @()
    if ($importRva -eq 0) { return $imports }
    $d = & $toOffset $importRva
    while ($true) {
        $nameRva = [BitConverter]::ToUInt32($b, $d + 12)
        if ($nameRva -eq 0) { break }
        $nameOffset = & $toOffset $nameRva
        $end = $nameOffset
        while ($b[$end] -ne 0) { $end++ }
        $imports += [pscustomobject]@{
            Dll    = [Text.Encoding]::ASCII.GetString($b, $nameOffset, $end - $nameOffset)
            Offset = $nameOffset
            Length = $end - $nameOffset
        }
        $d += 20
    }
    return $imports
}

function Repair-Win7Imports([string]$Path) {
    $replacement = [Text.Encoding]::ASCII.GetBytes('ole32.dll')
    $patched = 0
    $imports = Get-PeImports $Path
    $bytes = [IO.File]::ReadAllBytes($Path)
    foreach ($i in @($imports | Where-Object { $_.Dll -ieq 'combase.dll' })) {
        if ($replacement.Length -gt $i.Length) { throw 'replacement name is longer than the original' }
        for ($k = 0; $k -lt $i.Length; $k++) {
            $bytes[$i.Offset + $k] = if ($k -lt $replacement.Length) { $replacement[$k] } else { 0 }
        }
        $patched++
    }
    if ($patched -gt 0) { [IO.File]::WriteAllBytes($Path, $bytes) }
    return $patched
}

function Assert-Win7Imports([string]$Path) {
    $bad = Get-PeImports $Path | Where-Object { $_.Dll -match '^(api-ms-win-|ext-ms-win-|combase\.dll$|ucrtbase|vcruntime|msvcp|msvcr)' }
    if ($bad) { throw "$Path imports DLLs that Windows 7 does not have: $(($bad.Dll | Sort-Object -Unique) -join ', ')" }
}
