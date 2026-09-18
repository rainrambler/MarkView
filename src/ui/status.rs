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
    let s = prefs.language.strings();

    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // ---- 左半：文件与正文信息 ----
                let mark = if doc.dirty { "● " } else { "" };
                ui.label(
                    RichText::new(format!("{mark}{}", doc.display_name(s)))
                        .small()
                        .strong(),
                )
                .on_hover_text(doc.full_path(s));

                ui.separator();
                ui.label(
                    RichText::new(format!("{}:{}", *cursor_line + 1, *cursor_col + 1))
                        .small()
                        .monospace(),
                )
                .on_hover_text(s.status_line_col);

                ui.separator();
                ui.label(RichText::new(s.lines(stats.lines)).small());
                ui.label(RichText::new(s.words(stats.words)).small());
                ui.label(RichText::new(s.chars(stats.chars)).small());

                ui.separator();
                ui.label(RichText::new(s.reading_time(stats.reading_secs)).small())
                    .on_hover_text(s.status_reading_hint);

                ui.separator();
                ui.label(RichText::new(doc.line_ending.label()).small())
                    .on_hover_text(s.status_eol_hint);

                if doc.had_bom {
                    ui.label(RichText::new("BOM").small())
                        .on_hover_text(s.status_bom_hint);
                }
                if doc.lossy {
                    ui.label(
                        RichText::new(s.status_lossy_badge)
                            .small()
                            .color(ui.visuals().warn_fg_color),
                    )
                    .on_hover_text(s.status_lossy_hint);
                }

                // ---- 右半：控件（从右往左排） ----
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{}%", (prefs.zoom * 100.0).round() as i32)).small(),
                    );
                    if ui.small_button("＋").on_hover_text(s.hint_zoom_in).clicked() {
                        actions.push(Action::ZoomIn);
                    }
                    if ui.small_button("－").on_hover_text(s.hint_zoom_out).clicked() {
                        actions.push(Action::ZoomOut);
                    }

                    ui.separator();
                    // 反向遍历，这样视觉上是从左到右 编辑/分栏/预览
                    for mode in ViewMode::ALL.iter().rev() {
                        let selected = *mode == prefs.view_mode;
                        let hint = match mode {
                            ViewMode::Editor => s.hint_view_editor,
                            ViewMode::Split => s.hint_view_split,
                            ViewMode::Preview => s.hint_view_preview,
                        };
                        if ui
                            .selectable_label(selected, RichText::new(mode.label(s)).small())
                            .on_hover_text(hint)
                            .clicked()
                        {
                            actions.push(Action::SetView(*mode));
                        }
                    }

                    ui.separator();
                    if ui
                        .small_button(RichText::new(prefs.theme.label(s)).small())
                        .on_hover_text(s.hint_cycle_theme)
                        .clicked()
                    {
                        actions.push(Action::CycleTheme);
                    }
                });
            });
        });
}
