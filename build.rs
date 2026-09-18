//! 构建脚本：把 `assets/AppIcon.ico` 嵌成 Windows 可执行文件里的图标资源。
//!
//! 为什么必须在这里做：资源管理器显示的是 PE 文件里的图标资源，不是窗口图标。
//! `src/main.rs` 里的 `ViewportBuilder::with_icon` 只管标题栏和任务栏，运行时设多少遍
//! 都不会改变 exe 自己的图标 —— 资源只能在链接期写进 PE，Rust 侧没有对应的运行时接口。
//!
//! 做法是 `assets/markview.rc` 这份资源脚本经 `rc.exe` 编成一个 `.lib`，再用
//! `cargo:rustc-link-arg-bins` 交给链接器（都由 embed-resource 代劳，包括满世界
//! 找 rc.exe：它可能在 Windows SDK 里，也可能只被 vswhere 知道）。
//!
//! 图标本身是画出来的：`cargo run --release --example make_icon`。

fn main() {
    // embed-resource 不产出 rerun-if-changed（它没法从 rc 脚本反推依赖了哪些文件），
    // 缺了这行 Cargo 就会保守地把整个 crate 目录当成本脚本的输入：改一行 Rust 代码
    // 也要重跑一遍资源编译。真正的输入只有这两个文件。
    println!("cargo:rerun-if-changed=assets/markview.rc");
    println!("cargo:rerun-if-changed=assets/AppIcon.ico");

    // 只有 Windows 目标需要图标资源，也只有它才谈得上 rc.exe。
    // 其余平台直接跳过，免得为一个纯 Windows 的东西去要求 windres / llvm-rc。
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    // 这里故意用 manifest_required 而不是 manifest_optional：找不到资源编译器时
    // 前者会让构建失败，后者**只会安静地跳过**。而"安静地跳过"的结果是图标没了、
    // 构建日志里却一个字都不提 —— 那种问题要等到有人盯着资源管理器看才发现。
    embed_resource::compile("assets/markview.rc", embed_resource::NONE)
        .manifest_required()
        .expect(
            "编译 Windows 图标资源失败：需要一个资源编译器。\n\
             MSVC 目标：装 Windows SDK（提供 rc.exe），或用 RC / RC_<target> 环境变量指定。\n\
             交叉编译到 Windows：装 llvm-rc 或 <target>-w64-mingw32-windres。",
        );
}
