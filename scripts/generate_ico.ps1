Add-Type -AssemblyName System.Drawing

$sizes = @(16, 32, 48, 64, 128, 256)
$pngBytesList = New-Object System.Collections.Generic.List[byte[]]

foreach ($size in $sizes) {
    $bmp = New-Object System.Drawing.Bitmap $size, $size, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.Clear([System.Drawing.Color]::Transparent)

    $pad = [Math]::Max(1.0, $size * 0.05)
    $rect = New-Object System.Drawing.RectangleF $pad, $pad, ($size - 2 * $pad), ($size - 2 * $pad)
    $radius = $size * 0.22

    # Squircle path
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $diameter = $radius * 2
    $arc = New-Object System.Drawing.RectangleF $rect.X, $rect.Y, $diameter, $diameter
    $path.AddArc($arc, 180, 90)
    $arc.X = $rect.Right - $diameter
    $path.AddArc($arc, 270, 90)
    $arc.Y = $rect.Bottom - $diameter
    $path.AddArc($arc, 0, 90)
    $arc.X = $rect.Left
    $path.AddArc($arc, 90, 90)
    $path.CloseFigure()

    # Gradient fill: Electric Blue to Cobalt Deep Blue
    $brush = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
        $rect,
        [System.Drawing.Color]::FromArgb(255, 13, 132, 255),
        [System.Drawing.Color]::FromArgb(255, 0, 85, 204),
        [System.Drawing.Drawing2D.LinearGradientMode]::Vertical
    )
    $g.FillPath($brush, $path)
    $brush.Dispose()

    # Top specular sheen (glass reflection)
    $sheenRect = New-Object System.Drawing.RectangleF ($rect.X + 1), ($rect.Y + 1), ($rect.Width - 2), ($rect.Height * 0.42)
    $sheenBrush = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
        $sheenRect,
        [System.Drawing.Color]::FromArgb(70, 255, 255, 255),
        [System.Drawing.Color]::FromArgb(0, 255, 255, 255),
        [System.Drawing.Drawing2D.LinearGradientMode]::Vertical
    )
    $g.FillRectangle($sheenBrush, $sheenRect)
    $sheenBrush.Dispose()

    # Subtle inner border
    $borderPen = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(40, 255, 255, 255)), 1
    $g.DrawPath($borderPen, $path)
    $borderPen.Dispose()

    # Luminous status dot (Concept A) at top right
    $dotR = [Math]::Max(1.5, $size * 0.055)
    $dotCx = $rect.Right - $radius * 0.7
    $dotCy = $rect.Top + $radius * 0.7
    $dotBrush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 100, 230, 255))
    $g.FillEllipse($dotBrush, ($dotCx - $dotR), ($dotCy - $dotR), ($dotR * 2), ($dotR * 2))
    $dotBrush.Dispose()

    # Text "pK"
    $fontSize = [Math]::Max(8.0, $size * 0.44)
    $font = New-Object System.Drawing.Font "Segoe UI", $fontSize, ([System.Drawing.FontStyle]::Bold), ([System.Drawing.GraphicsUnit]::Pixel)
    $format = New-Object System.Drawing.StringFormat
    $format.Alignment = [System.Drawing.StringAlignment]::Center
    $format.LineAlignment = [System.Drawing.StringAlignment]::Center
    
    $textRect = New-Object System.Drawing.RectangleF 0, ($size * 0.02), $size, $size
    $textBrush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 255, 255, 255))
    $g.DrawString("pK", $font, $textBrush, $textRect, $format)
    
    $textBrush.Dispose()
    $format.Dispose()
    $font.Dispose()
    $path.Dispose()
    $g.Dispose()

    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $pngBytesList.Add($ms.ToArray())
    $ms.Dispose()
    $bmp.Dispose()
}

# Construct ICO binary
$icoPath = "d:\Tools\pKey\platform\windows-app\app.ico"
$fs = [System.IO.File]::Create($icoPath)
$bw = New-Object System.IO.BinaryWriter $fs

# Header: reserved(2) + type(2, 1=ico) + count(2)
$count = $sizes.Length
$bw.Write([uint16]0)
$bw.Write([uint16]1)
$bw.Write([uint16]$count)

# Compute offsets: header(6) + directory entries(16 * count)
$offset = 6 + (16 * $count)

for ($i = 0; $i -lt $count; $i++) {
    $size = $sizes[$i]
    $bytes = $pngBytesList[$i]
    $wByte = if ($size -ge 256) { [byte]0 } else { [byte]$size }
    $hByte = if ($size -ge 256) { [byte]0 } else { [byte]$size }

    $bw.Write($wByte)            # width
    $bw.Write($hByte)            # height
    $bw.Write([byte]0)           # colors
    $bw.Write([byte]0)           # reserved
    $bw.Write([uint16]1)         # planes
    $bw.Write([uint16]32)        # bpp
    $bw.Write([uint32]$bytes.Length) # data size
    $bw.Write([uint32]$offset)   # data offset

    $offset += $bytes.Length
}

# Write PNG images data
for ($i = 0; $i -lt $count; $i++) {
    $bw.Write($pngBytesList[$i])
}

$bw.Close()
$fs.Close()
Write-Output "Successfully generated $icoPath with $($sizes.Length) sizes."
