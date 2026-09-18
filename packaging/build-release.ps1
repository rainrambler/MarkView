<#
    把 Windows 版打成一个可以直接传上 GitHub Release 的 zip。

        pwsh -File packaging/build-release.ps1

    产物（都在 dist/ 下，`/dist` 已经在 .gitignore 里）：
        dist/markview-<版本>-<target>/        解压出来的样子，方便先看一眼
        dist/markview-<版本>-<target>.zip     要上传的那个文件
        dist/SHA256SUMS.txt                   校验和（sha256sum -c 认的格式）

    和 packaging/build-app.sh 一样，**版本号只认 Cargo.toml** —— 两处各写一份的话，
    迟早会出现"tag 是 v0.2.0、压缩包名字里还是 0.1.0"这种事。
#>

param(
    # 仓库根目录，默认取脚本所在目录的上一层
    [string]$Root
)

$ErrorActionPreference = 'Stop'
if (-not $Root) { $Root = Split-Path -Parent $PSScriptRoot }
$Root = (Resolve-Path $Root).Path

Push-Location $Root
try {
    # ---------------------------------------------------------------- 版本 / 目标平台
    $version = (Select-String -Path 'Cargo.toml' -Pattern '^version\s*=\s*"([^"]+)"' |
        Select-Object -First 1).Matches[0].Groups[1].Value
    if (-not $version) { throw '读不出 Cargo.toml 里的 version' }

    # 用 host 三连而不是写死一个常量：同一份脚本在别的机器上也要打得出对的名字
    $target = (& rustc -vV | Where-Object { $_ -like 'host:*' }) -replace '^host:\s*', ''
    if (-not $target) { throw 'rustc -vV 里没有 host 行' }

    $name = "markview-$version-$target"
    Write-Host "==> $name"

    # ---------------------------------------------------------------- 构建
    $exe = Join-Path 'target\release' 'markview.exe'
    if (-not (Test-Path $exe)) {
        Write-Host '==> 构建 release'
        cargo build --release --locked
        if ($LASTEXITCODE -ne 0) { throw 'cargo build --release --locked 失败' }
    } else {
        Write-Host "==> 复用已存在的 $exe（要强制重建就删掉它再跑）"
    }

    # ---------------------------------------------------------------- 打包前先验图标
    #
    # 资源管理器读的是 PE 里的图标资源。漏了它，用户拿到的是一个通用图标的 exe，
    # 而 zip 本身完全正常、校验和也对 —— 这种问题只有装到别人机器上才看得出来，
    # 所以在打包这一步就拦下来。
    if (-not ('MarkView.IconProbe' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

namespace MarkView {
    public static class IconProbe {
        [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
        static extern int ExtractIconEx(string file, int index, IntPtr[] large, IntPtr[] small, int count);

        public static int Count(string path) => ExtractIconEx(path, -1, null, null, 0);
    }
}
'@
    }

    $icons = [MarkView.IconProbe]::Count((Resolve-Path $exe).Path)
    Write-Host "==> exe 里的图标资源：$icons 个"
    if ($icons -lt 1) {
        throw 'exe 里没有图标资源 —— build.rs / assets/markview.rc 没生效？'
    }

    # ---------------------------------------------------------------- 组装
    $dist = Join-Path $Root 'dist'
    $stage = Join-Path $dist $name
    if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $stage | Out-Null

    $files = @($exe, 'README.md', 'README.zh-CN.md', 'LICENSE')
    foreach ($file in $files) {
        if (-not (Test-Path $file)) { throw "缺文件：$file" }
        Copy-Item $file -Destination $stage
    }

    # 压缩包是**平铺**的：就一个 exe 加三个说明文件，多套一层目录只会让人多解一次。
    $zipPath = Join-Path $dist "$name.zip"
    if (Test-Path $zipPath) { Remove-Item $zipPath -Force }
    Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zipPath -CompressionLevel Optimal

    # ---------------------------------------------------------------- 校验和
    # 格式照抄 coreutils 的 sha256sum：小写十六进制 + 两个空格 + 文件名。
    #
    # 行尾**必须是 LF**：PowerShell 的 Set-Content 在 Windows 上写 CRLF，而
    # `sha256sum -c` 会把行尾那个 \r 当成文件名的一部分，于是在 Linux/macOS 上
    # 报 "markview-....zip\r: No such file or directory" —— 一个只有别人会踩到的坑。
    $hash = (Get-FileHash $zipPath -Algorithm SHA256).Hash.ToLower()
    $sums = Join-Path $dist 'SHA256SUMS.txt'
    [System.IO.File]::WriteAllText($sums, "$hash  $name.zip`n", [System.Text.Encoding]::ASCII)

    # ---------------------------------------------------------------- 回读压缩包
    # "打包成功了"和"包能解开"是两件事，顺手看一眼里面到底是什么。
    Add-Type -AssemblyName System.IO.Compression.FileSystem -ErrorAction SilentlyContinue
    $archive = [System.IO.Compression.ZipFile]::OpenRead($zipPath)
    try {
        Write-Host "==> $name.zip 里："
        foreach ($entry in $archive.Entries | Sort-Object FullName) {
            Write-Host ("    {0,-20} {1,12:N0} 字节" -f $entry.FullName, $entry.Length)
        }
    } finally {
        $archive.Dispose()
    }

    $zipSize = (Get-Item $zipPath).Length
    Write-Host ''
    Write-Host "==> 完成：$zipPath（$('{0:N0}' -f $zipSize) 字节）"
    Write-Host "    SHA-256：$hash"
    Write-Host ''
    Write-Host '    上传到 GitHub Release 时，附件就是上面这个 zip 和 dist/SHA256SUMS.txt。'
} finally {
    Pop-Location
}
