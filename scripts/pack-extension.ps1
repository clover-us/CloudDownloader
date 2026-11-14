param(
    [string]$Out = ''
)

# Build a zip of browser/extension for store upload or manual unpacked install.
# Usage: powershell -ExecutionPolicy Bypass -File scripts/pack-extension.ps1 [-Out path.zip]

$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$source = Join-Path $projectRoot 'extension'
$manifestPath = Join-Path $source 'manifest.json'
if (-not (Test-Path -LiteralPath $manifestPath)) { throw "missing $manifestPath" }

$manifest = Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
$version = $manifest.version

foreach ($icon in $manifest.icons.PSObject.Properties) {
    $iconPath = Join-Path $source $icon.Value
    if (-not (Test-Path -LiteralPath $iconPath)) { throw "manifest lists $($icon.Value) but the file is missing" }
}

if (-not $Out) {
    $Out = Join-Path $projectRoot "package/clouddownloader-extension-$version.zip"
}
$Out = [IO.Path]::GetFullPath($Out)
$outDir = Split-Path -Parent $Out
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
if (Test-Path -LiteralPath $Out) { Remove-Item -LiteralPath $Out -Force }

Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem

$files = @(Get-ChildItem -LiteralPath $source -File -Recurse | Sort-Object FullName)
if ($files.Count -eq 0) { throw 'extension/ has no files' }

$zip = [System.IO.Compression.ZipFile]::Open($Out, [System.IO.Compression.ZipArchiveMode]::Create)
try {
    foreach ($file in $files) {
        # Zip entries always use forward slashes; Chrome rejects backslash paths.
        $relative = $file.FullName.Substring($source.Length + 1).Replace('\', '/')
        $entry = $zip.CreateEntry($relative, [System.IO.Compression.CompressionLevel]::Optimal)
        $entry.LastWriteTime = $file.LastWriteTime
        $stream = $entry.Open()
        try {
            $bytes = [IO.File]::ReadAllBytes($file.FullName)
            $stream.Write($bytes, 0, $bytes.Length)
        }
        finally { $stream.Dispose() }
    }
}
finally { $zip.Dispose() }

$size = (Get-Item -LiteralPath $Out).Length
Write-Host ('Packed extension {0}: {1} ({2:N0} bytes, {3} files)' -f $version, $Out, $size, $files.Count)