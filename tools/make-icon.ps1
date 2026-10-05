# Renders the FolderCompare app icon (see src/FolderCompare.App/Assets/app-icon.svg)
# into a multi-size .ico. Usage: pwsh tools/make-icon.ps1 [-Out <path>] [-PreviewDir <dir>]
param(
  [string]$Out = (Join-Path $PSScriptRoot '..\src\FolderCompare.App\Assets\app.ico'),
  [string]$PreviewDir
)
Add-Type -AssemblyName System.Drawing

function Fill-RoundRect($g, $brush, $x, $y, $w, $h, $r) {
  $p = New-Object System.Drawing.Drawing2D.GraphicsPath
  $d = 2 * $r
  $p.AddArc($x, $y, $d, $d, 180, 90); $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
  $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90); $p.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
  $p.CloseFigure(); $g.FillPath($brush, $p); $p.Dispose()
}
function Pt($x, $y) { New-Object System.Drawing.PointF $x, $y }
function Brush($hex) { New-Object System.Drawing.SolidBrush ([System.Drawing.ColorTranslator]::FromHtml($hex)) }

$entries = @()
foreach ($n in 16, 20, 24, 32, 40, 48, 64, 128, 256) {
  $bmp = New-Object System.Drawing.Bitmap $n, $n, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.SmoothingMode = 'AntiAlias'; $g.PixelOffsetMode = 'HighQuality'
  $g.Clear([System.Drawing.Color]::Transparent)
  $s = $n / 180.0
  $g.ScaleTransform($s, $s)   # draw in the SVG's 180x180 coordinate space

  Fill-RoundRect $g (Brush '#1E293B') 0 0 180 180 40
  $blue = Brush '#60A5FA'; $yellow = Brush '#FBBF24'
  Fill-RoundRect $g $blue 12 62 22 10 3;   Fill-RoundRect $g $blue 12 68 56 44 5
  Fill-RoundRect $g $yellow 112 62 22 10 3; Fill-RoundRect $g $yellow 112 68 56 44 5

  # Keep the arrows readable at small sizes: at least ~1.3 device pixels wide.
  $pen = New-Object System.Drawing.Pen ([System.Drawing.Color]::White), ([Math]::Max(3.0, 1.3 / $s))
  $pen.StartCap = 'Round'; $pen.EndCap = 'Round'; $pen.LineJoin = 'Round'
  $g.DrawLine($pen, 76, 84, 104, 84);  $g.DrawLines($pen, [System.Drawing.PointF[]]@((Pt 98 78), (Pt 104 84), (Pt 98 90)))
  $g.DrawLine($pen, 104, 100, 76, 100); $g.DrawLines($pen, [System.Drawing.PointF[]]@((Pt 82 94), (Pt 76 100), (Pt 82 106)))
  $g.Dispose()

  $ms = New-Object System.IO.MemoryStream
  $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
  if ($PreviewDir) { $bmp.Save((Join-Path $PreviewDir "icon_$n.png"), [System.Drawing.Imaging.ImageFormat]::Png) }
  $bmp.Dispose()
  $entries += , @($n, $ms.ToArray())
}

# ICO container with PNG-compressed entries.
$fs = [System.IO.File]::Create([System.IO.Path]::GetFullPath($Out))
$bw = New-Object System.IO.BinaryWriter $fs
$bw.Write([UInt16]0); $bw.Write([UInt16]1); $bw.Write([UInt16]$entries.Count)
$offset = 6 + 16 * $entries.Count
foreach ($e in $entries) {
  $dim = if ($e[0] -ge 256) { 0 } else { $e[0] }
  $bw.Write([byte]$dim); $bw.Write([byte]$dim); $bw.Write([byte]0); $bw.Write([byte]0)
  $bw.Write([UInt16]1); $bw.Write([UInt16]32); $bw.Write([UInt32]$e[1].Length); $bw.Write([UInt32]$offset)
  $offset += $e[1].Length
}
foreach ($e in $entries) { $bw.Write([byte[]]$e[1]) }
$bw.Close()
