//! 顶部菜单栏。所有条目都只是往 `actions` 里塞意图，真正的执行在 `app` 层。

use egui::containers::menu::MenuButton;
use egui::{MenuBar, Ui};

use crate::app::{Action, App};
use crate::i18n::{Language, fill};
use crate::prefs::ViewMode;

pub fn show(app: &mut App, ui: &mut Ui, actions: &mut Vec<Action>) {
    // 只借用这几个字段。若在这里留着 `app` 再用，借用检查器会认为重借用冲突。
    let App { prefs, doc, .. } = app;
    let s = prefs.language.strings();

    egui::Panel::top("markview.menu.bar").show(ui, |ui| {
        MenuBar::new().ui(ui, |ui| {
            // ---------------------------------------------------------- 文件
            MenuButton::new(s.menu_file).ui(ui, |ui| {
                if item(ui, s.item_new, "⌘N") {
                    actions.push(Action::New);
                }
                if item(ui, s.item_open, "⌘O") {
                    actions.push(Action::Open);
                }
                ui.separator();
                if item(ui, s.item_save, "⌘S") {
                    actions.push(Action::Save);
                }
                if item(ui, s.item_save_as, "⌘⇧S") {
                    actions.push(Action::SaveAs);
                }
                ui.separator();
                if item(ui, s.item_export_html, "⌘E") {
                    actions.push(Action::ExportHtml);
                }
                ui.separator();

                // 最近打开（列表为空时整项置灰）
                ui.add_enabled_ui(!prefs.recent.is_empty(), |ui| {
                    MenuButton::new(s.item_recent).ui(ui, |ui| {
                        // clone 一份，免得边遍历边给 actions 塞东西时借用冲突
                        for path in prefs.recent.clone() {
                            let label = path
                                .file_name()
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_else(|| path.display().to_string());
                            if ui
                                .button(label)
                                .on_hover_text(path.display().to_string())
                                .clicked()
                            {
                                actions.push(Action::OpenPath(path));
                            }
                        }
                        ui.separator();
                        if ui.button(s.item_clear_recent).clicked() {
                            actions.push(Action::ClearRecent);
                        }
                    });
                });

                ui.separator();
                if item(ui, s.item_quit, "⌘Q") {
                    actions.push(Action::Quit);
                }
            });

            // ---------------------------------------------------------- 编辑
            MenuButton::new(s.menu_edit).ui(ui, |ui| {
                if item(ui, s.item_bold, "⌘B") {
                    actions.push(Action::WrapSelection("**", "**"));
                }
                if item(ui, s.item_italic, "⌘I") {
                    actions.push(Action::WrapSelection("*", "*"));
                }
                if item(ui, s.item_strikethrough, "⌘⇧X") {
                    actions.push(Action::WrapSelection("~~", "~~"));
                }
                if item(ui, s.item_inline_code, "⌘⇧C") {
                    actions.push(Action::WrapSelection("`", "`"));
                }
                if item(ui, s.item_link, "⌘K") {
                    actions.push(Action::WrapSelection("[", "](url)"));
                }
                ui.separator();
                if item(ui, s.item_bullet_list, "⌘⇧U") {
                    actions.push(Action::ToggleLinePrefix("- "));
                }
                if item(ui, s.item_blockquote, "⌘⇧P") {
                    actions.push(Action::ToggleLinePrefix("> "));
                }
                ui.separator();
                if item(ui, s.item_heading1, "⌘⌥1") {
                    actions.push(Action::SetHeading(1));
                }
                if item(ui, s.item_heading2, "⌘⌥2") {
                    actions.push(Action::SetHeading(2));
                }
                if item(ui, s.item_heading3, "⌘⌥3") {
                    actions.push(Action::SetHeading(3));
                }
                ui.separator();
                if item(ui, s.item_find, "⌘F") {
                    actions.push(Action::OpenFind);
                }
                if item(ui, s.item_next_match, "⌘G") {
                    actions.push(Action::FindStep(1));
                }
                if item(ui, s.item_prev_match, "⌘⇧G") {
                    actions.push(Action::FindStep(-1));
                }
            });

            // ---------------------------------------------------------- 视图
            MenuButton::new(s.menu_view).ui(ui, |ui| {
                for (mode, shortcut) in [
                    (ViewMode::Editor, "⌘1"),
                    (ViewMode::Split, "⌘2"),
                    (ViewMode::Preview, "⌘3"),
                ] {
                    let checked = prefs.view_mode == mode;
                    if ui
                        .add(
                            egui::Button::new(mode.label(s))
                                .shortcut_text(shortcut)
                                .selected(checked),
                        )
                        .clicked()
                    {
                        actions.push(Action::SetView(mode));
                    }
                }
                ui.separator();

                if ui
                    .add(
                        egui::Button::new(s.item_outline)
                            .shortcut_text("⌘⇧O")
                            .selected(prefs.show_outline),
                    )
                    .clicked()
                {
                    actions.push(Action::ToggleOutline);
                }
                if ui
                    .add(
                        egui::Button::new(s.item_line_numbers)
                            .shortcut_text("⌘⇧L")
                            .selected(prefs.show_line_numbers),
                    )
                    .clicked()
                {
                    actions.push(Action::ToggleLineNumbers);
                }
                if ui
                    .add(
                        egui::Button::new(s.item_wrap)
                            .shortcut_text("")
                            .selected(prefs.editor_wrap),
                    )
                    .on_hover_text(s.item_wrap_hint)
                    .clicked()
                {
                    actions.push(Action::ToggleWrap);
                }
                ui.separator();

                if ui
                    .add(
                        egui::Button::new(s.item_theme)
                            .shortcut_text("⌘⇧T")
                            .selected(false),
                    )
                    .on_hover_text(fill(s.item_theme_current, &[("theme", prefs.theme.label(s))]))
                    .clicked()
                {
                    actions.push(Action::CycleTheme);
                }
                ui.separator();

                if item(ui, s.item_zoom_in, "⌘+") {
                    actions.push(Action::ZoomIn);
                }
                if item(ui, s.item_zoom_out, "⌘-") {
                    actions.push(Action::ZoomOut);
                }
                if item(ui, s.item_zoom_reset, "⌘0") {
                    actions.push(Action::ZoomReset);
                }
            });

            // ---------------------------------------------------------- 语言
            MenuButton::new(s.menu_language).ui(ui, |ui| {
                for language in Language::ALL {
                    // 语言名各自用母语写（English / 简体中文），所以这里不套用当前语言的标签
                    let selected = prefs.language == language;
                    if ui
                        .add(egui::Button::new(language.label()).selected(selected))
                        .clicked()
                    {
                        actions.push(Action::SetLanguage(language));
                    }
                }
            });

            // ---------------------------------------------------------- 帮助
            MenuButton::new(s.menu_help).ui(ui, |ui| {
                if item(ui, s.item_shortcuts, "") {
                    actions.push(Action::Shortcuts);
                }
                if item(ui, s.item_about, "") {
                    actions.push(Action::About);
                }
            });

            // 右对齐的提示：当前文档有未保存改动
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if doc.dirty {
                    ui.label(
                        egui::RichText::new(s.unsaved_badge)
                            .small()
                            .color(ui.visuals().warn_fg_color),
                    );
                }
            });
        });
    });
}

/// 一个菜单项，带右对齐的快捷键提示。返回是否被点击。
fn item(ui: &mut egui::Ui, label: &str, shortcut: &str) -> bool {
    ui.add(egui::Button::new(label).shortcut_text(shortcut))
        .clicked()
}
