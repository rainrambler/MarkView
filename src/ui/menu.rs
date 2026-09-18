//! 顶部菜单栏。所有条目都只是往 `actions` 里塞意图，真正的执行在 `app` 层。

use egui::containers::menu::MenuButton;
use egui::{MenuBar, Ui};

use crate::app::{Action, App};
use crate::prefs::ViewMode;

pub fn show(app: &mut App, ui: &mut Ui, actions: &mut Vec<Action>) {
    // 只借用这两个字段。若在这里留着 `app` 再用，借用检查器会认为重借用冲突。
    let App { prefs, doc, .. } = app;

    egui::Panel::top("mdviewer.menu.bar").show(ui, |ui| {
        MenuBar::new().ui(ui, |ui| {
            // ---------------------------------------------------------- 文件
            MenuButton::new("文件").ui(ui, |ui| {
                if item(ui, "新建", "⌘N") {
                    actions.push(Action::New);
                }
                if item(ui, "打开…", "⌘O") {
                    actions.push(Action::Open);
                }
                ui.separator();
                if item(ui, "保存", "⌘S") {
                    actions.push(Action::Save);
                }
                if item(ui, "另存为…", "⌘⇧S") {
                    actions.push(Action::SaveAs);
                }
                ui.separator();
                if item(ui, "导出为 HTML…", "⌘E") {
                    actions.push(Action::ExportHtml);
                }
                ui.separator();

                // 最近打开（列表为空时整项置灰）
                ui.add_enabled_ui(!prefs.recent.is_empty(), |ui| {
                    MenuButton::new("最近打开").ui(ui, |ui| {
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
                        if ui.button("清空列表").clicked() {
                            actions.push(Action::ClearRecent);
                        }
                    });
                });

                ui.separator();
                if item(ui, "退出", "⌘Q") {
                    actions.push(Action::Quit);
                }
            });

            // ---------------------------------------------------------- 编辑
            MenuButton::new("编辑").ui(ui, |ui| {
                if item(ui, "加粗", "⌘B") {
                    actions.push(Action::WrapSelection("**", "**"));
                }
                if item(ui, "斜体", "⌘I") {
                    actions.push(Action::WrapSelection("*", "*"));
                }
                if item(ui, "删除线", "⌘⇧X") {
                    actions.push(Action::WrapSelection("~~", "~~"));
                }
                if item(ui, "行内代码", "⌘⇧C") {
                    actions.push(Action::WrapSelection("`", "`"));
                }
                if item(ui, "链接", "⌘K") {
                    actions.push(Action::WrapSelection("[", "](url)"));
                }
                ui.separator();
                if item(ui, "无序列表", "⌘⇧U") {
                    actions.push(Action::ToggleLinePrefix("- "));
                }
                if item(ui, "引用", "⌘⇧P") {
                    actions.push(Action::ToggleLinePrefix("> "));
                }
                ui.separator();
                if item(ui, "一级标题", "⌘⌥1") {
                    actions.push(Action::SetHeading(1));
                }
                if item(ui, "二级标题", "⌘⌥2") {
                    actions.push(Action::SetHeading(2));
                }
                if item(ui, "三级标题", "⌘⌥3") {
                    actions.push(Action::SetHeading(3));
                }
                ui.separator();
                if item(ui, "查找…", "⌘F") {
                    actions.push(Action::OpenFind);
                }
                if item(ui, "下一个匹配", "⌘G") {
                    actions.push(Action::FindStep(1));
                }
                if item(ui, "上一个匹配", "⌘⇧G") {
                    actions.push(Action::FindStep(-1));
                }
            });

            // ---------------------------------------------------------- 视图
            MenuButton::new("视图").ui(ui, |ui| {
                for (mode, shortcut) in [
                    (ViewMode::Editor, "⌘1"),
                    (ViewMode::Split, "⌘2"),
                    (ViewMode::Preview, "⌘3"),
                ] {
                    let checked = prefs.view_mode == mode;
                    if ui
                        .add(
                            egui::Button::new(mode.label())
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
                        egui::Button::new("大纲")
                            .shortcut_text("⌘⇧O")
                            .selected(prefs.show_outline),
                    )
                    .clicked()
                {
                    actions.push(Action::ToggleOutline);
                }
                if ui
                    .add(
                        egui::Button::new("行号")
                            .shortcut_text("⌘⇧L")
                            .selected(prefs.show_line_numbers),
                    )
                    .clicked()
                {
                    actions.push(Action::ToggleLineNumbers);
                }
                if ui
                    .add(
                        egui::Button::new("自动换行")
                            .shortcut_text("")
                            .selected(prefs.editor_wrap),
                    )
                    .on_hover_text("关闭后长行改为横向滚动")
                    .clicked()
                {
                    actions.push(Action::ToggleWrap);
                }
                ui.separator();

                if ui
                    .add(
                        egui::Button::new("配色")
                            .shortcut_text("⌘⇧T")
                            .selected(false),
                    )
                    .on_hover_text(format!("当前：{}", prefs.theme.label()))
                    .clicked()
                {
                    actions.push(Action::CycleTheme);
                }
                ui.separator();

                if item(ui, "放大", "⌘+") {
                    actions.push(Action::ZoomIn);
                }
                if item(ui, "缩小", "⌘-") {
                    actions.push(Action::ZoomOut);
                }
                if item(ui, "实际大小", "⌘0") {
                    actions.push(Action::ZoomReset);
                }
            });

            // ---------------------------------------------------------- 帮助
            MenuButton::new("帮助").ui(ui, |ui| {
                if item(ui, "快捷键", "") {
                    actions.push(Action::Shortcuts);
                }
                if item(ui, "关于", "") {
                    actions.push(Action::About);
                }
            });

            // 右对齐的提示：当前文档有未保存改动
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if doc.dirty {
                    ui.label(
                        egui::RichText::new("● 未保存")
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
