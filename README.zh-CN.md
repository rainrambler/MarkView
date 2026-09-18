# MarkView

[English](README.md) · **简体中文**

一个用 Rust 写的原生 Markdown 编辑器 / 查看器：**左边写源码，右边实时渲染**。
不需要 Node、不需要浏览器、不套系统 WebView —— 编译出来就是一个可直接双击运行的二进制。

![MarkView 打开一篇 Markdown 文档，左边编辑、右边实时预览](assets/screenshots/app.png)

```bash
cargo run                     # 打开空白文档
cargo run -- 笔记.md           # 直接打开指定文件（也支持把文件拖进窗口）
```

## 界面

```
┌──────────────────────────────────────────────────────────────┐
│ 文件  编辑  视图  帮助                                    ● 未保存 │
├──────────┬───────────────────────────┬───────────────────────┤
│ 大纲      │ 1  # 标题                 │  标题                 │
│  H1 标题  │ 2                          │                       │
│  H2 小节  │ 3  正文 **加粗**          │  正文 加粗            │
│  H3 细节  │ 4                          │                       │
├──────────┴───────────────────────────┴───────────────────────┤
│ 笔记.md │ 3:12 │ 37 行 213 词 446 字符 │ LF │ 分栏 编辑 预览 │
└──────────────────────────────────────────────────────────────┘
```

## 功能

**编辑**
- 行号槽（软换行的续行不编号，长行换行后行号依然对齐）
- Markdown 语法高亮，主题跟随深浅色
- 当前行左侧有强调竖条
- 撤销 / 重做（⌘Z / ⌘⇧Z）
- 一键格式化：加粗、斜体、删除线、行内代码、链接、列表、引用、标题
- 查找栏：⌘F，实时匹配计数，⌘G / ⌘⇧G 上下跳转

**预览**
- GFM 全量支持：表格、删除线、任务列表、脚注、告警框
- 代码块语法高亮（syntect），行内图片和本地图片都能显示
- **Mermaid 流程图直接画出来**（纯 Rust 渲染，不需要 Node / 浏览器），语法错了退回显示源码
- **预览里的任务列表复选框可以直接点**，改动会写回源文本

**文件**
- 系统原生文件对话框（rfd）
- 保存时原样保留 CRLF / LF 换行风格与 UTF-8 BOM，不污染 git diff
- 导出为 HTML（自带样式，跟随系统深浅色，可直接打印）
- 最近打开列表、拖放文件、`markview 文件.md` 命令行传参
- 未保存改动在关窗 / 换文件前会拦下来确认

**界面**
- 大纲侧栏，点击跳转到对应行（会正确识别并跳过围栏代码块里的 `#`）
- 三种布局：只看编辑 / 左右分栏 / 只看预览（⌘1 / ⌘2 / ⌘3）
- 深浅色主题（跟随系统 / 浅色 / 深色），⌘+ / ⌘- 缩放
- 窗口大小、面板宽度、偏好设置、最近文件都会记住
- **界面支持英文和简体中文**，运行中随时从「语言」菜单切换

## 快捷键

macOS 上是 ⌘，其他平台 `Command` 对应 Ctrl。

| 键 | 作用 |
| --- | --- |
| ⌘N / ⌘O | 新建 / 打开 |
| ⌘S / ⌘⇧S | 保存 / 另存为 |
| ⌘E | 导出为 HTML |
| ⌘1 / ⌘2 / ⌘3 | 只看编辑 / 左右分栏 / 只看预览 |
| ⌘⇧O / ⌘⇧L | 大纲 / 行号 |
| ⌘⇧T | 切换配色 |
| ⌘+ / ⌘- / ⌘0 | 放大 / 缩小 / 重置 |
| ⌘Z / ⌘⇧Z | 撤销 / 重做 |
| ⌘B / ⌘I / ⌘⇧X | 加粗 / 斜体 / 删除线 |
| ⌘K / ⌘⇧C | 链接 / 行内代码 |
| ⌘⇧U / ⌘⇧P | 无序列表 / 引用 |
| ⌘⌥1 / ⌘⌥2 / ⌘⌥3 | 一 / 二 / 三级标题 |
| ⌘F | 查找 |
| ⌘G / ⌘⇧G | 下一个 / 上一个匹配 |

## 构建

需要 Rust **1.95 或更新**（本 crate 用了 edition 2024）。

```bash
git clone <本仓库>
cd markview
cargo build --release
./target/release/markview
```

或者装进 `PATH`：

```bash
cargo install --path .
```

### macOS 应用包

macOS 忽略窗口级别的图标，Dock 和 Finder 只认 `.app` bundle 里 `CFBundleIconFile` 指的
`.icns`。想要图标就得打成 bundle：

```bash
packaging/build-app.sh          # -> dist/MarkView.app
```

### Windows 可执行文件图标

资源管理器读的是**可执行文件自己的资源**，所以图标必须在链接期就写进 exe ——
`main.rs` 里的 `ViewportBuilder::with_icon` 只管标题栏和任务栏，没有任何运行时接口能改
exe 文件本身的图标。这件事由 `build.rs` 完成：它把 `assets/markview.rc` 编成资源，
而那份脚本指向 `assets/AppIcon.ico`。

```powershell
cargo build --release           # target/release/markview.exe 里已经带图标了
```

和其他图标一样，`.ico` 也是画出来的，不是手绘或从设计工具导出的：

```bash
cargo run --release --example make_icon    # 重写 assets/AppIcon.ico（macOS 上还有 .icns）
```

### Windows 发布包

```powershell
pwsh -File packaging/build-release.ps1      # -> dist/markview-<版本>-<target>.zip
```

脚本用 `--locked` 构建，发现 exe 里没有图标资源就拒绝打包，最后产出 zip 和一份
`sha256sum -c` 认的 `SHA256SUMS.txt`。macOS 的产物走 `packaging/build-app.sh`。

## 代码结构

```
build.rs          构建脚本：把 Windows 图标资源链进 exe（见上）
examples/
  make_icon.rs    用代码画出每个尺寸的图标，含多尺寸 .ico
assets/           生成出来的图标：AppIcon.ico（Windows）/ AppIcon.icns（macOS）/ PNG 母版
src/
  main.rs          入口：窗口选项、拖放支持、渲染后端选择
  app.rs           应用状态机：Action 队列、主循环、快捷键表
  document.rs      文档模型：磁盘 I/O、CRLF/BOM 保真、脏标记
  prefs.rs         偏好设置（serde 持久化到 eframe storage）
  i18n.rs          界面文案表（英文 / 简体中文）
  markdown.rs      纯逻辑：大纲提取、字数统计、HTML 导出
  mermaid.rs       纯逻辑：按 mermaid 围栏切段、渲染与位图缓存
  fonts.rs         中文字体加载
  ui/
    mod.rs         主题与样式
    menu.rs        顶部菜单栏
    editor.rs      编辑区：行号、语法高亮、查找栏、格式化操作
    preview.rs     预览区
    outline.rs     大纲侧栏
    status.rs      底部状态栏
    dialogs.rs     关于 / 快捷键 / 未保存确认
    find.rs        查找状态与匹配逻辑
tests/fixtures/    手工验收用的样本文件（见「测试」）
```

### 三个值得说明的设计

**交互分两步走。** UI 代码只往一个 `Vec<Action>` 里塞"用户想干什么"，等所有面板画完之后再统一执行。
这样菜单项和快捷键共用同一套处理逻辑，也不会在画 UI 的时候和借用检查器打架。
`SaveAndProceed` 这类动作还会往队尾追加新动作，所以处理循环是可增长的。

**行号对齐靠 galley 而不是猜。** `TextEdit::show()` 会把排好版的 `Galley` 连同它在屏幕上的位置
（`galley_pos`）一起返回，而 `Galley.rows` 里每一行都带 `pos`（相对偏移）和 `ends_with_newline`。
于是"第 N 个逻辑行画在哪个 y"可以精确算出来 —— 即使某行因为太长被软换行成了好几个屏幕行，
编号也只标在逻辑行首、**不会错位**。这个行为有单元测试钉住
（`soft_wrapped_continuation_does_not_start_a_new_line`）。

**Mermaid 只能靠自己切段。** `egui_commonmark` 没给"自定义代码块渲染"留钩子
（只开放了行内 HTML 和公式两个回调），围栏代码块一律走语法高亮。所以预览遇到 `mermaid` 围栏时
把正文按围栏切成若干段：Markdown 段仍交给它，Mermaid 段由 `merman`（纯 Rust，
不需要 Node / 浏览器 / 子进程）渲染成位图贴上去。切段是有代价的 —— 隔着围栏的语法
（比如脚注引用和脚注定义被分到两段）不会互相解析，所以只有真的出现 mermaid 围栏时才走这条路。

位图按 (源码, 深浅色, 缩放比, 底色) 缓存，只有改过的那一段会重算；而重算不便宜
（debug 构建下小图一次约半秒，大头在解析、布局和 resvg 光栅化），所以改完还要再等
250ms 才真去重画：打字期间先拿上一张图顶着，手停下来才换成新的。配色走 `merman` 的
host theme，把 egui 的页面底色和深浅色喂进去 —— 默认管线会照搬 Mermaid 的行为往根节点写
`background-color:white`，深色界面里那就是一块刺眼的白板（这个坑有单元测试钉住）。

## 测试

```bash
cargo test
```

- `ui/editor.rs`：在无头 `egui::Context` 里真的排一次版，验证软换行续行不编号、
  行号 y 偏移严格递增、末尾换行多出一行、中文按字符而非字节计算行列
- `ui/find.rs`：匹配查找、大小写不敏感、循环跳转、空查询
- `mermaid.rs`：围栏切段（别的围栏里的 `mermaid` 不算数、没闭合的围栏、CRLF、
  切完拼回去必须和原文逐字节一致）
- `i18n.rs`：占位符替换、英文单复数选择，以及两种语言的文案表条目数必须一致

`tests/fixtures/linetest.md` 是手工验收用的样本：每一行的文字里都写着自己应该是第几行，
打开它（`cargo run -- tests/fixtures/linetest.md`）就能用肉眼确认行号槽和正文对得上。

## 依赖

| crate | 用途 |
| --- | --- |
| `eframe` / `egui` | 原生窗口与即时模式 UI（wgpu/Metal 渲染） |
| `egui_commonmark` + `pulldown-cmark` | Markdown / GFM 渲染 |
| `merman` | Mermaid 渲染（纯 Rust：解析 + 布局 + SVG + 光栅化） |
| `egui_extras` | syntect 语法高亮、图片加载 |
| `rfd` | 系统原生文件对话框 |
| `skrifa` | 校验系统字体是否可解析（epaint 解析失败会直接 panic） |

## 关于中文字体

egui 自带字体只有拉丁字形，中文会渲染成空白方块，所以启动时会挂一个系统中文字体做兜底。
候选按 macOS → Linux → Windows 的常见路径排列，第一个能被 `skrifa` 解析的胜出；
如果都没找到，程序照常运行（只是中文会显示成方块），不会崩。

macOS 26 上实测命中 `/System/Library/Fonts/Hiragino Sans GB.ttc`
（注意这台机器上**没有** `PingFang.ttc`，所以候选表里不能只写 PingFang）。

## 开发用截图

在没有"屏幕录制"权限的机器上也能验收渲染效果 —— egui 的截图走应用自己的帧缓冲，
不依赖系统截屏：

```bash
MARKVIEW_SCREENSHOT=/tmp/shot.png cargo run -- demo.md
```

程序会在界面稳定几帧后把窗口内容写成 PNG 然后自己退出。
图片是异步解码的，如果文档里挂着大图，用 `MARKVIEW_SCREENSHOT_FRAMES=120` 多等几帧再截。

## Windows 上的渲染后端

eframe 默认的 wgpu 后端在 Windows 上会把 **Vulkan** 也列进候选，而 wgpu 是按位顺序
枚举适配器的（Vulkan 排在 DX12 前面），于是很容易选中 Intel 核显的 Vulkan 驱动。
部分版本的 `igvk64.dll` 在建立 surface / swapchain 时会踩空指针，进程直接以
`0xc0000005 STATUS_ACCESS_VIOLATION` 退出 —— 崩在驱动 DLL 里，Rust 侧连 panic
都不会触发，日志里只有一个毫无信息量的退出码。

所以 `main.rs` 的 `pick_render_backend()` 在 Windows 上把后端固定成 **DX12**，
不参与这个抽奖。想手动指定后端时照旧可以覆盖：

```powershell
$env:WGPU_BACKEND="gl"; .\markview.exe      # 或者 dx12 / vulkan
```

排查这类崩溃可以先看 Windows 事件查看器里的 "应用程序错误"，
"出错模块名称" 直接指出是哪个 DLL 崩的（本例是 `igvk64.dll`）。

## 许可证

[MIT](LICENSE)。
