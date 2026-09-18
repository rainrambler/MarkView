//! 原生 Markdown 编辑器 / 查看器。
//!
//! 用法：
//!     markview              打开空文档
//!     markview 笔记.md       直接打开指定文件
//!
//! 打包成 macOS 应用（Dock 里才会有图标）：`packaging/build-app.sh`

mod app;
mod document;
mod fonts;
mod i18n;
mod markdown;
mod mermaid;
mod prefs;
mod ui;

use app::{APP_NAME, App};

fn main() -> Result<(), eframe::Error> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(APP_NAME)
        .with_inner_size([1280.0, 840.0])
        .with_min_inner_size([760.0, 480.0])
        // 允许把 .md 文件直接拖进窗口打开
        .with_drag_and_drop(true);

    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }

    let mut options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    pick_render_backend(&mut options);

    eframe::run_native(
        APP_NAME,
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

/// 挑一个靠谱的渲染后端。
///
/// eframe 0.36 的默认特性只带 `wgpu`，而 egui-wgpu 默认把后端设成
/// `Backends::PRIMARY | Backends::GL`，在 Windows 上就是 **Vulkan + DX12 + GL**。
/// wgpu 枚举适配器时按位顺序来（Vulkan 在最前），于是常常选中 Intel 核显的
/// Vulkan 适配器 —— 而 `igvk64.dll` 在建立 surface / swapchain 时会踩空指针，
/// 进程直接以 `0xc0000005 STATUS_ACCESS_VIOLATION` 死掉。
///
/// 这种崩溃发生在 C 层的图形驱动里，**Rust 侧连 panic 钩子都不会触发**，
/// 所以没有任何办法在运行时兜住，只能从一开始就别把 Vulkan 列进候选。
/// 这里在 Windows 上只用 DX12（Win10+ 的显卡驱动都提供），避开这个驱动雷区。
///
/// 用户显式设了 `WGPU_BACKEND` 时不做干预 —— 排查问题时仍然可以手动指定后端，
/// 比如 `$env:WGPU_BACKEND="gl"` 或 `"vulkan"`（想复现这个崩溃时就用得着）。
fn pick_render_backend(options: &mut eframe::NativeOptions) {
    #[cfg(target_os = "windows")]
    {
        use eframe::egui_wgpu::WgpuSetup;
        use eframe::wgpu::Backends;

        // 用户自己对后端有主张时不覆盖
        if Backends::from_env().is_some() {
            return;
        }

        if let WgpuSetup::CreateNew(setup) = &mut options.wgpu_options.wgpu_setup {
            // 注意不要顺手把 Backends::GL 也加回来：GL 的位比 DX12 靠前，
            // 会被优先枚举到，又变成"选中哪个看运气"。
            setup.instance_descriptor.backends = Backends::DX12;
        }
    }

    #[cfg(not(target_os = "windows"))]
    let _ = options;
}

/// 窗口图标，从 assets/ 里编译期内嵌。
///
/// **macOS 上这个不起作用** —— 系统忽略窗口级别的图标设置，Dock 和 Finder 用的是
/// `.app` bundle 里 `CFBundleIconFile` 指的 `.icns`。所以这里主要是给 Windows /
/// Linux 的窗口管理器用的；macOS 想要图标得走 `packaging/build-app.sh`。
///
/// 图标本身由 `cargo run --release --example make_icon` 用代码画出来。
fn window_icon() -> Option<egui::IconData> {
    const PNG: &[u8] = include_bytes!("../assets/icon_256.png");

    let image = image::load_from_memory(PNG).ok()?.into_rgba8();
    let (width, height) = image.dimensions();

    // 注意：`IconData.rgba` 的文档说是"非预乘 alpha"，但 egui 把它转成 ColorImage 时
    // 调用的是 `from_rgba_premultiplied`。两边差一个预乘，半透明边缘（圆角处）会发暗，
    // 所以这里自己先乘一遍，按实际消费方要的格式给。
    let rgba = image
        .pixels()
        .flat_map(|pixel| {
            let alpha = u32::from(pixel[3]);
            [
                (u32::from(pixel[0]) * alpha / 255) as u8,
                (u32::from(pixel[1]) * alpha / 255) as u8,
                (u32::from(pixel[2]) * alpha / 255) as u8,
                pixel[3],
            ]
        })
        .collect();

    Some(egui::IconData {
        rgba,
        width,
        height,
    })
}
