//! 用户偏好设置。整个结构体会被 serde 序列化进 eframe 的 storage
//! （macOS 上是 `~/Library/Application Support/mdviewer/`），
//! 所以**新增字段必须给它默认值**，否则老配置文件会反序列化失败。

use std::path::{Path, PathBuf};

use egui::Context;
use serde::{Deserialize, Serialize};

/// 最近文件列表最多保留多少条。
const MAX_RECENT: usize = 12;

/// 窗口布局：分栏 / 只看编辑 / 只看预览。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewMode {
    Split,
    Editor,
    Preview,
}

impl ViewMode {
    pub const ALL: [Self; 3] = [Self::Split, Self::Editor, Self::Preview];

    pub fn label(self) -> &'static str {
        match self {
            Self::Split => "分栏",
            Self::Editor => "编辑",
            Self::Preview => "预览",
        }
    }
}

/// 配色主题。System 表示跟随系统深浅色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppTheme {
    System,
    Light,
    Dark,
}

impl AppTheme {
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "跟随系统",
            Self::Light => "浅色",
            Self::Dark => "深色",
        }
    }

    pub fn to_egui(self) -> egui::ThemePreference {
        match self {
            Self::System => egui::ThemePreference::System,
            Self::Light => egui::ThemePreference::Light,
            Self::Dark => egui::ThemePreference::Dark,
        }
    }

    /// 菜单里点一下就轮换到下一个。
    pub fn next(self) -> Self {
        match self {
            Self::System => Self::Light,
            Self::Light => Self::Dark,
            Self::Dark => Self::System,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub view_mode: ViewMode,
    pub theme: AppTheme,
    pub show_outline: bool,
    pub show_line_numbers: bool,
    /// 编辑器是否自动换行。关闭后长行改为横向滚动。
    pub editor_wrap: bool,
    /// 预览区是否显示行内 HTML 的原始文本（默认按纯文本渲染）。
    pub show_syntax_themes: bool,
    /// 界面缩放系数，1.0 为 100%。
    pub zoom: f32,
    /// 最近打开过的文件，最新的在前。
    pub recent: Vec<PathBuf>,
    /// 打开/保存对话框的起始目录。
    pub last_dir: Option<PathBuf>,
    /// 预览里代码块使用的 syntect 主题名。
    pub syntax_dark: String,
    pub syntax_light: String,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            view_mode: ViewMode::Split,
            theme: AppTheme::System,
            show_outline: true,
            show_line_numbers: true,
            editor_wrap: true,
            show_syntax_themes: false,
            zoom: 1.0,
            recent: Vec::new(),
            last_dir: None,
            syntax_dark: "base16-ocean.dark".to_owned(),
            syntax_light: "base16-ocean.light".to_owned(),
        }
    }
}

impl Preferences {
    /// 缩放系数的合法区间，避免用户把界面缩到看不见。
    pub const ZOOM_RANGE: std::ops::RangeInclusive<f32> = 0.5..=3.0;

    /// 把某个文件提到最近列表最前面（去重 + 截断）。
    pub fn touch_recent(&mut self, path: &Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(MAX_RECENT);

        // 注意：相对路径（比如 `mdviewer foo.md`）的 parent() 是空路径而不是 None，
        // 直接存进去会让下次的"打开"对话框落在奇怪的地方。
        if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
            self.last_dir = Some(dir.to_path_buf());
        }
    }

    /// 丢掉已经不存在的最近文件（外接盘拔掉、文件被删等）。
    pub fn prune_recent(&mut self) {
        self.recent.retain(|p| p.exists());
        if self
            .last_dir
            .as_ref()
            .is_some_and(|dir| dir.as_os_str().is_empty())
        {
            self.last_dir = None;
        }
    }

    /// 把持久化的缩放系数推给 egui。启动时调用一次即可。
    pub fn push_zoom_to(&self, ctx: &Context) {
        ctx.set_zoom_factor(
            self.zoom
                .clamp(*Self::ZOOM_RANGE.start(), *Self::ZOOM_RANGE.end()),
        );
    }
}

/// 作用在 egui 的缩放系数上。
///
/// 之所以不直接改 [`Preferences::zoom`]：egui 自己也会处理 ⌘+滚轮缩放，
/// 两边各推一次就会互相覆盖。让 egui 的 `zoom_factor` 当唯一事实来源，
/// 每帧再回读进偏好设置持久化，这样两种输入方式都自然生效。
pub fn apply_zoom(ctx: &Context, factor: f32) {
    let range = Preferences::ZOOM_RANGE;
    let next = (ctx.zoom_factor() * factor).clamp(*range.start(), *range.end());
    ctx.set_zoom_factor(next);
}
