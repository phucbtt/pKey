Add-Type -AssemblyName System.Drawing

$srcPath = "C:\Users\yf242\Downloads\Biểu tượng xanh chữ p bóng bẩy.png"
if (-not (Test-Path $srcPath)) {
    $srcPath = "d:\Tools\pKey\platform\windows-app\app_source.png"
}
Write-Output "Loading source image: $srcPath"
$srcImg = [System.Drawing.Image]::FromFile($srcPath)

$sizes = @(16, 20, 24, 32, 40, 48, 64, 96, 128, 256)
$pngBytesList = New-Object System.Collections.Generic.List[byte[]]

foreach ($size in $sizes) {
    $bmp = New-Object System.Drawing.Bitmap $size, $size, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
    $g.Clear([System.Drawing.Color]::Transparent)

    $rect = New-Object System.Drawing.Rectangle 0, 0, $size, $size
    $g.DrawImage($srcImg, $rect, 0, 0, $srcImg.Width, $srcImg.Height, [System.Drawing.GraphicsUnit]::Pixel)
    $g.Dispose()

    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $pngBytesList.Add($ms.ToArray())
    $ms.Dispose()
    $bmp.Dispose()
}
$srcImg.Dispose()

# Construct ICO binary
$icoPath = "d:\Tools\pKey\platform\windows-app\app.ico"
$fs = [System.IO.File]::Create($icoPath)
$bw = New-Object System.IO.BinaryWriter $fs

$count = $sizes.Length
$bw.Write([uint16]0)
$bw.Write([uint16]1)
$bw.Write([uint16]$count)

$offset = 6 + (16 * $count)

for ($i = 0; $i -lt $count; $i++) {
    $size = $sizes[$i]
    $bytes = $pngBytesList[$i]
    $wByte = if ($size -ge 256) { [byte]0 } else { [byte]$size }
    $hByte = if ($size -ge 256) { [byte]0 } else { [byte]$size }

    $bw.Write($wByte)
    $bw.Write($hByte)
    $bw.Write([byte]0)
    $bw.Write([byte]0)
    $bw.Write([uint16]1)
    $bw.Write([uint16]32)
    $bw.Write([uint32]$bytes.Length)
    $bw.Write([uint32]$offset)

    $offset += $bytes.Length
}

for ($i = 0; $i -lt $count; $i++) {
    $bw.Write($pngBytesList[$i])
}

$bw.Close()
$fs.Close()
Write-Output "Successfully generated $icoPath with $($sizes.Length) sizes."
