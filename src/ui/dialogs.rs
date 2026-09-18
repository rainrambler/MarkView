//! 弹窗：关于、快捷键速查、以及"有未保存改动"的确认框。

use egui::{Context, Modal, RichText, Ui};

use crate::app::{Action, AfterDiscard, App, Dialog};

pub fn show(app: &mut App, ctx: &Context, actions: &mut Vec<Action>) {
    match app.dialog.clone() {
        Dialog::None => {}
        Dialog::About => {
            if about(app, ctx) {
                app.dialog = Dialog::None;
            }
        }
        Dialog::Shortcuts => {
            if shortcuts(ctx) {
                app.dialog = Dialog::None;
            }
        }
        Dialog::Discard { then } => match discard(app, ctx, &then) {
            Choice::None => {}
            Choice::Cancel => app.dialog = Dialog::None,
            // 先保存，保存成功后再继续原来那件事
            Choice::SaveAndProceed => actions.push(Action::SaveAndProceed(then)),
            Choice::Discard => {
                app.dialog = Dialog::None;
                actions.push(then.force_action());
            }
        },
    }
}

/// `Modal::show` 返回是否应该关闭（点背景、按 Esc、或点了关闭按钮）。
fn about(app: &mut App, ctx: &Context) -> bool {
    let response = Modal::new(egui::Id::new("mdviewer.dialog.about")).show(ctx, |ui| {
        ui.set_max_width(470.0);
        ui.heading("Markdown 查看器");
        ui.add_space(2.0);
        ui.label(
            RichText::new(format!("版本 {}", env!("CARGO_PKG_VERSION")))
                .small()
                .weak(),
        );
        ui.add_space(12.0);
        ui.label("用 Rust 写的原生 Markdown 编辑器 / 查看器：左边写源码，右边实时渲染。");

        ui.add_space(14.0);
        ui.label(RichText::new("技术栈").strong());
        for line in [
            "eframe / egui —— 原生窗口与即时模式界面",
            "egui_commonmark + pulldown-cmark —— GFM 渲染（表格、任务列表、脚注）",
            "syntect —— 源码与代码块的语法高亮",
            "rfd —— 系统原生文件对话框",
        ] {
            ui.label(RichText::new(format!("· {line}")).small());
        }

        if let Some(note) = app.font_note.as_deref() {
            ui.add_space(12.0);
            ui.label(RichText::new(note).small().weak());
        }

        ui.add_space(14.0);
        ui.label(RichText::new("按 Esc 或点击空白处关闭").small().weak());
    });

    response.should_close()
}

fn shortcuts(ctx: &Context) -> bool {
    const ENTRIES: &[(&str, &str)] = &[
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
    ];

    let response = Modal::new(egui::Id::new("mdviewer.dialog.shortcuts")).show(ctx, |ui| {
        ui.set_max_width(430.0);
        ui.heading("快捷键");
        ui.add_space(10.0);
        for (key, description) in ENTRIES {
            if description.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new(*key).strong());
                continue;
            }
            row(ui, key, description);
        }
    });

    response.should_close()
}

fn row(ui: &mut Ui, key: &str, description: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(key).monospace().small());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(description).small());
        });
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Choice {
    None,
    Cancel,
    SaveAndProceed,
    Discard,
}

fn discard(app: &mut App, ctx: &Context, then: &AfterDiscard) -> Choice {
    let mut choice = Choice::None;

    let response = Modal::new(egui::Id::new("mdviewer.dialog.discard")).show(ctx, |ui| {
        ui.set_max_width(430.0);
        ui.heading("还有未保存的修改");
        ui.add_space(8.0);
        ui.label(format!("「{}」有改动还没保存。", app.doc.display_name()));
        ui.add_space(4.0);
        ui.label(RichText::new(describe(then)).small().weak());
        ui.add_space(16.0);

        ui.horizontal(|ui| {
            if ui.button("保存后继续").clicked() {
                choice = Choice::SaveAndProceed;
            }
            if ui.button("放弃修改").clicked() {
                choice = Choice::Discard;
            }
            if ui.button("取消").clicked() {
                choice = Choice::Cancel;
            }
        });
    });

    if choice != Choice::None {
        return choice;
    }
    if response.should_close() {
        return Choice::Cancel;
    }
    Choice::None
}

fn describe(then: &AfterDiscard) -> &'static str {
    match then {
        AfterDiscard::NewFile => "继续会丢弃当前内容，新建一个空白文档。",
        AfterDiscard::OpenDialog => "继续会打开文件选择器，并丢弃当前内容。",
        AfterDiscard::OpenPath(_) => "继续会打开新文件，并丢弃当前内容。",
        AfterDiscard::Quit => "继续会退出程序，未保存的内容会丢失。",
    }
}
