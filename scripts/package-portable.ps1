# M5 C1：组装绿色便携目录（docs/09 阶段 C）。
# 前置：已执行 `pnpm tauri build`（产物 target/release/micaspeak.exe）。
# 产出：dist/MicaSpeak/{ MicaSpeak.exe, portable.dat, LICENSES/ , README.txt }
# - 不包含任何 WebView2 Runtime（Evergreen 是系统前置，C2 硬约束）。
# - 无安装器、无注册表写入；配置与 WebView 数据写 exe 旁 Data/。
# 用法：powershell -ExecutionPolicy Bypass -File scripts\package-portable.ps1
param(
    [string]$ProjectRoot = (Split-Path -Parent $PSScriptRoot)
)
$ErrorActionPreference = 'Stop'

$exe = Join-Path $ProjectRoot 'target\release\micaspeak.exe'
if (-not (Test-Path $exe)) { throw "未找到 $exe，请先运行 pnpm tauri build" }

$outRoot = Join-Path $ProjectRoot 'dist\MicaSpeak-portable'
$appDir = Join-Path $outRoot 'MicaSpeak'
if (Test-Path $appDir) { Remove-Item -Recurse -Force $appDir }
New-Item -ItemType Directory -Path (Join-Path $appDir 'LICENSES\texts') -Force | Out-Null

Copy-Item $exe (Join-Path $appDir 'MicaSpeak.exe')
New-Item -ItemType File -Path (Join-Path $appDir 'portable.dat') -Force | Out-Null

# ---- 第三方许可清单（C7/§9）：cargo metadata 提供每个 crate 的许可证 ----
$metadataJson = & cargo metadata --format-version 1 --manifest-path (Join-Path $ProjectRoot 'src-tauri\Cargo.toml') 2>$null
$metadata = $metadataJson | ConvertFrom-Json
$notices = New-Object System.Collections.Generic.List[string]
$notices.Add('MicaSpeak 第三方组件许可清单（THIRD-PARTY NOTICES）')
$notices.Add('生成时间：' + (Get-Date -Format 'yyyy-MM-dd HH:mm'))
$notices.Add('')
$notices.Add('MicaSpeak 本体遵循与下列组件一致的宽松许可发布。本应用不复制任何')
$notices.Add('GPL/AGPL/无许可证项目代码；"TeamSpeak" 仅用于兼容性说明，本应用为')
$notices.Add('非官方第三方客户端。')
$notices.Add('')
$notices.Add('组件名称 | 版本 | 许可证 | 主页')
$notices.Add('--------------------------------------------------------------')
foreach ($p in ($metadata.packages | Sort-Object name)) {
    if ($p.name -eq 'micaspeak') { continue }
    $home = if ($p.homepage) { $p.homepage } else { $p.manifest_path }
    $notices.Add(('{0} | {1} | {2} | {3}' -f $p.name, $p.version, $p.license, $home))
}
$noticesPath = Join-Path $appDir 'LICENSES\THIRD-PARTY-NOTICES.txt'
$notices | Out-File -FilePath $noticesPath -Encoding utf8

# ---- 许可证全文：从各 crate 的源码目录拷贝 LICENSE/COPYING ----
$srcRoots = Get-ChildItem "$env:USERPROFILE\.cargo\registry\src\*" -Directory -ErrorAction SilentlyContinue
$gitRoots = Get-ChildItem "$env:USERPROFILE\.cargo\git\checkouts\*" -Directory -ErrorAction SilentlyContinue
foreach ($p in $metadata.packages) {
    if ($p.name -eq 'micaspeak') { continue }
    $manifest = $p.manifest_path  # ...\crate-x.y.z\Cargo.toml 或 git checkout 内路径
    $srcDir = Split-Path -Parent $manifest
    $licenseFiles = Get-ChildItem $srcDir -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|UNLICENSE)' }
    if ($licenseFiles) {
        $dest = Join-Path $appDir ("LICENSES\texts\" + $p.name + '-' + $p.version)
        New-Item -ItemType Directory -Path $dest -Force | Out-Null
        foreach ($f in $licenseFiles) { Copy-Item $f.FullName $dest }
    }
}

# ---- 说明文件 ----
@'
MicaSpeak —— 非官方 TeamSpeak 3 第三方客户端（绿色便携版）

运行前置：Windows 10/11 x64，且系统已安装 Microsoft Edge WebView2
(Evergreen) 运行时。缺失时请安装：
https://developer.microsoft.com/microsoft-edge/webview2/

使用方法：
1. 双击 MicaSpeak.exe 运行；本目录无安装器、不写注册表。
2. 配置、身份与 WebView 数据保存在本目录 Data/ 下；删除 Data/
   即恢复初始状态（身份文件 identity.json 含私钥，请勿外传）。
3. 启动参数 --force-fallback：强制使用实体背景（跳过 Win11 Mica）。

本应用与 TeamSpeak Systems GmbH 无关联；"TeamSpeak" 仅为兼容性说明。
第三方组件许可见 LICENSES\THIRD-PARTY-NOTICES.txt。
'@ | Out-File -FilePath (Join-Path $appDir 'README.txt') -Encoding utf8

# ---- 校验：目录内不得出现 WebView2 运行时 ----
$forbidden = Get-ChildItem $appDir -Recurse -File |
    Where-Object { $_.Name -match '^(msedgewebview2|WebView2Loader|msedgewebview)' }
if ($forbidden) { throw "绿色目录中出现了 WebView2 运行时文件：$($forbidden.FullName)" }

$size = (Get-ChildItem $appDir -Recurse -File | Measure-Object Length -Sum).Sum
Write-Output ("绿色目录已生成：{0}" -f $appDir)
Write-Output ("应用目录体积（不含 Data/，字节）：{0:N0}  ≈ {1:N2} MB" -f $size, ($size / 1MB))
