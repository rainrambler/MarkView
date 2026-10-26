//! 预览区：用 egui_commonmark 渲染 Markdown。
//!
//! 这里用 `show_mut` 而不是 `show`，于是预览里的任务列表复选框是可以点的，
//! 点了会直接改回源文本（也就是"所见即所得"的那一点点）。
//!
//! 正文里出现 `mermaid` 围栏时走另一条路：`egui_commonmark` 没有自定义代码块渲染的钩子，
//! 所以按围栏把正文切成若干段 —— Markdown 段照旧交给它，Mermaid 段交给
//! `crate::mermaid` 渲染成贴图。没有 Mermaid 的文档仍然整篇一次渲染。

use std::path::Path;

use egui::{ScrollArea, Ui};
use egui_commonmark::CommonMarkViewer;

use crate::app::{Action, App};
use crate::i18n::fill;
use crate::math;
use crate::mermaid::{self, Segment};

/// 预览内容左右各留多少内边距，图片最大宽度据此计算。
const PAD_X: f32 = 28.0;

pub fn show(app: &mut App, ui: &mut Ui, _actions: &mut Vec<Action>) {
    let App {
        doc,
        cache,
        prefs,
        mermaid,
        math,
        ..
    } = app;

    let strings = prefs.language.strings();
    let mut changed = false;
    let content_width = (ui.available_width() - PAD_X * 2.0).max(160.0);
    // 提前算好基地址：闭包里就只借用 prefs，下面才能同时可变借用 doc.text
    let base_uri = base_uri(doc.dir().as_deref());

    // 公式回调要求 `'static`（`RenderMathFn` 是 `dyn Fn + 'static`），
    // 所以闭包自己拎一份缓存句柄，而不是借用 `app.math`
    let math = math.clone();
    let render_math =
        move |ui: &mut Ui, tex: &str, inline: bool| math::show(&math, ui, tex, !inline);
    let render_math: Option<&egui_commonmark::RenderMathFn> =
        prefs.render_math.then_some(&render_math);

    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(PAD_X as i8, 16))
        .show(ui, |ui| {
            ScrollArea::vertical()
                .id_salt("markview.preview.scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let viewer = || {
                        CommonMarkViewer::new()
                            .syntax_theme_dark(&prefs.syntax_dark)
                            .syntax_theme_light(&prefs.syntax_light)
                            // 相对路径的图片按文档所在目录解析；没有路径时退化成当前目录
                            .default_implicit_uri_scheme(base_uri.clone())
                            .max_image_width(Some(content_width as usize))
                            .default_width(Some(content_width as usize))
                            .enable_scroll_to_heading(true)
                            // `$...$` / `$$...$$` 交给 `crate::math` 画成贴图；
                            // 关掉开关时传 None，公式就按纯文本留在原地
                            .render_math_fn(render_math)
                    };

                    // 绝大多数文档没有 Mermaid：整篇渲染，任务列表的复选框能直接改回源文本
                    if !mermaid::has_mermaid(&doc.text) {
                        let response = viewer().show_mut(ui, cache, &mut doc.text);
                        changed = response.response.changed();
                        return;
                    }

                    // 有 Mermaid：逐段渲染，再把（可能被复选框改过的）文本重新拼起来。
                    // 拼装而不是原地改，是因为分段之后各处偏移都会变，重拼一遍最省心。
                    let mut rebuilt = String::with_capacity(doc.text.len());
                    // Mermaid 块的序号：渲染缓存和防抖都按它排队
                    let mut block = 0usize;
                    for (index, segment) in mermaid::split(&doc.text).into_iter().enumerate() {
                        match segment {
                            Segment::Markdown(src) => {
                                // 纯空白段没有可渲染的东西，原样留下
                                if src.trim().is_empty() {
                                    rebuilt.push_str(src);
                                    continue;
                                }
                                // 段是借来的，渲染要可变借用，先拷一份出来
                                let mut scratch = src.to_owned();
                                show_segment(ui, index, |ui| {
                                    viewer().show_mut(ui, cache, &mut scratch);
                                });
                                if scratch != src {
                                    changed = true;
                                }
                                rebuilt.push_str(&scratch);
                            }
                            Segment::Mermaid { raw, source } => {
                                rebuilt.push_str(raw);
                                if let Err(err) = mermaid.show(ui, block, source, content_width) {
                                    // 语法还没写对（打字到一半就是这种状态）：先按代码块显示，
                                    // 顺便把原因摆出来，别让这块地方凭空消失
                                    show_segment(ui, index, |ui| {
                                        viewer().show_mut(ui, cache, &mut raw.to_owned());
                                    });
                                    ui.colored_label(
                                        ui.visuals().warn_fg_color,
                                        fill(
                                            strings.mermaid_prefix,
                                            &[("err", &err.message(strings))],
                                        ),
                                    );
                                }
                                block += 1;
                            }
                        }
                    }

                    if changed {
                        doc.text = rebuilt;
                    }
                });
        });

    if changed {
        // 复选框被点 -> 源文本真的变了，走正常的"已修改"流程
        app.touch();
    }
}

/// 渲染一段 Markdown，并把它的持久化 id 和前后的段隔开。
///
/// 有 Mermaid 的文档是按围栏切成若干段、逐段交给 `egui_commonmark` 的，
/// 而每一段都会新建一个 `CommonMarkViewer` —— 它内部给表格起的 id 是
/// `ui.make_persistent_id(("_table", 序号))`，序号**每段都从 0 重新数**
/// （`CommonMarkViewerInternal::new()` 里 `curr_table: 0`）。
///
/// 段与段之间共用同一个 `ui`，于是「第一段的第 1 张表」和「第二段的第 1 张表」
/// 算出的是同一个 id：egui 把 Grid 的列宽、表格外层横向 `ScrollArea` 的偏移都存在
/// 这个 id 下面，**后一张表直接套用了前一张表的列宽**。列宽比内容窄、文字又不换行，
/// 前一格的文字就压到后一格里去了（egui 的 debug 构建还会打
/// "First/Second use of Grid ID ..."）。
///
/// 每段套一个独立的 `ui` id（`push_id`）之后，段内的编号再怎么重复也不会共用状态。
fn show_segment<R>(ui: &mut Ui, index: usize, render: impl FnOnce(&mut Ui) -> R) -> R {
    ui.push_id(("markview.preview.segment", index), render)
        .inner
}

/// 给预览里的相对图片路径拼一个 `file://` 基地址。
///
/// `dir` 是文档所在目录；`None` 表示文档还没保存过，退回当前目录。
fn base_uri(dir: Option<&Path>) -> String {
    let Some(dir) = dir else {
        return "file://".to_owned();
    };

    let path = file_url_path(&dir.to_string_lossy());
    // 三种形状，拼出来的 URL 各不相同：
    //   `//server/share`（UNC）-> `file://server/share/`，主机名走 authority 位
    //   `/home/u`             -> `file:///home/u/`，多一个斜杠凑出空 authority
    //   `D:/a`                -> `file:///D:/a/`，同上，否则盘符会被当成主机名
    let uri = if let Some(unc) = path.strip_prefix("//") {
        format!("file://{unc}")
    } else if path.starts_with('/') {
        format!("file://{path}")
    } else {
        format!("file:///{path}")
    };

    // 结尾必须有斜杠，否则会和文件名粘在一起
    format!("{}/", uri.trim_end_matches('/'))
}

/// 本地目录 -> 能塞进 `file://` URL 的路径片段。
///
/// Windows 上有两个坑：
///  * `fs::canonicalize` 返回的是 `\\?\D:\...` 扩展长度形式。原样放进 URL 就不再是
///    合法地址了（`?` 会被当成 query 的分隔符），微软家的 `.jpg` 图片就此消失；
///  * 路径分隔符是反斜杠，而 URL 只认正斜杠。
fn file_url_path(path: &str) -> String {
    // `\\?\UNC\server\share` 就是 `\\server\share` 的扩展长度写法，先还原成后者，
    // 好让下面统一按 `\\` 开头处理
    let path = match path.strip_prefix(r"\\?\UNC\") {
        Some(rest) => format!(r"\\{rest}"),
        None => path.strip_prefix(r"\\?\").unwrap_or(path).to_owned(),
    };

    path.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 把一帧里所有的文字图元连同它们的矩形收集出来（`Shape::Text` 才是真正画出去的字）。
    fn text_rects(shapes: &[egui::epaint::ClippedShape]) -> Vec<(egui::Rect, String)> {
        fn walk(shape: &egui::epaint::Shape, out: &mut Vec<(egui::Rect, String)>) {
            match shape {
                egui::epaint::Shape::Text(text) => out.push((
                    egui::Rect::from_min_size(text.pos, text.galley.size()),
                    text.galley.job.text.clone(),
                )),
                egui::epaint::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, out);
                    }
                }
                _ => {}
            }
        }

        let mut out = Vec::new();
        for clipped in shapes {
            walk(&clipped.shape, &mut out);
        }
        out
    }

    /// 找某个文字图元，返回它的矩形。
    fn rect_of(rects: &[(egui::Rect, String)], needle: &str) -> egui::Rect {
        rects
            .iter()
            .find(|(_, text)| text.contains(needle))
            .unwrap_or_else(|| panic!("没有找到 {needle:?} 的文字图元"))
            .0
    }

    /// 无头跑几帧，让 Grid 的列宽稳定下来，返回最后一帧的所有文字图元。
    fn render_segments(segments: &[&str]) -> Vec<(egui::Rect, String)> {
        let ctx = egui::Context::default();
        let mut cache = egui_commonmark::CommonMarkCache::default();
        let mut last = Vec::new();

        for _ in 0..4 {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                ui.set_max_width(600.0);
                for (index, src) in segments.iter().enumerate() {
                    let mut scratch = (*src).to_owned();
                    show_segment(ui, index, |ui| {
                        egui_commonmark::CommonMarkViewer::new()
                            .default_width(Some(500))
                            .max_image_width(Some(500))
                            .show_mut(ui, &mut cache, &mut scratch);
                    });
                }
            });
            // 不放掉纹理增量的话，epaint 的 debug 断言会在 drop 时 panic
            output.textures_delta.clear();
            last = text_rects(&output.shapes);
        }

        last
    }

    #[test]
    fn segments_do_not_share_table_column_widths() {
        // 第一段是一张窄表，第二段有一格超长文字。
        // 两段的表格编号都从 0 开始，共用一个 ui 时 Grid 的 id 会撞上：
        // 后一张表套用前一张的窄列宽，文字就压到下一格里（见 `show_segment`）。
        let narrow = "| a | b |\n| --- | --- |\n| 1 | 2 |\n";
        let wide = "| this_is_a_long_cell_name | 值 |\n\
                    | --- | --- |\n\
                    | very_long_environment_variable_name_for_testing | ok |\n";

        let rects = render_segments(&[narrow, wide]);

        // 表头行与数据行各查一遍：右边那格必须排在前一格文字的后面
        for (left, right) in [
            ("this_is_a_long_cell_name", "值"),
            ("very_long_environment_variable_name_for_testing", "ok"),
        ] {
            let left = rect_of(&rects, left);
            let right = rect_of(&rects, right);
            assert!(
                right.min.x >= left.max.x - 0.5,
                "{left:?} 的右边界 {:?} 越过了 {right:?} 的左边界 {:?}",
                left.max.x,
                right.min.x
            );
        }
    }

    #[test]
    fn windows_drive_paths_keep_the_drive_out_of_the_host_slot() {
        // `file://D:/a/` 会把 `D:` 当成主机名，必须写成 `file:///D:/a/`
        assert_eq!(
            base_uri(Some(Path::new(r"D:\docs\notes"))),
            "file:///D:/docs/notes/"
        );
    }

    #[test]
    fn canonicalized_windows_paths_lose_the_extended_length_prefix() {
        // fs::canonicalize 给的就是这个形状，`?` 直接塞进 URL 会变成 query 分隔符
        assert_eq!(
            base_uri(Some(Path::new(r"\\?\D:\docs"))),
            "file:///D:/docs/"
        );
        assert_eq!(
            base_uri(Some(Path::new(r"\\?\UNC\server\share\docs"))),
            "file://server/share/docs/"
        );
        assert_eq!(
            base_uri(Some(Path::new(r"\\server\share\docs"))),
            "file://server/share/docs/"
        );
    }

    #[test]
    fn unix_paths_become_a_three_slash_file_url() {
        assert_eq!(
            base_uri(Some(Path::new("/home/u/docs"))),
            "file:///home/u/docs/"
        );
    }

    #[test]
    fn trailing_separators_do_not_double_up() {
        // Path::new("/a/") 的 to_string_lossy 可能带尾斜杠，拼完不能变成 //
        assert_eq!(base_uri(Some(Path::new("/a/"))), "file:///a/");
    }

    #[test]
    fn an_unsaved_document_falls_back_to_the_plain_scheme() {
        assert_eq!(base_uri(None), "file://");
    }
}
