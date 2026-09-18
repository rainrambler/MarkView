//! 左侧大纲：列出文档里所有 ATX 标题，点一下就跳到对应行。

use egui::{Align, Layout, RichText, ScrollArea, Sense, Ui};

use crate::app::{Action, App, Reveal};
use crate::markdown;

/// 标题在侧栏里最多显示这么多字符，超出打省略号（完整文本放在悬停提示里）。
const MAX_TITLE_CHARS: usize = 34;

pub fn show(app: &mut App, ui: &mut Ui, _actions: &mut Vec<Action>) {
    let App {
        doc,
        headings,
        prefs,
        reveal,
        cursor_line,
        ..
    } = app;

    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("大纲").strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .small_button("✕")
                        .on_hover_text("隐藏大纲（⌘⇧O）")
                        .clicked()
                    {
                        prefs.show_outline = false;
                    }
                    if !headings.is_empty() {
                        ui.label(RichText::new(headings.len().to_string()).small().weak());
                    }
                });
            });

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(4.0);

            ScrollArea::vertical()
                .id_salt("mdviewer.outline.scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if headings.is_empty() {
                        ui.add_space(20.0);
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new("还没有标题").weak());
                            ui.add_space(2.0);
                            ui.label(RichText::new("用 # ## ### 起一行就有了").weak().small());
                        });
                        return;
                    }

                    let visuals = ui.visuals().clone();
                    for heading in headings.iter() {
                        let indent = (heading.level.saturating_sub(1) as f32) * 12.0;
                        let active = *cursor_line == heading.line;

                        let label = if heading.text.is_empty() {
                            "（空标题）".to_owned()
                        } else {
                            clip(&heading.text, MAX_TITLE_CHARS)
                        };

                        let mut text = RichText::new(label)
                            .size(if heading.level <= 2 { 14.0 } else { 12.5 })
                            .color(if active {
                                visuals.strong_text_color()
                            } else {
                                visuals.text_color()
                            });
                        if heading.level <= 2 {
                            text = text.strong();
                        }
                        if active {
                            text = text.underline();
                        }

                        ui.horizontal(|ui| {
                            ui.add_space(indent);
                            ui.label(
                                RichText::new(format!("H{}", heading.level))
                                    .size(10.0)
                                    .weak(),
                            );
                            let response = ui.add(egui::Label::new(text).sense(Sense::click()));
                            if response.clicked() {
                                let at = markdown::line_char_offset(&doc.text, heading.line);
                                *reveal = Some(Reveal {
                                    char_start: at,
                                    char_end: at,
                                });
                            }
                            response.on_hover_text(format!(
                                "第 {} 行\n{}",
                                heading.line + 1,
                                heading.text
                            ));
                        });
                    }
                });
        });
}

fn clip(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push('…');
    out
}
