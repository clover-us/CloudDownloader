$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$running = Get-Process clouddownloader -ErrorAction SilentlyContinue | Where-Object {
    $_.Path -and $_.Path.StartsWith($projectRoot + '\', [StringComparison]::OrdinalIgnoreCase)
}
if ($running) { throw '请先从托盘菜单退出正在运行的 CloudDownloader，再执行打包。' }
Push-Location $projectRoot
try {
    if (-not (Test-Path 'node_modules')) {
        npm.cmd ci
        if ($LASTEXITCODE -ne 0) { throw 'npm ci 失败' }
    }
    cargo fmt --manifest-path src-tauri/Cargo.toml --check
    if ($LASTEXITCODE -ne 0) { throw 'Rust 格式检查失败' }
    cargo test --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw 'Rust 测试失败' }
    if (-not $env:TAURI_SIGNING_PRIVATE_KEY) {
        $keyPath = Join-Path $HOME '.tauri/clouddownloader.key'
        if (-not (Test-Path $keyPath)) {
            throw '缺少自动更新签名私钥。请先执行 powershell -ExecutionPolicy Bypass -File scripts/updater-key.ps1，或设置 TAURI_SIGNING_PRIVATE_KEY。'
        }
        $env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content $keyPath -Raw).Trim()
    }
    node node_modules/@tauri-apps/cli/tauri.js build --bundles nsis
    if ($LASTEXITCODE -ne 0) { throw 'Tauri 构建失败' }
    & (Join-Path $PSScriptRoot 'package.ps1') -IncludeInstaller
    & (Join-Path $PSScriptRoot 'latest-json.ps1')
} finally {
    Pop-Location
}
