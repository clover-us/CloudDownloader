param(
    [string]$Notes = '',
    # 安装包实际发布到的仓库。默认是本仓库的 fork；向原作者仓库发布时传 -Repository sclliang/CloudDownloader。
    [string]$Repository = 'clover-us/CloudDownloader'
)
# 生成 Tauri 自动更新清单 latest.json。
# 用法：powershell -ExecutionPolicy Bypass -File scripts/latest-json.ps1
# 更新说明默认取自 docs/release-<版本>.md（避免命令行传中文时的编码问题），也可用 -Notes 覆盖。
# 前置：已完成 npm run tauri build --bundles nsis，且签名私钥可用。

$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$conf = Get-Content (Join-Path $projectRoot 'src-tauri/tauri.conf.json') -Encoding UTF8 -Raw | ConvertFrom-Json
$version = $conf.version
if (-not $Notes) {
    $releaseNotes = Join-Path $projectRoot "docs/release-$version.md"
    if (Test-Path $releaseNotes) {
        # Read the bytes ourselves: Get-Content's encoding handling mangles
        # non-ASCII release notes on machines whose ANSI code page is not UTF-8.
        $text = [Text.Encoding]::UTF8.GetString([IO.File]::ReadAllBytes($releaseNotes))
        $body = @($text -split "\r?\n" | Where-Object { $_ -notmatch '^#\s' })
        $firstSection = @()
        foreach ($line in $body) {
            if ($line -match '^##\s') { break }
            $firstSection += $line
        }
        $Notes = ($firstSection -join "`n").Trim()
    }
}
# 以 package/ 里最终交付、重新签名的安装包为准；bundle 里的那份只是中间产物。
$installerDirs = @(
    (Join-Path $projectRoot 'package'),
    (Join-Path $projectRoot 'src-tauri/target/release/bundle/nsis')
)
$installer = $null
foreach ($dir in $installerDirs) {
    $installer = Get-ChildItem $dir -Filter '*setup.exe' -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -like "*$version*" } |
        Select-Object -First 1
    if ($installer) { break }
}
if (-not $installer) { throw "未找到 $version 的 NSIS 安装包，请先执行 scripts/build.ps1 或 npm run tauri build --bundles nsis" }
$signatureFile = "$($installer.FullName).sig"
if (-not (Test-Path $signatureFile)) { throw '缺少安装包签名文件（.sig），请确认打包时已提供签名私钥。' }

$signature = (Get-Content $signatureFile -Raw).Trim()
$url = "https://github.com/$Repository/releases/download/v$version/$($installer.Name)"
$platform = @{
    signature = $signature
    url       = $url
}
$manifest = [ordered]@{
    version   = $version
    notes     = $Notes
    pub_date  = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = [ordered]@{
        'windows-x86_64'       = $platform
        'windows-x86_64-nsis'  = $platform
        'windows-x86_64-msi'   = $platform
    }
}
$target = Join-Path $projectRoot 'package/latest.json'
New-Item -ItemType Directory -Force -Path (Split-Path $target) | Out-Null
# 不带 BOM 的 UTF-8：更新器按严格 JSON 解析，BOM 会导致读取失败。
$json = $manifest | ConvertTo-Json -Depth 6
[System.IO.File]::WriteAllText($target, $json, (New-Object System.Text.UTF8Encoding($false)))
Write-Host "已生成 $target"
Write-Host "请把 $($installer.Name)、$($installer.Name).sig 和 latest.json 上传到 v$version 的 GitHub Release。"
