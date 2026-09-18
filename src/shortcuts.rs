//! 快捷键的按键定义与显示标签。
//!
//! 按键常量只写一次：[`keys`] 既给 `app::SHORTCUTS` 用来匹配按键，也给界面用来
//! 生成标签 —— 两边共用同一个 [`KeyboardShortcut`]，所以不会再出现
//! 「标签写着 ⌘S、实际要按 Ctrl+S」这种错位。
//!
//! 标签一律交给 [`egui::Context::format_shortcut`]：macOS 上是 `⌘S` / `⇧⌘T`，
//! Windows / Linux 上是 `Ctrl+S` / `Ctrl+Shift+T`。
//!
//! 文案表（[`crate::i18n`]）里不写死按键，只写 `{save}` 这样的占位符，
//! 画界面时由 [`fill_keys`] 换成当前平台的标签。

use egui::{Context, Key, KeyboardShortcut, Modifiers};

/// 会出现在界面上的按键绑定。
///
/// 一律用 `Modifiers::COMMAND` 而不是 `Modifiers::CTRL`：egui 会把它翻译成
/// macOS 的 ⌘ / 其它平台的 Ctrl。
pub mod keys {
    use super::{Key, KeyboardShortcut, Modifiers};

    // -------------------------------------------------------------- 文件
    pub const NEW: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::N);
    pub const OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);
    pub const SAVE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
    pub const SAVE_AS: KeyboardShortcut =
        KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::S);
    pub const EXPORT_HTML: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::E);
    pub const QUIT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Q);

    // -------------------------------------------------------------- 视图
    pub const VIEW_EDITOR: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Num1);
    pub const VIEW_SPLIT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Num2);
    pub const VIEW_PREVIEW: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Num3);
    pub const TOGGLE_OUTLINE: KeyboardShortcut =
        KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::O);
    pub const TOGGLE_LINE_NUMBERS: KeyboardShortcut =
        KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::L);
    pub const CYCLE_THEME: KeyboardShortcut =
        KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::T);
    pub const ZOOM_IN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Plus);
    /// 主键盘上的 `+` 得按 Shift+`=`，所以 `=` 也当放大用。
    /// 只是个别名，界面上不显示它。
    pub const ZOOM_IN_EQUALS: KeyboardShortcut =
        KeyboardShortcut::new(Modifiers::COMMAND, Key::Equals);
    pub const ZOOM_OUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Minus);
    pub const ZOOM_RESET: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Num0);

    // -------------------------------------------------------------- 查找
    pub const FIND: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::F);
    pub const FIND_NEXT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::G);
    pub const FIND_PREV: KeyboardShortcut =
        KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::G);

    // ---------------------------------------------------- Markdown 格式化
    pub const BOLD: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::B);
    pub const ITALIC: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::I);
    pub const STRIKETHROUGH: KeyboardShortcut =
        KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::X);
    pub const LINK: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::K);
    pub const INLINE_CODE: KeyboardShortcut =
        KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::C);
    pub const BULLET_LIST: KeyboardShortcut =
        KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::U);
    pub const BLOCKQUOTE: KeyboardShortcut =
        KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::P);
    pub const HEADING1: KeyboardShortcut =
        KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::ALT), Key::Num1);
    pub const HEADING2: KeyboardShortcut =
        KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::ALT), Key::Num2);
    pub const HEADING3: KeyboardShortcut =
        KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::ALT), Key::Num3);

    /// 撤销 / 重做由 `TextEdit` 自己处理，我们不去消费这个按键，
    /// 但菜单和速查表要显示它的标签，所以照样定义一份。
    pub const UNDO: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Z);
    pub const REDO: KeyboardShortcut =
        KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::Z);
}

/// 文案表里可以用的占位符：`{save}` 会变成当前平台的 `⌘S` / `Ctrl+S`。
///
/// 只列需要显示标签的按键；[`keys`] 里像 `ZOOM_IN_EQUALS` 这样的别名不在其中。
/// 名字和 [`keys`] 里的常量名一一对应（`{save_as}` ↔ `SAVE_AS`）。
static PLACEHOLDERS: &[(&str, KeyboardShortcut)] = &[
    // 文件
    ("new", keys::NEW),
    ("open", keys::OPEN),
    ("save", keys::SAVE),
    ("save_as", keys::SAVE_AS),
    ("export_html", keys::EXPORT_HTML),
    // 视图
    ("view_editor", keys::VIEW_EDITOR),
    ("view_split", keys::VIEW_SPLIT),
    ("view_preview", keys::VIEW_PREVIEW),
    ("toggle_outline", keys::TOGGLE_OUTLINE),
    ("toggle_line_numbers", keys::TOGGLE_LINE_NUMBERS),
    ("cycle_theme", keys::CYCLE_THEME),
    ("zoom_in", keys::ZOOM_IN),
    ("zoom_out", keys::ZOOM_OUT),
    ("zoom_reset", keys::ZOOM_RESET),
    // 查找
    ("find", keys::FIND),
    ("find_next", keys::FIND_NEXT),
    ("find_prev", keys::FIND_PREV),
    // Markdown 格式化
    ("bold", keys::BOLD),
    ("italic", keys::ITALIC),
    ("strikethrough", keys::STRIKETHROUGH),
    ("link", keys::LINK),
    ("inline_code", keys::INLINE_CODE),
    ("bullet_list", keys::BULLET_LIST),
    ("blockquote", keys::BLOCKQUOTE),
    ("heading1", keys::HEADING1),
    ("heading2", keys::HEADING2),
    ("heading3", keys::HEADING3),
    // TextEdit 自带，我们只负责显示标签
    ("undo", keys::UNDO),
    ("redo", keys::REDO),
];

/// 一个快捷键在当前平台上的标签，比如 `⌘S` 或 `Ctrl+S`。
pub fn label(ctx: &Context, keys: KeyboardShortcut) -> String {
    ctx.format_shortcut(&keys)
}

/// 把模板里的 `{save}` / `{zoom_in}` 之类换成当前平台的标签。
///
/// 认不出的占位符原样留下（和 [`crate::i18n::fill`] 一个脾气），写错名字时
/// 界面上会直接露出 `{sav}` 而不是静默变成空串。
pub fn fill_keys(ctx: &Context, template: &str) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(start) = rest.find('{') {
        let Some(offset) = rest[start..].find('}') else {
            break; // 没有配对的 `}`，剩下的按原样输出
        };
        let end = start + offset;
        out.push_str(&rest[..start]);

        let name = &rest[start + 1..end];
        match PLACEHOLDERS.iter().find(|(known, _)| *known == name) {
            Some((_, keys)) => out.push_str(&label(ctx, *keys)),
            None => out.push_str(&rest[start..=end]),
        }
        rest = &rest[end + 1..];
    }

    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Language;
    use egui::os::OperatingSystem;

    /// macOS 的按键符号。
    const MAC_SYMBOLS: [char; 4] = ['⌘', '⌥', '⇧', '⌃'];

    /// 所有带按键占位符的模板（两种语言各来一份）。
    fn templates(language: Language) -> Vec<&'static str> {
        let s = language.strings();
        let mut out = vec![
            s.hint_zoom_in,
            s.hint_zoom_out,
            s.hint_view_editor,
            s.hint_view_split,
            s.hint_view_preview,
            s.hint_cycle_theme,
            s.find_footer,
            s.outline_hide_hint,
        ];
        out.extend(s.shortcuts.iter().map(|(keys, _)| *keys));
        out
    }

    /// 在无头 Context 里跑一帧再取值。
    ///
    /// 必须跑在帧里：macOS 上 `format_shortcut` 要查字体才知道画不画得出 ⌘。
    fn with_ctx<R>(os: OperatingSystem, f: impl FnOnce(&Context) -> R) -> R {
        let ctx = Context::default();
        ctx.set_os(os);

        let mut result = None;
        let mut once = Some(f);
        let mut output = ctx.run_ui(Default::default(), |ui| {
            if let Some(f) = once.take() {
                result = Some(f(ui.ctx()));
            }
        });
        // epaint 要求纹理增量被处理掉，否则 Drop 时会 panic（同 ui/editor.rs 的测试）
        output.textures_delta.clear();

        result.expect("run_ui 应当跑完闭包")
    }

    /// 这就是当初的 bug：Windows 上的标签写着 ⌘，实际要按 Ctrl。
    #[test]
    fn windows_labels_use_ctrl() {
        with_ctx(OperatingSystem::Windows, |ctx| {
            assert_eq!(label(ctx, keys::SAVE), "Ctrl+S");
            assert_eq!(label(ctx, keys::SAVE_AS), "Ctrl+Shift+S");
            assert_eq!(label(ctx, keys::VIEW_EDITOR), "Ctrl+1");
            assert_eq!(label(ctx, keys::HEADING1), "Ctrl+Alt+1");
            assert_eq!(label(ctx, keys::FIND_NEXT), "Ctrl+G");
            assert!(!label(ctx, keys::FIND_PREV).contains(MAC_SYMBOLS));

            for (name, keys) in PLACEHOLDERS {
                let text = label(ctx, *keys);
                assert!(
                    !text.contains(MAC_SYMBOLS) && !text.contains("Cmd"),
                    "Windows 上 {name} 显示成了 {text}"
                );
                assert!(text.contains("Ctrl"), "{name} 少了 Ctrl：{text}");
            }
        });
    }

    /// 反过来，macOS 上不能出现 Ctrl（字体画不出符号时 egui 会退化成 `Cmd`，那也算对）。
    #[test]
    fn mac_labels_use_the_command_key() {
        with_ctx(OperatingSystem::Mac, |ctx| {
            for (name, keys) in PLACEHOLDERS {
                let text = label(ctx, *keys);
                assert!(
                    text.contains('⌘') || text.contains("Cmd"),
                    "macOS 上 {name} 显示成了 {text}"
                );
                assert!(!text.contains("Ctrl"), "macOS 上 {name} 显示成了 {text}");
            }
        });
    }

    /// 文案表里的占位符必须都填得上：写错一个名字，界面上就会露出字面量。
    #[test]
    fn tables_fill_every_placeholder() {
        with_ctx(OperatingSystem::Windows, |ctx| {
            for language in Language::ALL {
                for template in templates(language) {
                    let filled = fill_keys(ctx, template);
                    assert!(
                        !filled.contains('{'),
                        "{language:?} 的 `{template}` 留下了没填的占位符：{filled}"
                    );
                    assert!(
                        !filled.contains(MAC_SYMBOLS),
                        "Windows 上不该出现 mac 的按键符号：{filled}"
                    );
                }
            }
        });
    }

    /// 反向检查：不许留没人用的占位符（多半是把名字打错了）。
    #[test]
    fn no_unused_placeholder_names() {
        let all = Language::ALL
            .iter()
            .flat_map(|language| templates(*language))
            .collect::<Vec<_>>()
            .join("\n");

        for (name, _) in PLACEHOLDERS {
            assert!(
                all.contains(&format!("{{{name}}}")),
                "占位符 {name} 没有任何模板在用"
            );
        }
    }

    #[test]
    fn placeholder_names_are_unique() {
        let mut names: Vec<&str> = PLACEHOLDERS.iter().map(|(name, _)| *name).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "占位符名字重复了");
    }

    #[test]
    fn unknown_placeholders_are_left_alone() {
        with_ctx(OperatingSystem::Windows, |ctx| {
            assert_eq!(fill_keys(ctx, "Zoom in ({zoom_in})"), "Zoom in (Ctrl+Plus)");
            assert_eq!(fill_keys(ctx, "{nope}"), "{nope}");
            assert_eq!(fill_keys(ctx, "no placeholders"), "no placeholders");
        });
    }
}
