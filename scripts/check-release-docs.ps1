# 发布前文档一致性检查：版本号、Release 说明、README / 使用指南 / 验证记录。
# 由 scripts/build.ps1 在打包前调用，也可单独执行：
#   powershell -ExecutionPolicy Bypass -File scripts/check-release-docs.ps1
$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$problems = New-Object 'System.Collections.Generic.List[string]'

function Read-Text([string]$relative) {
    $path = Join-Path $projectRoot $relative
    if (-not (Test-Path -LiteralPath $path)) {
        $problems.Add("缺少文件：$relative")
        return $null
    }
    # 按字节自行解码：Get-Content 的默认编码在非 UTF-8 代码页的机器上会读坏中文。
    [Text.Encoding]::UTF8.GetString([IO.File]::ReadAllBytes($path))
}

$packageJson = Read-Text 'package.json'
$tauriJson = Read-Text 'src-tauri/tauri.conf.json'
$cargoToml = Read-Text 'src-tauri/Cargo.toml'
if ($problems.Count -gt 0) {
    $problems | ForEach-Object { Write-Host "  - $_" }
    throw '发布前文档检查未通过：缺少版本号来源文件'
}

$packageVersion = ($packageJson | ConvertFrom-Json).version
$tauriVersion = ($tauriJson | ConvertFrom-Json).version
# 只取 [package] 段，避免把依赖表里的 version 当成包版本。
$cargoVersion = $null
if ($cargoToml -match '(?m)^\[package\]\s*$') {
    $packageSection = ($cargoToml -split '(?m)^\[package\]\s*$', 2)[1]
    $packageSection = ($packageSection -split '(?m)^\[')[0]
    if ($packageSection -match '(?m)^\s*version\s*=\s*"([^"]+)"') { $cargoVersion = $Matches[1] }
}
if (-not $cargoVersion) { $problems.Add('src-tauri/Cargo.toml 的 [package] 段没有 version') }

if ($cargoVersion -and (@($packageVersion, $tauriVersion, $cargoVersion) | Select-Object -Unique).Count -ne 1) {
    $problems.Add("三处版本号不一致：package.json=$packageVersion，tauri.conf.json=$tauriVersion，Cargo.toml=$cargoVersion")
}
$version = $packageVersion
$v = [regex]::Escape($version)

$releaseNotes = Read-Text "docs/release-$version.md"
if ($releaseNotes -and $releaseNotes -notmatch "(?m)^#\s+CloudDownloader\s+$v\s*$") {
    $problems.Add("docs/release-$version.md 的标题不是「# CloudDownloader $version」")
}

$readme = Read-Text 'README.md'
if ($readme) {
    if ($readme -notmatch [regex]::Escape("最新版本 **$version**")) {
        $problems.Add("README.md 的版本说明没有写成「最新版本 **$version**」")
    }
    if ($readme -notmatch [regex]::Escape("(docs/release-$version.md)")) {
        $problems.Add("README.md 的更新说明没有指向 docs/release-$version.md")
    }
    foreach ($ref in 'installer', 'portable', 'checksums') {
        $match = [regex]::Match($readme, "(?m)^\[$ref\]:\s*(\S+)\s*$")
        if (-not $match.Success) {
            $problems.Add("README.md 缺少 [$ref] 链接定义")
        } elseif ($match.Groups[1].Value -notmatch [regex]::Escape("/v$version/")) {
            $problems.Add("README.md 的 [$ref] 链接没有指向 v$version：$($match.Groups[1].Value)")
        }
    }
}

$usage = Read-Text 'docs/usage.md'
if ($usage) {
    if ($usage -notmatch [regex]::Escape("releases/tag/v$version")) {
        $problems.Add("docs/usage.md 没有指向 v$version 的 Release")
    }
    if ($usage -notmatch [regex]::Escape("CloudDownloader_${version}_x64-setup.exe")) {
        $problems.Add("docs/usage.md 的安装包文件名不是 $version")
    }
}

$verification = Read-Text 'VERIFICATION.md'
if ($verification -and $verification -notmatch "当前验证对象为\s*$v") {
    $problems.Add("VERIFICATION.md 没有写成「当前验证对象为 $version」")
}

if ($problems.Count -gt 0) {
    $problems | ForEach-Object { Write-Host "  - $_" }
    throw "发布前文档检查未通过（$($problems.Count) 项，版本 $version）"
}
Write-Host "发布前文档检查通过：$version（版本号、更新说明、README、使用指南、验证记录已对齐）"