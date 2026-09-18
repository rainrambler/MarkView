//! UI string table / 界面文案表。
//!
//! Every user-visible string in the app lives here, once per language, in
//! [`Strings`]. Adding a language means adding one `static` and one match arm in
//! [`Language::strings`] — the compiler then forces every field to be filled in.
//!
//! 所有会出现在界面上的文字都集中在 [`Strings`] 里。要加一门语言，只需新增一个
//! `static` 并在 [`Language::strings`] 里加一个分支，剩下的交给编译器。
//!
//! Placeholders use `{name}` and are filled by [`fill`]. Templates with a `{n}`
//! should go through [`Strings::lines`] / [`Strings::words`] / [`Strings::chars`]
//! so English can pick the singular form.
//!
//! Note: log lines written to stderr (`eprintln!`) are deliberately **not**
//! translated — they are developer-facing and always English.

use serde::{Deserialize, Serialize};

/// 界面语言。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Language {
    #[default]
    En,
    Zh,
}

impl Language {
    pub const ALL: [Self; 2] = [Self::En, Self::Zh];

    /// 语言名用各自的母语写，用户才能一眼认出自己那一项。
    pub fn label(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::Zh => "简体中文",
        }
    }

    /// 这门语言的文案表。
    pub fn strings(self) -> &'static Strings {
        match self {
            Self::En => &EN,
            Self::Zh => &ZH,
        }
    }
}

/// 把模板里的 `{name}` 占位符替换掉。未出现的占位符会被忽略。
pub fn fill(template: &str, args: &[(&str, &str)]) -> String {
    let mut out = template.to_owned();
    for (key, value) in args {
        out = out.replace(&format!("{{{key}}}"), value);
    }
    out
}

/// 一整套界面文案。字段顺序尽量和它在界面上出现的顺序一致。
pub struct Strings {
    // ---------------------------------------------------------------- 通用
    /// 产品名。两种语言下都一样，但放在这里就不用满仓库找字面量了。
    pub app_name: &'static str,

    // ---------------------------------------------------------------- 菜单
    pub menu_file: &'static str,
    pub menu_edit: &'static str,
    pub menu_view: &'static str,
    pub menu_help: &'static str,
    pub menu_language: &'static str,

    pub item_new: &'static str,
    pub item_open: &'static str,
    pub item_save: &'static str,
    pub item_save_as: &'static str,
    pub item_export_html: &'static str,
    pub item_recent: &'static str,
    pub item_clear_recent: &'static str,
    pub item_quit: &'static str,

    pub item_bold: &'static str,
    pub item_italic: &'static str,
    pub item_strikethrough: &'static str,
    pub item_inline_code: &'static str,
    pub item_link: &'static str,
    pub item_bullet_list: &'static str,
    pub item_blockquote: &'static str,
    pub item_heading1: &'static str,
    pub item_heading2: &'static str,
    pub item_heading3: &'static str,
    pub item_find: &'static str,
    pub item_next_match: &'static str,
    pub item_prev_match: &'static str,

    pub item_outline: &'static str,
    pub item_line_numbers: &'static str,
    pub item_wrap: &'static str,
    pub item_wrap_hint: &'static str,
    pub item_theme: &'static str,
    /// `{theme}` = current theme label.
    pub item_theme_current: &'static str,
    pub item_zoom_in: &'static str,
    pub item_zoom_out: &'static str,
    pub item_zoom_reset: &'static str,
    pub item_shortcuts: &'static str,
    pub item_about: &'static str,

    /// 菜单栏右上角的未保存标记。
    pub unsaved_badge: &'static str,

    // ---------------------------------------------------------------- 布局与配色
    pub view_split: &'static str,
    pub view_editor: &'static str,
    pub view_preview: &'static str,
    pub theme_system: &'static str,
    pub theme_light: &'static str,
    pub theme_dark: &'static str,

    // ---------------------------------------------------------------- 状态栏
    /// 光标位置的悬停提示，例如 "Line:Col"。
    pub status_line_col: &'static str,
    pub status_lines_one: &'static str,
    pub status_lines_many: &'static str,
    pub status_words_one: &'static str,
    pub status_words_many: &'static str,
    pub status_chars_one: &'static str,
    pub status_chars_many: &'static str,
    /// `{n}` = 分钟数。
    pub status_reading_time: &'static str,
    pub status_reading_hint: &'static str,
    pub status_eol_hint: &'static str,
    pub status_bom_hint: &'static str,
    pub status_lossy_badge: &'static str,
    pub status_lossy_hint: &'static str,
    pub hint_zoom_in: &'static str,
    pub hint_zoom_out: &'static str,
    pub hint_view_editor: &'static str,
    pub hint_view_split: &'static str,
    pub hint_view_preview: &'static str,
    pub hint_cycle_theme: &'static str,

    // ---------------------------------------------------------------- 编辑区
    pub find_label: &'static str,
    pub find_hint: &'static str,
    pub find_case_hint: &'static str,
    pub find_no_match: &'static str,
    pub find_prev: &'static str,
    pub find_next: &'static str,
    pub find_close: &'static str,
    pub find_footer: &'static str,

    // ---------------------------------------------------------------- 大纲
    pub outline_title: &'static str,
    pub outline_hide_hint: &'static str,
    pub outline_empty: &'static str,
    pub outline_empty_hint: &'static str,
    pub outline_untitled: &'static str,
    /// `{line}` = 1-based line number, `{text}` = full heading text.
    pub outline_tooltip: &'static str,

    // ---------------------------------------------------------------- 弹窗
    /// `{version}` = crate version.
    pub about_version: &'static str,
    pub about_tagline: &'static str,
    pub about_stack_title: &'static str,
    pub about_stack: &'static [&'static str],
    pub about_hint: &'static str,

    pub shortcuts_title: &'static str,
    /// (键, 说明)。说明为空的行是分组标题。
    pub shortcuts: &'static [(&'static str, &'static str)],

    pub discard_title: &'static str,
    /// `{name}` = 文件名。
    pub discard_body: &'static str,
    pub discard_save_continue: &'static str,
    pub discard_discard: &'static str,
    pub discard_cancel: &'static str,
    pub discard_new_file: &'static str,
    pub discard_open_dialog: &'static str,
    pub discard_open_path: &'static str,
    pub discard_quit: &'static str,

    // ---------------------------------------------------------------- 文件对话框
    pub dialog_open_title: &'static str,
    pub dialog_save_title: &'static str,
    pub dialog_save_as_title: &'static str,
    pub dialog_export_title: &'static str,
    pub filter_markdown: &'static str,
    pub filter_html: &'static str,
    pub filter_all_files: &'static str,
    /// 未命名文档导出时用的文件名主干。
    pub untitled_stem: &'static str,
    /// 未命名文档的占位文件名。
    pub untitled_file_name: &'static str,
    /// 还没有路径的文档，在悬停提示里显示什么。
    pub no_path_placeholder: &'static str,

    // ---------------------------------------------------------------- 提示条 / 错误
    /// `{path}`
    pub toast_opened: &'static str,
    /// `{path}`
    pub toast_file_not_found: &'static str,
    /// `{path}`
    pub toast_lossy_open: &'static str,
    pub toast_new_doc: &'static str,
    /// `{path}`
    pub toast_saved: &'static str,
    /// `{path}`
    pub toast_exported: &'static str,
    /// `{err}`
    pub toast_export_failed: &'static str,
    /// `{path}`
    pub font_note: &'static str,

    /// `{path}` `{err}`
    pub error_read: &'static str,
    /// `{path}` `{err}`
    pub error_write: &'static str,
    pub error_no_path: &'static str,

    /// `{err}`
    pub mermaid_prefix: &'static str,
    pub mermaid_not_a_diagram: &'static str,
    /// `{err}`
    pub mermaid_decode_failed: &'static str,

    // ---------------------------------------------------------------- 导出 HTML
    /// 写进 `<html lang="...">`。
    pub html_lang: &'static str,
}

impl Strings {
    pub fn lines(&self, n: usize) -> String {
        self.plural(n, self.status_lines_one, self.status_lines_many)
    }

    pub fn words(&self, n: usize) -> String {
        self.plural(n, self.status_words_one, self.status_words_many)
    }

    pub fn chars(&self, n: usize) -> String {
        self.plural(n, self.status_chars_one, self.status_chars_many)
    }

    /// 阅读耗时。`secs` 是秒。
    pub fn reading_time(&self, secs: u32) -> String {
        let minutes = match secs {
            0 => 0,
            s if s < 60 => 1,
            // 30 秒向上取整，1 分 40 秒显示成 2 分钟
            s => (s + 30) / 60,
        };
        fill(self.status_reading_time, &[("n", &minutes.to_string())])
    }

    fn plural(&self, n: usize, one: &'static str, many: &'static str) -> String {
        let template = if n == 1 { one } else { many };
        fill(template, &[("n", &n.to_string())])
    }
}

static EN: Strings = Strings {
    app_name: "MarkView",

    menu_file: "File",
    menu_edit: "Edit",
    menu_view: "View",
    menu_help: "Help",
    menu_language: "Language",

    item_new: "New",
    item_open: "Open…",
    item_save: "Save",
    item_save_as: "Save As…",
    item_export_html: "Export as HTML…",
    item_recent: "Open Recent",
    item_clear_recent: "Clear list",
    item_quit: "Quit",

    item_bold: "Bold",
    item_italic: "Italic",
    item_strikethrough: "Strikethrough",
    item_inline_code: "Inline code",
    item_link: "Link",
    item_bullet_list: "Bullet list",
    item_blockquote: "Blockquote",
    item_heading1: "Heading 1",
    item_heading2: "Heading 2",
    item_heading3: "Heading 3",
    item_find: "Find…",
    item_next_match: "Next match",
    item_prev_match: "Previous match",

    item_outline: "Outline",
    item_line_numbers: "Line numbers",
    item_wrap: "Wrap lines",
    item_wrap_hint: "Turn off to scroll long lines horizontally",
    item_theme: "Theme",
    item_theme_current: "Current: {theme}",
    item_zoom_in: "Zoom in",
    item_zoom_out: "Zoom out",
    item_zoom_reset: "Actual size",
    item_shortcuts: "Keyboard shortcuts",
    item_about: "About",

    unsaved_badge: "● Unsaved",

    view_split: "Split",
    view_editor: "Editor",
    view_preview: "Preview",
    theme_system: "System",
    theme_light: "Light",
    theme_dark: "Dark",

    status_line_col: "Line:Column",
    status_lines_one: "{n} line",
    status_lines_many: "{n} lines",
    status_words_one: "{n} word",
    status_words_many: "{n} words",
    status_chars_one: "{n} character",
    status_chars_many: "{n} characters",
    status_reading_time: "{n} min read",
    status_reading_hint: "Estimated at 400 CJK characters/min and 220 words/min",
    status_eol_hint: "Line ending style — written back unchanged when saving",
    status_bom_hint: "File starts with a UTF-8 BOM; it is restored when saving",
    status_lossy_badge: "⚠ Not UTF-8",
    status_lossy_hint:
        "The file is not valid UTF-8 and was decoded lossily; saving will overwrite the original",
    hint_zoom_in: "Zoom in (⌘+)",
    hint_zoom_out: "Zoom out (⌘-)",
    hint_view_editor: "Editor only (⌘1)",
    hint_view_split: "Split view (⌘2)",
    hint_view_preview: "Preview only (⌘3)",
    hint_cycle_theme: "Cycle theme (⌘⇧T)",

    find_label: "Find",
    find_hint: "Search…",
    find_case_hint: "Match case",
    find_no_match: "No matches",
    find_prev: "Previous",
    find_next: "Next",
    find_close: "Close",
    find_footer: "⌘G next · Esc to close",

    outline_title: "Outline",
    outline_hide_hint: "Hide the outline (⌘⇧O)",
    outline_empty: "No headings yet",
    outline_empty_hint: "Start a line with # ## or ###",
    outline_untitled: "(empty heading)",
    outline_tooltip: "Line {line}\n{text}",

    about_version: "Version {version}",
    about_tagline: "A native Markdown editor / viewer written in Rust: source on the left, live preview on the right.",
    about_stack_title: "Built with",
    about_stack: &[
        "eframe / egui — native window and immediate-mode UI",
        "egui_commonmark + pulldown-cmark — GFM rendering (tables, task lists, footnotes)",
        "syntect — syntax highlighting for the editor and code blocks",
        "merman — Mermaid diagrams, rendered in pure Rust",
        "rfd — native file dialogs",
    ],
    about_hint: "Press Esc or click outside to close",

    shortcuts_title: "Keyboard shortcuts",
    shortcuts: &[
        ("File", ""),
        ("⌘N / ⌘O", "New / Open"),
        ("⌘S / ⌘⇧S", "Save / Save As"),
        ("⌘E", "Export as HTML"),
        ("View", ""),
        ("⌘1 / ⌘2 / ⌘3", "Editor only / Split / Preview only"),
        ("⌘⇧O / ⌘⇧L", "Outline / Line numbers"),
        ("⌘⇧T", "Cycle theme"),
        ("⌘+ / ⌘- / ⌘0", "Zoom in / out / reset"),
        ("Edit", ""),
        ("⌘Z / ⌘⇧Z", "Undo / Redo"),
        ("⌘B / ⌘I / ⌘⇧X", "Bold / Italic / Strikethrough"),
        ("⌘K / ⌘⇧C", "Link / Inline code"),
        ("⌘⇧U / ⌘⇧P", "Bullet list / Blockquote"),
        ("⌘⌥1 / ⌘⌥2 / ⌘⌥3", "Heading 1 / 2 / 3"),
        ("Find", ""),
        ("⌘F", "Open the find bar"),
        ("⌘G / ⌘⇧G", "Next / previous match"),
        ("Esc", "Close the find bar or a dialog"),
    ],

    discard_title: "Unsaved changes",
    discard_body: "“{name}” has changes that have not been saved.",
    discard_save_continue: "Save and continue",
    discard_discard: "Discard changes",
    discard_cancel: "Cancel",
    discard_new_file: "Continuing discards the current content and starts a blank document.",
    discard_open_dialog: "Continuing opens the file picker and discards the current content.",
    discard_open_path: "Continuing opens another file and discards the current content.",
    discard_quit: "Continuing quits the app; unsaved content will be lost.",

    dialog_open_title: "Open a Markdown file",
    dialog_save_title: "Save",
    dialog_save_as_title: "Save As",
    dialog_export_title: "Export as HTML",
    filter_markdown: "Markdown",
    filter_html: "HTML",
    filter_all_files: "All files",
    untitled_stem: "Untitled",
    untitled_file_name: "Untitled.md",
    no_path_placeholder: "(not saved yet)",

    toast_opened: "Opened {path}",
    toast_file_not_found: "File not found: {path}",
    toast_lossy_open: "{path} is not valid UTF-8; opened with replacement characters",
    toast_new_doc: "New blank document",
    toast_saved: "Saved {path}",
    toast_exported: "Exported {path}",
    toast_export_failed: "Export failed: {err}",
    font_note: "CJK font: {path}",

    error_read: "Could not read {path}: {err}",
    error_write: "Could not write {path}: {err}",
    error_no_path: "This document has no path yet — use Save As first.",

    mermaid_prefix: "Mermaid: {err}",
    mermaid_not_a_diagram: "this source does not look like a Mermaid diagram",
    mermaid_decode_failed: "PNG decode failed: {err}",

    html_lang: "en",
};

static ZH: Strings = Strings {
    app_name: "MarkView",

    menu_file: "文件",
    menu_edit: "编辑",
    menu_view: "视图",
    menu_help: "帮助",
    menu_language: "语言",

    item_new: "新建",
    item_open: "打开…",
    item_save: "保存",
    item_save_as: "另存为…",
    item_export_html: "导出为 HTML…",
    item_recent: "最近打开",
    item_clear_recent: "清空列表",
    item_quit: "退出",

    item_bold: "加粗",
    item_italic: "斜体",
    item_strikethrough: "删除线",
    item_inline_code: "行内代码",
    item_link: "链接",
    item_bullet_list: "无序列表",
    item_blockquote: "引用",
    item_heading1: "一级标题",
    item_heading2: "二级标题",
    item_heading3: "三级标题",
    item_find: "查找…",
    item_next_match: "下一个匹配",
    item_prev_match: "上一个匹配",

    item_outline: "大纲",
    item_line_numbers: "行号",
    item_wrap: "自动换行",
    item_wrap_hint: "关闭后长行改为横向滚动",
    item_theme: "配色",
    item_theme_current: "当前：{theme}",
    item_zoom_in: "放大",
    item_zoom_out: "缩小",
    item_zoom_reset: "实际大小",
    item_shortcuts: "快捷键",
    item_about: "关于",

    unsaved_badge: "● 未保存",

    view_split: "分栏",
    view_editor: "编辑",
    view_preview: "预览",
    theme_system: "跟随系统",
    theme_light: "浅色",
    theme_dark: "深色",

    status_line_col: "行:列",
    status_lines_one: "{n} 行",
    status_lines_many: "{n} 行",
    status_words_one: "{n} 词",
    status_words_many: "{n} 词",
    status_chars_one: "{n} 字符",
    status_chars_many: "{n} 字符",
    status_reading_time: "约 {n} 分钟",
    status_reading_hint: "按中文 400 字/分钟、英文 220 词/分钟估算",
    status_eol_hint: "换行符风格，保存时会原样写回",
    status_bom_hint: "文件带头部 UTF-8 BOM，保存时会补回",
    status_lossy_badge: "⚠ 非 UTF-8",
    status_lossy_hint: "原文件不是合法 UTF-8，已按有损方式解码；保存会覆盖原文",
    hint_zoom_in: "放大（⌘+）",
    hint_zoom_out: "缩小（⌘-）",
    hint_view_editor: "只看编辑区（⌘1）",
    hint_view_split: "左右分栏（⌘2）",
    hint_view_preview: "只看预览（⌘3）",
    hint_cycle_theme: "切换配色（⌘⇧T）",

    find_label: "查找",
    find_hint: "关键字…",
    find_case_hint: "区分大小写",
    find_no_match: "无匹配",
    find_prev: "上一个",
    find_next: "下一个",
    find_close: "关闭",
    find_footer: "⌘G 下一个 · Esc 关闭",

    outline_title: "大纲",
    outline_hide_hint: "隐藏大纲（⌘⇧O）",
    outline_empty: "还没有标题",
    outline_empty_hint: "用 # ## ### 起一行就有了",
    outline_untitled: "（空标题）",
    outline_tooltip: "第 {line} 行\n{text}",

    about_version: "版本 {version}",
    about_tagline: "用 Rust 写的原生 Markdown 编辑器 / 查看器：左边写源码，右边实时渲染。",
    about_stack_title: "技术栈",
    about_stack: &[
        "eframe / egui —— 原生窗口与即时模式界面",
        "egui_commonmark + pulldown-cmark —— GFM 渲染（表格、任务列表、脚注）",
        "syntect —— 源码与代码块的语法高亮",
        "merman —— Mermaid 图表，纯 Rust 渲染",
        "rfd —— 系统原生文件对话框",
    ],
    about_hint: "按 Esc 或点击空白处关闭",

    shortcuts_title: "快捷键",
    shortcuts: &[
        ("文件", ""),
        ("⌘N / ⌘O", "新建 / 打开"),
        ("⌘S / ⌘⇧S", "保存 / 另存为"),
        ("⌘E", "导出为 HTML"),
        ("视图", ""),
        ("⌘1 / ⌘2 / ⌘3", "只看编辑 / 左右分栏 / 只看预览"),
        ("⌘⇧O / ⌘⇧L", "大纲 / 行号"),
        ("⌘⇧T", "切换配色"),
        ("⌘+ / ⌘- / ⌘0", "放大 / 缩小 / 重置"),
        ("编辑", ""),
        ("⌘Z / ⌘⇧Z", "撤销 / 重做"),
        ("⌘B / ⌘I / ⌘⇧X", "加粗 / 斜体 / 删除线"),
        ("⌘K / ⌘⇧C", "链接 / 行内代码"),
        ("⌘⇧U / ⌘⇧P", "无序列表 / 引用"),
        ("⌘⌥1 / ⌘⌥2 / ⌘⌥3", "一级 / 二级 / 三级标题"),
        ("查找", ""),
        ("⌘F", "打开查找栏"),
        ("⌘G / ⌘⇧G", "下一个 / 上一个匹配"),
        ("Esc", "关闭查找栏或弹窗"),
    ],

    discard_title: "还有未保存的修改",
    discard_body: "「{name}」有改动还没保存。",
    discard_save_continue: "保存后继续",
    discard_discard: "放弃修改",
    discard_cancel: "取消",
    discard_new_file: "继续会丢弃当前内容，新建一个空白文档。",
    discard_open_dialog: "继续会打开文件选择器，并丢弃当前内容。",
    discard_open_path: "继续会打开新文件，并丢弃当前内容。",
    discard_quit: "继续会退出程序，未保存的内容会丢失。",

    dialog_open_title: "打开 Markdown 文件",
    dialog_save_title: "保存",
    dialog_save_as_title: "另存为",
    dialog_export_title: "导出为 HTML",
    filter_markdown: "Markdown",
    filter_html: "HTML",
    filter_all_files: "所有文件",
    untitled_stem: "未命名",
    untitled_file_name: "未命名.md",
    no_path_placeholder: "（尚未保存）",

    toast_opened: "已打开 {path}",
    toast_file_not_found: "找不到文件：{path}",
    toast_lossy_open: "{path} 不是合法的 UTF-8，已按有损方式打开",
    toast_new_doc: "已新建空白文档",
    toast_saved: "已保存 {path}",
    toast_exported: "已导出 {path}",
    toast_export_failed: "导出失败：{err}",
    font_note: "中文字体：{path}",

    error_read: "无法读取 {path}：{err}",
    error_write: "无法写入 {path}：{err}",
    error_no_path: "这个文档还没有路径，请先用「另存为」",

    mermaid_prefix: "Mermaid：{err}",
    mermaid_not_a_diagram: "这段源码不像 Mermaid 图",
    mermaid_decode_failed: "PNG 解码失败：{err}",

    html_lang: "zh",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_replaces_every_placeholder() {
        let out = fill("Opened {path} ({path})", &[("path", "a.md")]);
        assert_eq!(out, "Opened a.md (a.md)");
    }

    #[test]
    fn fill_leaves_unknown_placeholders_alone() {
        // 传漏了参数不该 panic，也不该静默变成空串 —— 留在原地更容易被发现
        assert_eq!(fill("{a} and {b}", &[("a", "1")]), "1 and {b}");
    }

    #[test]
    fn english_picks_the_singular_form() {
        let s = Language::En.strings();
        assert_eq!(s.lines(1), "1 line");
        assert_eq!(s.lines(0), "0 lines");
        assert_eq!(s.lines(2), "2 lines");
        assert_eq!(s.words(1), "1 word");
        assert_eq!(s.chars(1), "1 character");
    }

    #[test]
    fn chinese_does_not_inflect() {
        let s = Language::Zh.strings();
        assert_eq!(s.lines(1), "1 行");
        assert_eq!(s.lines(7), "7 行");
        assert_eq!(s.words(7), "7 词");
    }

    #[test]
    fn reading_time_rounds_seconds_into_minutes() {
        let s = Language::En.strings();
        assert_eq!(s.reading_time(0), "0 min read");
        assert_eq!(s.reading_time(1), "1 min read");
        assert_eq!(s.reading_time(59), "1 min read");
        // 1 分 40 秒 -> 向上取整到 2 分钟
        assert_eq!(s.reading_time(100), "2 min read");
    }

    #[test]
    fn languages_stay_in_lockstep() {
        // 快捷键表和"关于"里的条目按语言各写一份，很容易只改一边。
        let en = Language::En.strings();
        let zh = Language::Zh.strings();
        assert_eq!(en.shortcuts.len(), zh.shortcuts.len());
        assert_eq!(en.about_stack.len(), zh.about_stack.len());
        assert_eq!(en.html_lang, "en");
        assert_eq!(zh.html_lang, "zh");
    }
}
