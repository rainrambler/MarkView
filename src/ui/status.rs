//! 底部状态栏：文件状态、光标位置、统计信息、右侧的视图/主题/缩放控件。

use egui::{Align, Layout, RichText, Ui};

use crate::app::{Action, App};
use crate::prefs::ViewMode;

pub fn show(app: &mut App, ui: &mut Ui, actions: &mut Vec<Action>) {
    let App {
        doc,
        prefs,
        stats,
        cursor_line,
        cursor_col,
        ..
    } = app;

    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // ---- 左半：文件与正文信息 ----
                let mark = if doc.dirty { "● " } else { "" };
                ui.label(
                    RichText::new(format!("{mark}{}", doc.display_name()))
                        .small()
                        .strong(),
                )
                .on_hover_text(doc.full_path());

                ui.separator();
                ui.label(
                    RichText::new(format!("{}:{}", *cursor_line + 1, *cursor_col + 1))
                        .small()
                        .monospace(),
                )
                .on_hover_text("行:列");

                ui.separator();
                ui.label(RichText::new(format!("{} 行", stats.lines)).small());
                ui.label(RichText::new(format!("{} 词", stats.words)).small());
                ui.label(RichText::new(format!("{} 字符", stats.chars)).small());

                ui.separator();
                ui.label(RichText::new(reading_time(stats.reading_secs)).small())
                    .on_hover_text("按中文 400 字/分钟、英文 220 词/分钟估算");

                ui.separator();
                ui.label(RichText::new(doc.line_ending.label()).small())
                    .on_hover_text("换行符风格，保存时会原样写回");

                if doc.had_bom {
                    ui.label(RichText::new("BOM").small())
                        .on_hover_text("文件带头部 UTF-8 BOM，保存时会补回");
                }
                if doc.lossy {
                    ui.label(
                        RichText::new("⚠ 非 UTF-8")
                            .small()
                            .color(ui.visuals().warn_fg_color),
                    )
                    .on_hover_text("原文件不是合法 UTF-8，已按有损方式解码；保存会覆盖原文");
                }

                // ---- 右半：控件（从右往左排） ----
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{}%", (prefs.zoom * 100.0).round() as i32)).small(),
                    );
                    if ui.small_button("＋").on_hover_text("放大（⌘+）").clicked() {
                        actions.push(Action::ZoomIn);
                    }
                    if ui.small_button("－").on_hover_text("缩小（⌘-）").clicked() {
                        actions.push(Action::ZoomOut);
                    }

                    ui.separator();
                    // 反向遍历，这样视觉上是从左到右 编辑/分栏/预览
                    for mode in ViewMode::ALL.iter().rev() {
                        let selected = *mode == prefs.view_mode;
                        let hint = match mode {
                            ViewMode::Editor => "只看编辑区（⌘1）",
                            ViewMode::Split => "左右分栏（⌘2）",
                            ViewMode::Preview => "只看预览（⌘3）",
                        };
                        if ui
                            .selectable_label(selected, RichText::new(mode.label()).small())
                            .on_hover_text(hint)
                            .clicked()
                        {
                            actions.push(Action::SetView(*mode));
                        }
                    }

                    ui.separator();
                    if ui
                        .small_button(RichText::new(prefs.theme.label()).small())
                        .on_hover_text("切换配色（⌘⇧T）")
                        .clicked()
                    {
                        actions.push(Action::CycleTheme);
                    }
                });
            });
        });
}

fn reading_time(secs: u32) -> String {
    match secs {
        0 => "约 0 分钟".to_owned(),
        s if s < 60 => "约 1 分钟".to_owned(),
        s => format!("约 {} 分钟", (s + 30) / 60),
    }
}
