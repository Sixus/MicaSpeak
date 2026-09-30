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
# Data/ 里有用户身份私钥，重建绿色目录时必须原样保留（实机上曾因清掉
# 运行中应用的 Data 丢失身份文件——打包前脚本也应先退出正在运行的应用）。
$dataDir = Join-Path $appDir 'Data'
$savedData = $null
if (Test-Path $dataDir) {
    $savedData = Join-Path $env:TEMP ('micaspeak-data-' + [guid]::NewGuid().ToString('N'))
    Move-Item $dataDir $savedData
}
if (Test-Path $appDir) { try { Remove-Item -Recurse -Force $appDir } catch { if ($savedData) { Move-Item $savedData $dataDir }; throw } }
New-Item -ItemType Directory -Path (Join-Path $appDir 'LICENSES\texts') -Force | Out-Null
if ($savedData) { Move-Item $savedData $dataDir }

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
    $url = if ($p.homepage) { $p.homepage } else { $p.manifest_path }
    $notices.Add(('{0} | {1} | {2} | {3}' -f $p.name, $p.version, $p.license, $url))
}
$noticesPath = Join-Path $appDir 'LICENSES\THIRD-PARTY-NOTICES.txt'
$notices | Out-File -FilePath $noticesPath -Encoding utf8

# ---- 许可证全文：按内容哈希去重拷贝（543 份多为重复的 MIT/Apache 文本，
#      逐 crate 全量拷贝会占 ~8MB，威胁 C3 的 20MB 上限）----
$hashToCanon = @{}
$crateToCanon = New-Object System.Collections.Generic.List[string]
$crateToCanon.Add('组件 | 许可证全文（LICENSES\texts\ 下同名文件）')
foreach ($p in $metadata.packages) {
    if ($p.name -eq 'micaspeak') { continue }
    $srcDir = Split-Path -Parent $p.manifest_path
    $licenseFiles = Get-ChildItem $srcDir -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|UNLICENSE)' }
    foreach ($f in $licenseFiles) {
        $hash = (Get-FileHash $f.FullName -Algorithm SHA256).Hash
        if (-not $hashToCanon.ContainsKey($hash)) {
            $canonName = $p.name + '-' + $p.version + '-' + $f.Name
            $hashToCanon[$hash] = $canonName
            Copy-Item $f.FullName (Join-Path (Join-Path $appDir 'LICENSES\texts') $canonName)
        }
        $crateToCanon.Add($p.name + ' ' + $p.version + ' | ' + $hashToCanon[$hash])
    }
}
$crateToCanon | Out-File -FilePath (Join-Path $appDir 'LICENSES\texts\MAPPING.txt') -Encoding utf8

# ---- 说明文件（M6c C2：安装/运行、SHA256、范围、私钥安全、已知限制） ----
$exeHash = (Get-FileHash $exe -Algorithm SHA256).Hash.ToLower()
$readme = @"
MicaSpeak —— 非官方 TeamSpeak 3 第三方客户端（绿色便携版）

本应用与 TeamSpeak Systems GmbH 无关联；"TeamSpeak" 仅为兼容性说明。
本应用仅支持 TeamSpeak 3 服务器（TS5/TS6 不在支持范围）。

【安装与运行】
1. 前置：Windows 10/11 x64，系统已安装 Microsoft Edge WebView2 (Evergreen)
   运行时。检查方法：设置 → 应用 → 搜索 "WebView2"；或 PowerShell：
   Get-ItemProperty 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' -ErrorAction SilentlyContinue
   缺失时到官方页面安装（应用启动时也会显示引导页）：
   https://developer.microsoft.com/microsoft-edge/webview2/
2. 解压后双击 MicaSpeak.exe 运行；本目录无安装器、不写注册表。
   应用自带 WebView2 状态检查，缺运行时会显示中文引导页。

【SHA256 校验】
MicaSpeak.exe:  $exeHash
ZIP 包校验值见同目录 MicaSpeak-portable.zip.sha256。
PowerShell 校验：Get-FileHash .\MicaSpeak.exe -Algorithm SHA256

【数据与身份私钥安全】
1. 配置、身份与 WebView 数据保存在本目录 Data/ 下；删除 Data/ 即恢复初始状态。
2. Data\identities\ 下的身份文件包含私钥：请勿外传、勿截图、勿上传网盘、
   勿交给同步盘。设置 → 身份 → 导出 会显示完整私钥，请确认环境安全。
3. 使用"导出身份"得到的字符串可在官方客户端导入，反之亦然（仅限无密码导出）。

【已知限制】
1. 悬浮窗在独占全屏的游戏画面中不可见（Windows 系统限制）；无边框窗口化可正常显示。
2. 当前台程序以管理员权限运行时，Windows UIPI 会阻止全局 PTT 热键生效；
   请以相同权限运行本应用，或改用窗口内按钮说话。
3. Win11 22H2（build 22621+）使用 Acrylic 系统材质；Win10/22H2 以下或
   启用失败时自动回退实体背景。启动参数 --force-fallback 可强制实体背景。

【第三方许可】
许可见 LICENSES\THIRD-PARTY-NOTICES.txt 与 LICENSES\texts\。
"@
$readme | Out-File -FilePath (Join-Path $appDir 'README.txt') -Encoding utf8

# ---- 校验：目录内不得出现 WebView2 运行时 ----
$forbidden = Get-ChildItem $appDir -Recurse -File |
    Where-Object { $_.Name -match '^(msedgewebview2|WebView2Loader|msedgewebview)' }
if ($forbidden) { throw "绿色目录中出现了 WebView2 运行时文件：$($forbidden.FullName)" }

$size = (Get-ChildItem $appDir -Recurse -File | Where-Object { $_.FullName -notlike "$dataDir*" } | Measure-Object Length -Sum).Sum
Write-Output ("绿色目录已生成：{0}" -f $appDir)
Write-Output ("应用目录体积（不含 Data/，字节）：{0:N0}  ≈ {1:N2} MB" -f $size, ($size / 1MB))

# ---- M6c C3：打 ZIP 包（不含 WebView2 Runtime， Evergreen 是系统前置） ----
$zipPath = Join-Path $outRoot 'MicaSpeak-portable.zip'
if (Test-Path $zipPath) { Remove-Item -Force $zipPath }
# Compress-Archive 对部分 crate 许可文件的非法 LastWriteTime 会抛异常，
# 改用系统自带 bsdtar（绝对路径，避免 PATH 里的 MSYS/Git tar 把 D: 当主机名）。
$tarExe = Join-Path $env:SystemRoot 'System32\tar.exe'
$parent = Split-Path -Parent $appDir
$leaf = Split-Path -Leaf $appDir
Push-Location $parent
try { & $tarExe -a -c -f $zipPath --exclude 'MicaSpeak/Data' $leaf; if ($LASTEXITCODE -ne 0) { throw "tar 打包失败（退出码 $LASTEXITCODE）" } }
finally { Pop-Location }
$zipHash = (Get-FileHash $zipPath -Algorithm SHA256).Hash.ToLower()
$shaLine = "$zipHash  MicaSpeak-portable.zip"
$shaLine | Out-File -FilePath ($zipPath + '.sha256') -Encoding ascii
Write-Output ("ZIP 已生成：{0}" -f $zipPath)
Write-Output ("ZIP SHA256：{0}" -f $zipHash)
Write-Output ("ZIP 体积：{0:N2} MB" -f ((Get-Item $zipPath).Length / 1MB))
