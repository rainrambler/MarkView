//! 应用状态与主循环。
//!
//! 交互分两步走：UI 代码只往一个 `Vec<Action>` 里塞"用户想干什么"，
//! 等所有面板画完之后再统一执行。好处是菜单项和快捷键可以共用同一套处理逻辑，
//! 而且不会在画 UI 的时候和借用检查器打架。

use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::{Context, Key, KeyboardShortcut, Modifiers, ViewportCommand};
use egui_commonmark::CommonMarkCache;
use egui_extras::syntax_highlighting::CodeTheme;

use crate::document::Document;
use crate::fonts;
use crate::markdown::{self, Heading, Stats, byte_to_char};
use crate::mermaid;
use crate::prefs::{Preferences, ViewMode};
use crate::ui;
use crate::ui::find::FindState;

pub const WINDOW_TITLE: &str = "Markdown 查看器";

/// 偏好设置在 eframe storage 里的键。
const PREFS_KEY: &str = "mdviewer.preferences.v1";

/// 编辑区 TextEdit 的稳定 Id。查找跳转、加粗等操作都要靠它读写光标状态。
pub fn editor_id() -> egui::Id {
    egui::Id::new("mdviewer.editor.text")
}

/// 一个待跳转的位置，字符序号（不是字节）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reveal {
    pub char_start: usize,
    pub char_end: usize,
}

/// 被"未保存改动"挡住的那件事，用户确认放弃后继续执行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AfterDiscard {
    NewFile,
    OpenDialog,
    OpenPath(PathBuf),
    Quit,
}

impl AfterDiscard {
    /// 用户已经确认放弃修改后要执行的动作。
    pub(crate) fn force_action(&self) -> Action {
        match self {
            Self::NewFile => Action::ForceNew,
            Self::OpenDialog => Action::ForceOpen,
            Self::OpenPath(path) => Action::ForceOpenPath(path.clone()),
            Self::Quit => Action::ForceQuit,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Dialog {
    #[default]
    None,
    About,
    Shortcuts,
    Discard {
        then: AfterDiscard,
    },
}

#[derive(Debug, Clone)]
pub enum Action {
    New,
    Open,
    OpenPath(PathBuf),
    ClearRecent,
    Save,
    SaveAs,
    /// 先保存，成功则继续执行原本被拦下的那件事。
    SaveAndProceed(AfterDiscard),
    ExportHtml,
    Quit,

    // 「已确认放弃修改」之后强制执行，不再问第二遍
    ForceNew,
    ForceOpen,
    ForceOpenPath(PathBuf),
    ForceQuit,

    SetView(ViewMode),
    ToggleOutline,
    ToggleLineNumbers,
    ToggleWrap,
    CycleTheme,
    ZoomIn,
    ZoomOut,
    ZoomReset,

    OpenFind,
    CloseFind,
    FindStep(isize),

    About,
    Shortcuts,

    WrapSelection(&'static str, &'static str),
    ToggleLinePrefix(&'static str),
    SetHeading(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Error,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub kind: ToastKind,
    pub text: String,
    pub until: Instant,
}

/// 开发/验证用的截图钩子。
///
/// 用法：`MDVIEWER_SCREENSHOT=/tmp/a.png mdviewer demo.md`
/// 程序会在界面稳定几帧后把窗口内容写成 PNG 然后自己退出。
/// 存在的意义是在没有"屏幕录制"权限的机器上也能验收渲染效果
/// （egui 的截图走的是应用自己的帧缓冲，不依赖系统截屏）。
struct ScreenshotJob {
    path: PathBuf,
    /// 还要再等几帧，让 Markdown 缓存、图片加载都稳定下来。
    frames_left: u32,
    requested: bool,
}

impl Toast {
    fn new(kind: ToastKind, text: impl Into<String>, secs: u64) -> Self {
        Self {
            kind,
            text: text.into(),
            until: Instant::now() + Duration::from_secs(secs),
        }
    }

    fn info(text: impl Into<String>) -> Self {
        Self::new(ToastKind::Info, text, 4)
    }

    fn error(text: impl Into<String>) -> Self {
        Self::new(ToastKind::Error, text, 9)
    }
}

pub struct App {
    pub doc: Document,
    pub prefs: Preferences,
    pub cache: CommonMarkCache,
    /// Mermaid 位图缓存，见 [`crate::mermaid`]。
    pub mermaid: mermaid::MermaidCache,
    pub code_theme: CodeTheme,
    pub dialog: Dialog,
    pub find: FindState,
    pub headings: Vec<Heading>,
    pub stats: Stats,
    pub cursor_line: usize,
    pub cursor_col: usize,
    /// 正文版本号。变了才重算大纲/统计/查找结果，避免每帧扫全文。
    pub text_version: u64,
    derived_version: u64,
    pub toast: Option<Toast>,
    /// 下一帧把键盘焦点交给编辑器。
    pub focus_editor: bool,
    /// 下一帧把光标移到这个位置并滚动过去。
    pub reveal: Option<Reveal>,
    /// 用户已经在确认框里点过"放弃/保存"，别再拦一次。
    bypass_guard: bool,
    /// 缓存标题，只有变了才发给窗口系统。
    last_title: Option<String>,
    /// 启动时选中的中文字体，显示在"关于"里。
    pub font_note: Option<String>,
    /// 见 [`ScreenshotJob`]。
    screenshot: Option<ScreenshotJob>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let font_path = fonts::install(&cc.egui_ctx);

        let mut prefs = cc
            .storage
            .and_then(|storage| eframe::get_value::<Preferences>(storage, PREFS_KEY))
            .unwrap_or_default();
        prefs.prune_recent();
        prefs.zoom = prefs.zoom.clamp(
            *Preferences::ZOOM_RANGE.start(),
            *Preferences::ZOOM_RANGE.end(),
        );

        let mut app = Self {
            doc: Document::untitled(),
            prefs,
            cache: CommonMarkCache::default(),
            mermaid: mermaid::MermaidCache::default(),
            code_theme: CodeTheme::from_style(&cc.egui_ctx.style_of(cc.egui_ctx.theme())),
            dialog: Dialog::None,
            find: FindState::default(),
            headings: Vec::new(),
            stats: Stats::default(),
            cursor_line: 0,
            cursor_col: 0,
            text_version: 1,
            derived_version: 0,
            toast: None,
            focus_editor: true,
            reveal: None,
            bypass_guard: false,
            last_title: None,
            font_note: font_path.map(|path| format!("中文字体：{path}")),
            screenshot: std::env::var_os("MDVIEWER_SCREENSHOT").map(|path| ScreenshotJob {
                path: PathBuf::from(path),
                frames_left: 6,
                requested: false,
            }),
        };
        app.rebuild_derived();

        // 把持久化的缩放系数推给 egui（之后由 egui 的 zoom_factor 当唯一事实来源）
        app.prefs.push_zoom_to(&cc.egui_ctx);

        // 支持 `mdviewer 笔记.md`
        if let Some(arg) = std::env::args_os().nth(1) {
            let path = PathBuf::from(arg);
            if path.is_file() {
                app.load_path(path);
            } else {
                app.set_toast_error(format!("找不到文件：{}", path.display()));
            }
        }
        app
    }

    // ---------------------------------------------------------------- 状态

    /// 正文被改动了：置脏、版本号 +1（下游据此重算派生数据）。
    pub fn touch(&mut self) {
        self.doc.dirty = true;
        self.text_version = self.text_version.wrapping_add(1);
        self.find.mark_dirty();
    }

    pub fn set_toast_info(&mut self, message: impl Into<String>) {
        self.toast = Some(Toast::info(message));
    }

    pub fn set_toast_error(&mut self, message: impl Into<String>) {
        self.toast = Some(Toast::error(message));
    }

    /// 需要时把大纲 / 统计 / 查找结果算一遍。
    fn rebuild_derived(&mut self) {
        if self.text_version != self.derived_version {
            self.derived_version = self.text_version;
            self.headings = markdown::outline(&self.doc.text);
            self.stats = markdown::stats(&self.doc.text);
        }

        // 拆开借用：find 要可变、正文要不可变，直接写 self.find.rebuild(&self.doc.text)
        // 会让借用检查器看到两个指向 self 的借用，虽然字段其实是分开的
        let Self { find, doc, .. } = self;
        find.rebuild(&doc.text);
    }

    // ---------------------------------------------------------------- 文件

    fn load_path(&mut self, path: PathBuf) {
        // 转成绝对路径：命令行传 `demo.md` 时 cwd 一变，最近文件列表就失效了
        let path = path.canonicalize().unwrap_or(path);

        match Document::open(&path) {
            Ok(document) => {
                let lossy = document.lossy;
                self.doc = document;
                // 换了文档，预览缓存必须清掉，否则会显示上一个文件的渲染结果
                self.cache = CommonMarkCache::default();
                self.mermaid.clear();
                self.text_version = self.text_version.wrapping_add(1);
                self.reveal = Some(Reveal {
                    char_start: 0,
                    char_end: 0,
                });
                self.focus_editor = true;
                self.prefs.touch_recent(&path);

                if lossy {
                    self.set_toast_error(format!(
                        "{} 不是合法的 UTF-8，已按有损方式打开",
                        path.display()
                    ));
                } else {
                    self.set_toast_info(format!("已打开 {}", path.display()));
                }
            }
            Err(err) => self.set_toast_error(err),
        }
    }

    fn do_new(&mut self) {
        self.doc = Document::untitled();
        self.cache = CommonMarkCache::default();
        self.mermaid.clear();
        self.find.mark_dirty();
        self.text_version = self.text_version.wrapping_add(1);
        self.reveal = Some(Reveal {
            char_start: 0,
            char_end: 0,
        });
        self.focus_editor = true;
        self.set_toast_info("已新建空白文档");
    }

    fn do_open_dialog(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .set_title("打开 Markdown 文件")
            .add_filter("Markdown", &["md", "markdown", "mdx", "txt"])
            .add_filter("所有文件", &["*"]);
        if let Some(dir) = self.prefs.last_dir.clone() {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.pick_file() {
            self.load_path(path);
        }
    }

    pub fn save(&mut self, force_dialog: bool) {
        if force_dialog || self.doc.path.is_none() {
            let mut dialog = rfd::FileDialog::new()
                .set_title(if force_dialog { "另存为" } else { "保存" })
                .add_filter("Markdown", &["md", "markdown", "mdx", "txt"])
                .set_file_name(self.doc.display_name());
            if let Some(dir) = self.prefs.last_dir.clone() {
                dialog = dialog.set_directory(dir);
            }
            if let Some(path) = dialog.save_file() {
                let result = self.doc.save_as(&path);
                self.finish_write(result, path);
            }
        } else {
            let result = self.doc.save();
            let path = self.doc.path.clone().unwrap_or_default();
            self.finish_write(result, path);
        }
    }

    fn finish_write(&mut self, result: Result<(), String>, path: PathBuf) {
        match result {
            Ok(()) => {
                // 文件此时已经存在，canonicalize 能成功
                let path = path.canonicalize().unwrap_or(path);
                self.prefs.touch_recent(&path);
                self.set_toast_info(format!("已保存 {}", path.display()));
            }
            Err(err) => self.set_toast_error(err),
        }
    }

    fn export_html(&mut self) {
        let stem = self
            .doc
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "未命名".to_owned());

        let mut dialog = rfd::FileDialog::new()
            .set_title("导出为 HTML")
            .add_filter("HTML", &["html", "htm"])
            .set_file_name(format!("{stem}.html"));
        if let Some(dir) = self.prefs.last_dir.clone() {
            dialog = dialog.set_directory(dir);
        }
        let Some(path) = dialog.save_file() else {
            return;
        };

        let html = markdown::to_html(&self.doc.text, &stem);
        match std::fs::write(&path, html.as_bytes()) {
            Ok(()) => {
                if let Some(dir) = path.parent() {
                    self.prefs.last_dir = Some(dir.to_path_buf());
                }
                self.set_toast_info(format!("已导出 {}", path.display()));
            }
            Err(err) => self.set_toast_error(format!("导出失败：{err}")),
        }
    }

    /// 有未保存改动时先弹确认框；返回 true 表示可以立刻继续。
    fn guard(&mut self, then: AfterDiscard) -> bool {
        if !self.doc.dirty || self.bypass_guard {
            return true;
        }
        self.dialog = Dialog::Discard { then };
        false
    }

    // ---------------------------------------------------------------- 事件

    fn handle_actions(&mut self, ctx: &Context, mut actions: Vec<Action>) {
        // 边处理边追加：例如 SaveAndProceed 会往队尾塞一个 Force* 动作
        let mut index = 0usize;
        while index < actions.len() {
            let action = actions[index].clone();
            index += 1;

            match action {
                Action::New => {
                    if self.guard(AfterDiscard::NewFile) {
                        self.do_new();
                    }
                }
                Action::Open => {
                    if self.guard(AfterDiscard::OpenDialog) {
                        self.do_open_dialog();
                    }
                }
                Action::OpenPath(path) => {
                    if self.guard(AfterDiscard::OpenPath(path.clone())) {
                        self.load_path(path);
                    }
                }
                Action::ClearRecent => self.prefs.recent.clear(),
                Action::Save => self.save(false),
                Action::SaveAs => self.save(true),
                Action::SaveAndProceed(then) => {
                    self.save(false);
                    if !self.doc.dirty {
                        actions.push(then.force_action());
                    }
                }
                Action::ExportHtml => self.export_html(),
                Action::Quit => {
                    if self.guard(AfterDiscard::Quit) {
                        self.bypass_guard = true;
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                }
                Action::ForceNew => self.do_new(),
                Action::ForceOpen => self.do_open_dialog(),
                Action::ForceOpenPath(path) => self.load_path(path),
                Action::ForceQuit => {
                    self.bypass_guard = true;
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }

                Action::SetView(mode) => {
                    self.prefs.view_mode = mode;
                    if matches!(mode, ViewMode::Editor | ViewMode::Split) {
                        self.focus_editor = true;
                    }
                }
                Action::ToggleOutline => self.prefs.show_outline = !self.prefs.show_outline,
                Action::ToggleLineNumbers => {
                    self.prefs.show_line_numbers = !self.prefs.show_line_numbers;
                }
                Action::ToggleWrap => self.prefs.editor_wrap = !self.prefs.editor_wrap,
                Action::CycleTheme => self.prefs.theme = self.prefs.theme.next(),
                Action::ZoomIn => crate::prefs::apply_zoom(ctx, 1.1),
                Action::ZoomOut => crate::prefs::apply_zoom(ctx, 1.0 / 1.1),
                Action::ZoomReset => ctx.set_zoom_factor(1.0),

                Action::OpenFind => self.find.open(),
                Action::CloseFind => {
                    self.find.close();
                    self.focus_editor = true;
                }
                Action::FindStep(delta) => {
                    self.find.step(delta);
                    self.reveal_current_match(ctx);
                }

                Action::About => self.dialog = Dialog::About,
                Action::Shortcuts => self.dialog = Dialog::Shortcuts,

                Action::WrapSelection(prefix, suffix) => {
                    ui::editor::wrap_selection(ctx, self, prefix, suffix);
                }
                Action::ToggleLinePrefix(prefix) => {
                    ui::editor::toggle_line_prefix(ctx, self, prefix);
                }
                Action::SetHeading(level) => ui::editor::set_heading(ctx, self, level),
            }
        }
    }

    /// 把当前查找匹配换算成字符序号，交给编辑器去跳转。
    fn reveal_current_match(&mut self, ctx: &Context) {
        let Some(found) = self.find.current_match() else {
            return;
        };
        self.reveal = Some(Reveal {
            char_start: byte_to_char(&self.doc.text, found.start),
            char_end: byte_to_char(&self.doc.text, found.end),
        });
        self.focus_editor = true;
        // 这次改动发生在面板画完之后，得主动要一帧才看得见
        ctx.request_repaint();
    }

    fn handle_shortcuts(&mut self, ctx: &Context, actions: &mut Vec<Action>) {
        let mut pressed: Vec<Action> = Vec::new();
        ctx.input_mut(|input| {
            for (shortcut, action) in SHORTCUTS.iter() {
                if input.consume_shortcut(shortcut) {
                    pressed.push(action.clone());
                }
            }
        });

        // Esc 只在查找栏开着时被消费，免得抢走别的用途
        let escape = self.find.open
            && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape));

        actions.append(&mut pressed);
        if escape {
            actions.push(Action::CloseFind);
        }
    }

    fn poll_dropped_files(&mut self, ctx: &Context, actions: &mut Vec<Action>) {
        let dropped: Vec<PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_path_buf())
                .collect()
        });
        for path in dropped {
            if path.is_file() {
                actions.push(Action::OpenPath(path));
            }
        }
    }

    fn handle_close_request(&mut self, ctx: &Context) {
        if !ctx.input(|input| input.viewport().close_requested()) {
            return;
        }
        if self.doc.dirty && !self.bypass_guard {
            // 先拦下这次关闭，问清楚再说
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.dialog = Dialog::Discard {
                then: AfterDiscard::Quit,
            };
        }
    }

    /// 缩放以 egui 的 `zoom_factor` 为准（⌘+滚轮也由 egui 处理），
    /// 这里只负责每帧回读进偏好设置以便持久化。
    fn sync_zoom(&mut self, ctx: &Context) {
        self.prefs.zoom = ctx.zoom_factor().clamp(
            *Preferences::ZOOM_RANGE.start(),
            *Preferences::ZOOM_RANGE.end(),
        );
    }

    fn sync_code_theme(&mut self, ctx: &Context) {
        // 0.36 里 `Context::style()` 没了，要指明是哪个主题的样式
        let style = ctx.style_of(ctx.theme());
        if self.code_theme.is_dark() != style.visuals.dark_mode {
            self.code_theme = CodeTheme::from_style(&style);
        }
    }

    fn update_window_title(&mut self, ctx: &Context) {
        let title = format!(
            "{}{} · {WINDOW_TITLE}",
            if self.doc.dirty { "● " } else { "" },
            self.doc.display_name()
        );
        if self.last_title.as_deref() != Some(title.as_str()) {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.last_title = Some(title);
        }
    }

    // ---------------------------------------------------------------- 提示

    fn draw_toast(&mut self, ctx: &Context) {
        let Some(toast) = self.toast.as_ref() else {
            return;
        };
        let remaining = toast
            .until
            .saturating_duration_since(Instant::now())
            .as_secs_f32();
        if remaining <= 0.0 {
            self.toast = None;
            return;
        }
        // 最后 0.4 秒淡出
        let alpha = (remaining / 0.4).min(1.0);
        ctx.request_repaint_after(Duration::from_millis(80));

        let visuals = ctx.style_of(ctx.theme()).visuals.clone();
        let fg = match toast.kind {
            ToastKind::Info => visuals.text_color(),
            ToastKind::Error => visuals.error_fg_color,
        };

        egui::Area::new(egui::Id::new("mdviewer.toast"))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -46.0))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(visuals.window_fill.gamma_multiply(alpha))
                    .stroke(visuals.window_stroke)
                    .corner_radius(egui::CornerRadius::same(8))
                    .inner_margin(egui::Margin::symmetric(14, 9))
                    .shadow(visuals.window_shadow)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(&toast.text).color(fg.gamma_multiply(alpha)));
                    });
            });
    }

    /// 见 [`ScreenshotJob`]。每帧推进一点点：等若干帧 -> 请求截图 -> 收到图后存盘退出。
    fn tick_screenshot(&mut self, ctx: &Context) {
        if self.screenshot.is_none() {
            return;
        }

        // 截图结果通过 input 事件在下一帧回来
        let captured = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });

        if let Some(image) = captured {
            if let Some(job) = self.screenshot.as_ref() {
                let path = job.path.clone();
                let (width, height) = (image.width() as u32, image.height() as u32);
                let bytes: Vec<u8> = image
                    .pixels
                    .iter()
                    .flat_map(|pixel| pixel.to_array())
                    .collect();

                match image::RgbaImage::from_raw(width, height, bytes) {
                    Some(buffer) => match buffer.save(&path) {
                        Ok(()) => eprintln!("[mdviewer] 截图已保存：{}", path.display()),
                        Err(err) => eprintln!("[mdviewer] 截图保存失败：{err}"),
                    },
                    None => eprintln!("[mdviewer] 截图尺寸与像素数不匹配"),
                }
            }
            self.screenshot = None;
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }

        let job = self.screenshot.as_mut().expect("上面已确认 Some");
        if job.frames_left > 0 {
            job.frames_left -= 1;
            ctx.request_repaint();
        } else if !job.requested {
            job.requested = true;
            ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::UserData::default()));
            ctx.request_repaint();
        }
    }
}

impl eframe::App for App {
    // egui 0.36 起 `App` 的入口是 `ui`（而不是老版本的 `update(ctx, frame)`）：
    // 框架直接把根 `Ui` 交给我们，面板都基于它来切分。
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Context 内部是 Arc，clone 一下最省事，免得和 `ui` 的可变借用打架
        let ctx = ui.ctx().clone();
        let mut actions: Vec<Action> = Vec::new();

        ui::install_theme(&ctx, &self.prefs);
        self.sync_zoom(&ctx);
        self.sync_code_theme(&ctx);
        self.poll_dropped_files(&ctx, &mut actions);
        self.handle_shortcuts(&ctx, &mut actions);
        self.handle_close_request(&ctx);
        self.rebuild_derived();
        self.update_window_title(&ctx);

        // 面板顺序决定嵌套关系：菜单栏和状态栏横跨整窗，
        // 大纲和编辑区依次从左侧切走空间，剩下的给预览。
        ui::menu::show(self, ui, &mut actions);

        egui::Panel::bottom("mdviewer.status.bar")
            .show(ui, |ui| ui::status::show(self, ui, &mut actions));

        let show_outline =
            self.prefs.show_outline && !matches!(self.prefs.view_mode, ViewMode::Preview);
        if show_outline {
            egui::Panel::left("mdviewer.outline.panel")
                .resizable(true)
                .default_size(230.0)
                .min_size(160.0)
                .max_size(420.0)
                .show(ui, |ui| ui::outline::show(self, ui, &mut actions));
        }

        match self.prefs.view_mode {
            ViewMode::Editor => {
                egui::CentralPanel::default_margins()
                    .show(ui, |ui| ui::editor::show(self, ui, &mut actions));
            }
            ViewMode::Preview => {
                egui::CentralPanel::default_margins()
                    .show(ui, |ui| ui::preview::show(self, ui, &mut actions));
            }
            ViewMode::Split => {
                egui::Panel::left("mdviewer.editor.panel")
                    .resizable(true)
                    .default_size(620.0)
                    .min_size(260.0)
                    .max_size(1400.0)
                    .show(ui, |ui| ui::editor::show(self, ui, &mut actions));
                egui::CentralPanel::default_margins()
                    .show(ui, |ui| ui::preview::show(self, ui, &mut actions));
            }
        }

        self.handle_actions(&ctx, actions);

        // 弹窗可能产生新动作（保存后继续、放弃修改后继续…），再走一遍
        let mut dialog_actions: Vec<Action> = Vec::new();
        ui::dialogs::show(self, &ctx, &mut dialog_actions);
        if !dialog_actions.is_empty() {
            self.handle_actions(&ctx, dialog_actions);
        }

        self.draw_toast(&ctx);
        self.tick_screenshot(&ctx);
    }

    /// 退出时把偏好设置写进 eframe 的持久化存储。
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, PREFS_KEY, &self.prefs);
    }
}

/// 快捷键表。用 `Command` 而不是 Ctrl，在 macOS 上就是 ⌘。
static SHORTCUTS: &[(KeyboardShortcut, Action)] = &[
    // 文件
    (
        KeyboardShortcut::new(Modifiers::COMMAND, Key::N),
        Action::New,
    ),
    (
        KeyboardShortcut::new(Modifiers::COMMAND, Key::O),
        Action::Open,
    ),
    (
        KeyboardShortcut::new(Modifiers::COMMAND, Key::S),
        Action::Save,
    ),
    (
        KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::S),
        Action::SaveAs,
    ),
    (
        KeyboardShortcut::new(Modifiers::COMMAND, Key::E),
        Action::ExportHtml,
    ),
    (
        KeyboardShortcut::new(Modifiers::COMMAND, Key::Q),
        Action::Quit,
    ),
    // 视图
    (
        KeyboardShortcut::new(Modifiers::COMMAND, Key::Num1),
        Action::SetView(ViewMode::Editor),
    ),
    (
        KeyboardShortcut::new(Modifiers::COMMAND, Key::Num2),
        Action::SetView(ViewMode::Split),
    ),
    (
        KeyboardShortcut::new(Modifiers::COMMAND, Key::Num3),
        Action::SetView(ViewMode::Preview),
    ),
    (
        KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::O),
        Action::ToggleOutline,
    ),
    (
        KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::L),
        Action::ToggleLineNumbers,
    ),
    (
        KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::T),
        Action::CycleTheme,
    ),
    (
        KeyboardShortcut::new(Modifiers::COMMAND, Key::Plus),
        Action::ZoomIn,
    ),
    (
        KeyboardShortcut::new(Modifiers::COMMAND, Key::Equals),
        Action::ZoomIn,
    ),
    (
        KeyboardShortcut::new(Modifiers::COMMAND, Key::Minus),
        Action::ZoomOut,
    ),
    (
        KeyboardShortcut::new(Modifiers::COMMAND, Key::Num0),
        Action::ZoomReset,
    ),
    // 查找
    (
        KeyboardShortcut::new(Modifiers::COMMAND, Key::F),
        Action::OpenFind,
    ),
    (
        KeyboardShortcut::new(Modifiers::COMMAND, Key::G),
        Action::FindStep(1),
    ),
    (
        KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::G),
        Action::FindStep(-1),
    ),
    // Markdown 格式化
    (
        KeyboardShortcut::new(Modifiers::COMMAND, Key::B),
        Action::WrapSelection("**", "**"),
    ),
    (
        KeyboardShortcut::new(Modifiers::COMMAND, Key::I),
        Action::WrapSelection("*", "*"),
    ),
    (
        KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::X),
        Action::WrapSelection("~~", "~~"),
    ),
    (
        KeyboardShortcut::new(Modifiers::COMMAND, Key::K),
        Action::WrapSelection("[", "](url)"),
    ),
    (
        KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::C),
        Action::WrapSelection("`", "`"),
    ),
    (
        KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::U),
        Action::ToggleLinePrefix("- "),
    ),
    (
        KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::P),
        Action::ToggleLinePrefix("> "),
    ),
    (
        KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::ALT), Key::Num1),
        Action::SetHeading(1),
    ),
    (
        KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::ALT), Key::Num2),
        Action::SetHeading(2),
    ),
    (
        KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::ALT), Key::Num3),
        Action::SetHeading(3),
    ),
];
