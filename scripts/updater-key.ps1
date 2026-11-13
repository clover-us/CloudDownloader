param(
    [string]$Path = (Join-Path $HOME '.tauri/clouddownloader.key'),
    [string]$Password = ''
)
# 生成自动更新的签名密钥对。私钥只保存在本机，公钥需要写入 src-tauri/tauri.conf.json。
# 用法：powershell -ExecutionPolicy Bypass -File scripts/updater-key.ps1

$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$directory = Split-Path $Path
New-Item -ItemType Directory -Force -Path $directory | Out-Null
if (Test-Path $Path) { throw "私钥已存在：$Path。如需重新生成，请先备份并删除旧密钥。" }

Push-Location $projectRoot
try {
    # cmd.exe 才能把 "" 作为空参数传给 CLI（PowerShell 会丢弃空字符串）。
    if ($Password) {
        cmd /c "node node_modules/@tauri-apps/cli/tauri.js signer generate --ci -p `"$Password`" -w `"$Path`""
    } else {
        cmd /c "node node_modules/@tauri-apps/cli/tauri.js signer generate --ci -p `"`" -w `"$Path`""
    }
    if ($LASTEXITCODE -ne 0) { throw '密钥生成失败' }
    $public = "$Path.pub"
    Write-Host ''
    Write-Host "私钥：$Path（务必保密，不要提交到仓库）"
    Write-Host "公钥：$public"
    Write-Host ''
    Write-Host '把下面的公钥内容填入 src-tauri/tauri.conf.json 的 plugins.updater.pubkey：'
    Get-Content $public -Raw
} finally {
    Pop-Location
}
