//! 编辑区：带行号、语法高亮、查找栏的 Markdown 源码编辑器。
//!
//! 行号对齐的做法：`TextEdit::show()` 会把排好版的 `Galley` 连同它在屏幕上的
//! 位置（`galley_pos`）一起返回，而 `Galley.rows` 里每一行都带 `pos`（相对
//! galley 的偏移）和 `ends_with_newline`。于是"第 N 个逻辑行画在哪个 y"是可以
//! 精确算出来的 —— 即使这一行因为太长被软换行成了好几屏行，编号也不会错位。

use egui::text::{CCursor, CCursorRange};
use egui::widgets::text_edit::{TextEditOutput, TextEditState};
use egui::{
    Align, Align2, Context, Layout, Margin, Rect, RichText, ScrollArea, TextEdit, TextStyle, Ui,
    pos2, vec2,
};

use crate::app::{Action, App, editor_id};

/// 行号与右边界之间的距离。
const GUTTER_PAD: f32 = 9.0;
/// 编辑区内容内边距，直接来自 TextEdit 的 margin，算行号时要减掉。
const EDIT_MARGIN_X: i8 = 8;
const EDIT_MARGIN_Y: i8 = 4;

pub fn show(app: &mut App, ui: &mut Ui, actions: &mut Vec<Action>) {
    if app.find.open {
        find_bar(app, ui, actions);
    }

    let mut changed = false;
    let mut new_cursor: Option<(usize, usize)> = None;

    {
        let App {
            doc,
            prefs,
            code_theme,
            focus_editor,
            reveal,
            ..
        } = app;

        let wrap = prefs.editor_wrap;
        let show_numbers = prefs.show_line_numbers;

        // 行号槽的宽度按"总行数有几位"来留，长文档也不会被挤掉
        // 注意 `row_height` 要 &mut Fonts（内部有排版缓存），所以走 fonts_mut
        let row_height = ui
            .ctx()
            .fonts_mut(|fonts| fonts.row_height(&TextStyle::Monospace.resolve(ui.style())));
        let total_lines = doc.text.split('\n').count();
        let digits = total_lines.to_string().len().max(2) as f32;
        let gutter_width = if show_numbers {
            row_height * digits * 0.62 + GUTTER_PAD * 2.0
        } else {
            0.0
        };

        // 光标行取上一帧的状态就够了，省得为了画一个标记把排版再来一遍
        let cursor_line_prev = TextEditState::load(ui.ctx(), editor_id())
            .and_then(|state| state.cursor.char_range())
            .map(|range| line_col_of(&doc.text, usize::from(range.primary.index)).0);

        let visuals = ui.visuals().clone();

        egui::Frame::new()
            .fill(visuals.extreme_bg_color)
            .inner_margin(Margin::ZERO)
            .show(ui, |ui| {
                let scroll = if wrap {
                    ScrollArea::vertical()
                } else {
                    // 不换行时必须能横向滚动，否则长行直接看不见了
                    ScrollArea::both()
                };

                scroll
                    .id_salt("mdviewer.editor.scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            // 先占住行号槽的宽度，稍后再往里画数字
                            let (_id, gutter_rect) = ui.allocate_space(vec2(gutter_width, 0.0));

                            let theme: &egui_extras::syntax_highlighting::CodeTheme = code_theme;
                            let mut layouter =
                                |ui: &Ui, buffer: &dyn egui::TextBuffer, wrap_width: f32| {
                                    let mut job = egui_extras::syntax_highlighting::highlight(
                                        ui.ctx(),
                                        ui.style(),
                                        theme,
                                        buffer.as_str(),
                                        "Markdown",
                                    );
                                    job.wrap.max_width =
                                        if wrap { wrap_width } else { f32::INFINITY };
                                    ui.ctx().fonts_mut(|fonts| fonts.layout_job(job))
                                };

                            let output = TextEdit::multiline(&mut doc.text)
                                .id(editor_id())
                                .font(TextStyle::Monospace)
                                .desired_width(if wrap { f32::INFINITY } else { 0.0 })
                                .desired_rows(1)
                                .margin(Margin::symmetric(EDIT_MARGIN_X, EDIT_MARGIN_Y))
                                .frame(egui::Frame::NONE)
                                .lock_focus(true)
                                .layouter(&mut layouter)
                                .show(ui);

                            changed = output.response.response.changed();

                            if show_numbers {
                                paint_gutter(
                                    ui,
                                    &output,
                                    gutter_rect,
                                    row_height,
                                    cursor_line_prev,
                                    &visuals,
                                );
                            }

                            new_cursor = output.cursor_range.map(|range| {
                                line_col_of(&doc.text, usize::from(range.primary.index))
                            });

                            // 跳转请求：点大纲、按 ⌘G 都会塞一个 reveal 进来
                            if let Some(target) = reveal.take() {
                                set_selection(ui.ctx(), target.char_start, target.char_end);
                                let line = line_col_of(&doc.text, target.char_start).0;
                                if let Some(top) = line_top(&output.galley, line) {
                                    let rect = Rect::from_min_size(
                                        pos2(output.galley_pos.x, output.galley_pos.y + top),
                                        vec2(1.0, row_height),
                                    );
                                    ui.scroll_to_rect(rect, Some(Align::Center));
                                }
                            }

                            // 把键盘焦点交回编辑器（关掉查找栏、点击大纲之后）
                            if *focus_editor {
                                output.response.response.request_focus();
                                *focus_editor = false;
                            }
                        });
                    });
            });
    }

    if changed {
        app.touch();
    }
    if let Some((line, col)) = new_cursor {
        app.cursor_line = line;
        app.cursor_col = col;
    }
}

// ------------------------------------------------------------------ 绘制

/// 把行号画进预留出来的槽位里。
///
/// `row.pos.y` 是相对 galley 的，加上 `galley_pos.y` 就是屏幕坐标；视口外的行
/// 由 painter 自己的裁剪矩形丢弃，所以这里只做一次粗略的距离判断就够了。
fn paint_gutter(
    ui: &Ui,
    output: &TextEditOutput,
    gutter: Rect,
    row_height: f32,
    cursor_line: Option<usize>,
    visuals: &egui::Visuals,
) {
    let clip = ui.clip_rect();
    let painter = ui.painter();
    let font_id = TextStyle::Monospace.resolve(ui.style());

    let number_color = visuals.weak_text_color();
    let active_color = visuals.strong_text_color();
    let marker_color = visuals.selection.bg_fill;

    let top_limit = clip.top() - row_height * 4.0;
    let bottom_limit = clip.bottom() + row_height;

    for_each_logical_line(&output.galley, |line_number, top| {
        let y = output.galley_pos.y + top;
        if y < top_limit || y > bottom_limit {
            return;
        }

        let active = cursor_line == Some(line_number);
        painter.text(
            pos2(gutter.right() - GUTTER_PAD, y),
            Align2::RIGHT_TOP,
            (line_number + 1).to_string(),
            font_id.clone(),
            if active { active_color } else { number_color },
        );
        if active {
            // 当前行左侧的小竖条
            painter.rect_filled(
                Rect::from_min_max(
                    pos2(gutter.left(), y),
                    pos2(gutter.left() + 2.5, y + row_height),
                ),
                egui::CornerRadius::ZERO,
                marker_color,
            );
        }
    });
}

/// 按"逻辑行"遍历 galley，对每个逻辑行的**首行**调用 `f(行号, y偏移)`。
///
/// 关键点：只有 `ends_with_newline == true` 的行才意味着一个逻辑行结束了；
/// 因为太长被软换行分出来的续行 `ends_with_newline` 是 false，所以不会被当成
/// 新的一行。行号槽和"跳转到第 N 行"都走这一个函数，保证两边永远不会算得不一样。
fn for_each_logical_line(galley: &egui::Galley, mut f: impl FnMut(usize, f32)) {
    let mut line = 0usize;
    let mut at_line_start = true;
    for row in galley.rows.iter() {
        if at_line_start {
            f(line, row.pos.y);
            at_line_start = false;
        }
        if row.ends_with_newline {
            line += 1;
            at_line_start = true;
        }
    }
}

/// 某个逻辑行首个屏幕行在 galley 内的 y 偏移。越界返回 `None`。
fn line_top(galley: &egui::Galley, target: usize) -> Option<f32> {
    let mut found = None;
    for_each_logical_line(galley, |line, top| {
        if line == target && found.is_none() {
            found = Some(top);
        }
    });
    found
}

/// 字符序号 -> (行, 列)，都是 0 基。一趟扫完。
fn line_col_of(text: &str, char_index: usize) -> (usize, usize) {
    let mut line = 0usize;
    let mut column = 0usize;
    for (index, ch) in text.chars().enumerate() {
        if index >= char_index {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 0;
        } else {
            column += 1;
        }
    }
    (line, column)
}

// ------------------------------------------------------------------ 查找栏

fn find_bar(app: &mut App, ui: &mut Ui, actions: &mut Vec<Action>) {
    let App { find, doc, .. } = app;

    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("查找").small().strong());

                let response = ui.add(
                    TextEdit::singleline(&mut find.query)
                        .id(egui::Id::new("mdviewer.find.query"))
                        .hint_text("关键字…")
                        .desired_width(180.0)
                        .font(TextStyle::Monospace),
                );
                if find.just_opened {
                    find.just_opened = false;
                    response.request_focus();
                }
                if response.changed() {
                    find.mark_dirty();
                }
                // 单行输入框里回车会让它失焦，借这个时机跳下一个
                if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    actions.push(Action::FindStep(1));
                }

                if ui
                    .checkbox(&mut find.case_sensitive, "Aa")
                    .on_hover_text("区分大小写")
                    .changed()
                {
                    find.mark_dirty();
                }

                if !find.query.is_empty() {
                    if find.matches.is_empty() {
                        ui.label(
                            RichText::new("无匹配")
                                .small()
                                .color(ui.visuals().warn_fg_color),
                        );
                    } else {
                        ui.label(
                            RichText::new(format!("{} / {}", find.current + 1, find.matches.len()))
                                .small(),
                        );
                    }
                }

                if ui.button("上一个").clicked() {
                    actions.push(Action::FindStep(-1));
                }
                if ui.button("下一个").clicked() {
                    actions.push(Action::FindStep(1));
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("关闭").clicked() {
                        actions.push(Action::CloseFind);
                    }
                    ui.label(RichText::new("⌘G 下一个 · Esc 关闭").small().weak());
                });
            });
        });

    // 这一帧就重算，匹配计数不用等到下一帧
    find.rebuild(&doc.text);
}

// ------------------------------------------------------------------ 格式化

/// 当前选区的字符范围（lo, hi）。
fn selection(ctx: &Context) -> Option<(usize, usize)> {
    let state = TextEditState::load(ctx, editor_id())?;
    let range = state.cursor.char_range()?;
    let a = usize::from(range.primary.index);
    let b = usize::from(range.secondary.index);
    Some((a.min(b), a.max(b)))
}

fn set_selection(ctx: &Context, lo: usize, hi: usize) {
    if let Some(mut state) = TextEditState::load(ctx, editor_id()) {
        state
            .cursor
            .set_char_range(Some(CCursorRange::two(CCursor::new(lo), CCursor::new(hi))));
        state.store(ctx, editor_id());
    }
}

/// 字符序号 -> 字节偏移。
fn char_to_byte(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(byte, _)| byte)
        .unwrap_or(text.len())
}

fn apply(app: &mut App, ctx: &Context, new_text: String, selection: (usize, usize)) {
    app.doc.text = new_text;
    set_selection(ctx, selection.0, selection.1);
    app.touch();
    app.focus_editor = true;
    // 格式化发生在面板画完之后，得主动要一帧
    ctx.request_repaint();
}

/// 把选区包进 `prefix`/`suffix`。再按一次会去掉这层包裹（切换语义）。
pub fn wrap_selection(ctx: &Context, app: &mut App, prefix: &str, suffix: &str) {
    let Some((lo, hi)) = selection(ctx) else {
        return;
    };

    let text = &app.doc.text;
    let start = char_to_byte(text, lo);
    let end = char_to_byte(text, hi);

    // 标记已经在了？那就是要取消。用 get 而不是切片，越界时返回 None 不会 panic。
    let already = start >= prefix.len()
        && text.get(start - prefix.len()..start) == Some(prefix)
        && text.get(end..end + suffix.len()) == Some(suffix);

    let (new_text, selection) = if already {
        let mut out = String::with_capacity(text.len());
        out.push_str(&text[..start - prefix.len()]);
        out.push_str(&text[start..end]);
        out.push_str(&text[end + suffix.len()..]);
        let at = lo - prefix.chars().count();
        (out, (at, at + (hi - lo)))
    } else {
        let mut out = String::with_capacity(text.len() + prefix.len() + suffix.len());
        out.push_str(&text[..start]);
        out.push_str(prefix);
        out.push_str(&text[start..end]);
        out.push_str(suffix);
        out.push_str(&text[end..]);
        let at = lo + prefix.chars().count();
        (out, (at, at + (hi - lo)))
    };

    apply(app, ctx, new_text, selection);
}

/// 给选区覆盖到的每一行加上（或去掉）行首标记，比如 `- ` 和 `> `。
pub fn toggle_line_prefix(ctx: &Context, app: &mut App, prefix: &str) {
    let Some((lo, hi)) = selection(ctx) else {
        return;
    };

    let text = &app.doc.text;
    let start = char_to_byte(text, lo);
    let end = char_to_byte(text, hi);

    // 把选区扩到完整的行边界
    let block_start = text[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let block_end = text[end..]
        .find('\n')
        .map(|i| end + i)
        .unwrap_or(text.len());
    let block = &text[block_start..block_end];

    let lines: Vec<&str> = block.split('\n').collect();
    // 每一行都已有标记 -> 整体去掉；否则整体加上
    let remove = !lines.is_empty() && lines.iter().all(|line| line.starts_with(prefix));

    let new_block = lines
        .iter()
        .map(|line| {
            if remove {
                line.strip_prefix(prefix).unwrap_or(line).to_owned()
            } else if line.trim().is_empty() {
                // 空行不加标记，否则改几次就会积出一堆孤零零的 "- "
                (*line).to_owned()
            } else {
                format!("{prefix}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let mut out = String::with_capacity(text.len() + new_block.len());
    out.push_str(&text[..block_start]);
    out.push_str(&new_block);
    out.push_str(&text[block_end..]);

    // 选中整块，方便接着再改
    let block_start_chars = text[..block_start].chars().count();
    let selection = (
        block_start_chars,
        block_start_chars + new_block.chars().count(),
    );

    apply(app, ctx, out, selection);
}

/// 把光标所在行设成 N 级标题；如果已经是同一级，则取消标题。
pub fn set_heading(ctx: &Context, app: &mut App, level: u8) {
    let Some((lo, _)) = selection(ctx) else {
        return;
    };

    let text = &app.doc.text;
    let at = char_to_byte(text, lo);

    let line_start = text[..at].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = text[at..].find('\n').map(|i| at + i).unwrap_or(text.len());
    let line = &text[line_start..line_end];

    let existing = line.chars().take_while(|c| *c == '#').count();
    let body = line.trim_start_matches('#');
    let body = body.strip_prefix(' ').unwrap_or(body);

    let new_line = if existing > 0 && existing == level as usize {
        body.to_owned()
    } else {
        format!("{} {}", "#".repeat(level as usize), body)
    };

    let mut out = String::with_capacity(text.len() + 8);
    out.push_str(&text[..line_start]);
    out.push_str(&new_line);
    out.push_str(&text[line_end..]);

    // 光标停在标题行末尾
    let line_start_chars = text[..line_start].chars().count();
    let at_end = line_start_chars + new_line.chars().count();

    apply(app, ctx, out, (at_end, at_end));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// 在无头 Context 里真的排一次版，拿到 egui 自己产出的 Galley —— 这样测的是
    /// "egui 的 PlacedRow 到底怎么标 ends_with_newline"，而不是我对它的猜测。
    fn layout(text: &str, max_width: f32) -> Arc<egui::Galley> {
        let ctx = Context::default();
        let mut result: Option<Arc<egui::Galley>> = None;
        let mut output = ctx.run_ui(Default::default(), |ui| {
            let mut job = egui::text::LayoutJob::default();
            job.append(
                text,
                0.0,
                egui::TextFormat {
                    font_id: egui::FontId::monospace(14.0),
                    ..Default::default()
                },
            );
            job.wrap.max_width = max_width;
            result = Some(ui.ctx().fonts_mut(|fonts| fonts.layout_job(job)));
        });
        // epaint 要求拿到的纹理增量必须被处理掉，否则 Drop 时会 panic
        output.textures_delta.clear();
        result.expect("应当排版出 galley")
    }

    fn logical_lines(galley: &egui::Galley) -> Vec<usize> {
        let mut lines = Vec::new();
        for_each_logical_line(galley, |line, _| lines.push(line));
        lines
    }

    /// 核心保证：一行太长被软换行切成多个屏幕行时，续行**不能**被算成新的逻辑行。
    #[test]
    fn soft_wrapped_continuation_does_not_start_a_new_line() {
        let text = format!("first\n{}\nthird", "x".repeat(400));
        let galley = layout(&text, 120.0);

        assert!(
            galley.rows.len() > 3,
            "400 个字符在 120px 宽下应当被切成多个屏幕行，实际只有 {} 行",
            galley.rows.len()
        );

        assert_eq!(
            logical_lines(&galley),
            vec![0, 1, 2],
            "三个逻辑行就该只有三个编号"
        );
    }

    /// 行号的 y 偏移必须严格递增，否则编号会互相叠在一起。
    #[test]
    fn line_number_positions_increase() {
        let galley = layout(&format!("a\n{}\nb\nc", "y".repeat(300)), 100.0);
        let mut tops = Vec::new();
        for_each_logical_line(&galley, |_, top| tops.push(top));

        assert_eq!(tops.len(), 4);
        for pair in tops.windows(2) {
            assert!(pair[0] < pair[1], "y 偏移应递增，实际 {tops:?}");
        }
    }

    /// 末尾换行之后还有一行空行（编辑器里确实看得见那一行）。
    #[test]
    fn trailing_newline_yields_an_extra_line() {
        assert_eq!(logical_lines(&layout("a\n", 400.0)), vec![0, 1]);
    }

    #[test]
    fn single_line_without_newline_has_one_number() {
        assert_eq!(logical_lines(&layout("only one line", 400.0)), vec![0]);
    }

    #[test]
    fn line_top_returns_row_y_and_none_when_out_of_range() {
        let galley = layout("aaa\nbbb\nccc", 400.0);
        assert_eq!(line_top(&galley, 0), Some(galley.rows[0].pos.y));
        assert_eq!(line_top(&galley, 1), Some(galley.rows[1].pos.y));
        assert_eq!(line_top(&galley, 99), None);
    }

    /// 光标行列换算要按**字符**数，不能按字节，否则中文行会算错。
    #[test]
    fn line_col_of_counts_chars_not_bytes() {
        let text = "中文abc\n第二行";
        assert_eq!(line_col_of(text, 0), (0, 0));
        assert_eq!(line_col_of(text, 5), (0, 5)); // 停在 \n 之前
        assert_eq!(line_col_of(text, 6), (1, 0)); // \n 之后
        assert_eq!(line_col_of(text, 8), (1, 2));
    }

    #[test]
    fn char_to_byte_handles_multibyte_and_overflow() {
        assert_eq!(char_to_byte("中文abc", 0), 0);
        assert_eq!(char_to_byte("中文abc", 2), 6); // 两个汉字各 3 字节
        assert_eq!(char_to_byte("中文abc", 99), 9); // 越界 -> 文本末尾
    }
}
