//! 预览区：用 egui_commonmark 渲染 Markdown。
//!
//! 这里用 `show_mut` 而不是 `show`，于是预览里的任务列表复选框是可以点的，
//! 点了会直接改回源文本（也就是"所见即所得"的那一点点）。
//!
//! 正文里出现 `mermaid` 围栏时走另一条路：`egui_commonmark` 没有自定义代码块渲染的钩子，
//! 所以按围栏把正文切成若干段 —— Markdown 段照旧交给它，Mermaid 段交给
//! `crate::mermaid` 渲染成贴图。没有 Mermaid 的文档仍然整篇一次渲染。
//!
//! 渲染的从来不是 `doc.text` 本身，而是 [`RenderSource`] 造的一份副本：它把围栏语言
//! 标签换成高亮器认得的写法（见那里的注释），渲染完再还原回原文。

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
    // 提前算好基地址：闭包里就只借用 prefs，下面才能同时可变借用渲染副本
    let base_uri = base_uri(doc.dir().as_deref());

    // 公式回调要求 `'static`（`RenderMathFn` 是 `dyn Fn + 'static`），
    // 所以闭包自己拎一份缓存句柄，而不是借用 `app.math`
    let math = math.clone();
    let render_math =
        move |ui: &mut Ui, tex: &str, inline: bool| math::show(&math, ui, tex, !inline);
    let render_math: Option<&egui_commonmark::RenderMathFn> =
        prefs.render_math.then_some(&render_math);

    let mut source = RenderSource::new(&doc.text);

    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(PAD_X as i8, 16))
        .show(ui, |ui| {
            ScrollArea::vertical()
                .id_salt("markview.preview.scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let viewer = || {
                        CommonMarkViewer::new()
                            // 相对路径的图片按文档所在目录解析；没有路径时退化成当前目录
                            .default_implicit_uri_scheme(base_uri.clone())
                            .max_image_width(Some(content_width as usize))
                            .default_width(Some(content_width as usize))
                            .enable_scroll_to_heading(true)
                            // `$...$` / `$$...$$` 交给 `crate::math` 画成贴图；
                            // 关掉开关时传 None，公式就按纯文本留在原地
                            .render_math_fn(render_math)
                    };

                    // 绝大多数文档没有 Mermaid：整篇渲染，任务列表的复选框能直接改回渲染副本
                    if !mermaid::has_mermaid(&source.text) {
                        let response = viewer().show_mut(ui, cache, &mut source.text);
                        changed = response.response.changed();
                        return;
                    }

                    // 有 Mermaid：逐段渲染，再把（可能被复选框改过的）文本重新拼起来。
                    // 拼装而不是原地改，是因为分段之后各处偏移都会变，重拼一遍最省心。
                    let mut rebuilt = String::with_capacity(source.text.len());
                    // Mermaid 块的序号：渲染缓存和防抖都按它排队
                    let mut block = 0usize;
                    for (index, segment) in mermaid::split(&source.text).into_iter().enumerate() {
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
                            Segment::Mermaid { raw, source: tex } => {
                                rebuilt.push_str(raw);
                                if let Err(err) = mermaid.show(ui, block, tex, content_width) {
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
                        source.text = rebuilt;
                    }
                });
        });

    if changed {
        // 复选框被点 -> 副本和原文只差那几个字节，还原标签后写回正文
        doc.text = source.into_document_text();
        // 走正常的"已修改"流程
        app.touch();
    }
}

/// 渲染用的正文副本：围栏语言标签被换成高亮器认得的写法。
///
/// `egui_commonmark` 关掉 `better_syntax_highlighting`（原因见 `Cargo.toml`）之后，
/// 代码块高亮走 `egui_extras`，而它解析语言标签用的是 `find_syntax_by_name`
/// （区分大小写）和 `find_syntax_by_extension`，**不认别名**：`find_syntax_by_token`
/// 认得的 ```rust / ```python，它认不得，整块直接退化成纯文本。
///
/// 所以渲染前先造一份副本，把这类标签换成 syntect 的规范名。表里的替换**只改大小写、
/// 字节数不变**，于是副本和原文的偏移一一对应：视图层改的只有复选框那一个字节，
/// 渲染完用 [`RenderSource::into_document_text`] 把标签换回去，就是用户的原文。
struct RenderSource {
    text: String,
    /// (副本里的字节偏移, 原文标签)，按偏移升序。
    aliases: Vec<(usize, &'static str)>,
}

/// 小写全名 -> syntect 的规范名。只有大小写不同的才收，长度必须一致。
///
/// 这张表是 syntect 默认语法集里"`find_syntax_by_token` 认得、`find_syntax_by_name`
/// 认不得"的那一批；`tests::alias_table_matches_length` 盯着长度这个前提。
const FENCE_ALIASES: &[(&str, &str)] = &[
    ("rust", "Rust"),
    ("python", "Python"),
    ("javascript", "JavaScript"),
    ("ruby", "Ruby"),
    ("perl", "Perl"),
    ("haskell", "Haskell"),
    ("erlang", "Erlang"),
    ("clojure", "Clojure"),
    ("objective-c", "Objective-C"),
];

impl RenderSource {
    fn new(text: &str) -> Self {
        let mut out = String::with_capacity(text.len());
        let mut aliases: Vec<(usize, &'static str)> = Vec::new();
        // 当前打开的围栏：(字符, 长度)。围栏内部不解析，免得把内容里的 ``` 当成新的开头
        let mut fence: Option<(u8, usize)> = None;

        // 围栏的判定方式和 `mermaid::split`、`markdown::outline` 保持一致
        for line in text.split_inclusive('\n') {
            let trimmed = line.trim();
            let lead = line.len() - line.trim_start().len();

            if let Some((ch, len)) = fence {
                if closes_fence(trimmed, ch, len) {
                    fence = None;
                }
                out.push_str(line);
                continue;
            }

            let Some((ch, len)) = opens_fence(trimmed) else {
                out.push_str(line);
                continue;
            };
            fence = Some((ch, len));

            // 围栏后第一个词就是语言标签，前面可能有空格
            let info = &trimmed[len..];
            let gap = info.len() - info.trim_start_matches([' ', '\t']).len();
            let info = &info[gap..];
            let tag_len = info.find(char::is_whitespace).unwrap_or(info.len());
            let tag = &info[..tag_len];

            match FENCE_ALIASES.iter().find(|(alias, _)| *alias == tag) {
                Some((alias, canonical)) => {
                    // 反引号/波浪号都是 ASCII，`len` 既是字符数也是字节偏移
                    let at = lead + len + gap;
                    aliases.push((out.len() + at, alias));
                    out.push_str(&line[..at]);
                    out.push_str(canonical);
                    out.push_str(&line[at + tag.len()..]);
                }
                None => out.push_str(line),
            }
        }

        Self { text: out, aliases }
    }

    /// 把（可能被复选框改过的）副本还原成用户的正文。
    fn into_document_text(mut self) -> String {
        for (at, alias) in self.aliases {
            let Some((_, canonical)) = FENCE_ALIASES.iter().find(|(name, _)| *name == alias) else {
                continue;
            };
            // 视图层只改复选框那一个字节（等长），偏移不会漂；真漂了（比如哪个版本开始
            // 改文本长度了）就干脆不还原，不能 panic，也不能把正文写坏
            if self.text.get(at..at + canonical.len()) == Some(*canonical) {
                self.text.replace_range(at..at + alias.len(), alias);
            }
        }
        self.text
    }
}

/// 这一行是不是围栏开头（3 个以上的 ` 或 ~）。入参已 `trim`，返回值里 `len` 也是字节偏移。
fn opens_fence(trimmed: &str) -> Option<(u8, usize)> {
    let first = *trimmed.as_bytes().first()?;
    if first != b'`' && first != b'~' {
        return None;
    }
    let len = trimmed.bytes().take_while(|b| *b == first).count();
    (len >= 3).then_some((first, len))
}

/// 收尾围栏：同一种字符、不短于开头，后面不许再有内容。
fn closes_fence(trimmed: &str, ch: u8, open_len: usize) -> bool {
    let len = trimmed.bytes().take_while(|b| *b == ch).count();
    len >= open_len && trimmed[len..].trim().is_empty()
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

    // -------------------------------------------------- RenderSource

    /// 替换必须等长，否则副本和正文的偏移就对不上，复选框会改错地方。
    #[test]
    fn alias_table_matches_length() {
        for (alias, canonical) in FENCE_ALIASES {
            assert_eq!(
                alias.len(),
                canonical.len(),
                "{alias} -> {canonical} 长度不同，不能原地替换"
            );
        }
    }

    #[test]
    fn fence_tags_are_rewritten_in_place() {
        let doc = "前言\n\n```rust\nlet x = 1;\n```\n\n后记\n";
        let source = RenderSource::new(doc);

        assert_eq!(source.text.len(), doc.len());
        assert!(source.text.contains("```Rust\n"));
        assert!(source.text.ends_with("```\n\n后记\n"));
        // 还原之后必须是用户的原文
        assert_eq!(source.into_document_text(), doc);
    }

    #[test]
    fn fence_content_and_unknown_tags_are_left_alone() {
        let doc = "```python\n# ```rust\nprint(1)\n```\n\n```text\nrust\n```\n";
        let source = RenderSource::new(doc);
        assert!(source.text.contains("```Python\n"));
        // 围栏内部（哪怕是 ```rust）和表外的标签都不能动
        assert!(source.text.contains("# ```rust\n"));
        assert!(source.text.contains("```text\nrust\n```"));
        assert_eq!(source.into_document_text(), doc);
    }

    #[test]
    fn tilde_fences_and_indented_fences_work_too() {
        let doc = "  ~~~python\n  print(1)\n  ~~~\n";
        let source = RenderSource::new(doc);
        assert!(source.text.contains("  ~~~Python\n"));
        assert_eq!(source.into_document_text(), doc);
    }

    /// 复选框改动（`[ ]` -> `[x]`）落在副本上，还原标签不能把它抹掉。
    #[test]
    fn checkbox_edits_survive_the_round_trip() {
        let doc = "```rust\nlet x = 1;\n```\n\n- [ ] 待办\n";
        let mut source = RenderSource::new(doc);
        let at = source.text.find("[ ]").expect("有复选框");
        source.text.replace_range(at..at + 3, "[x]");

        let restored = source.into_document_text();
        assert_eq!(restored, doc.replace("[ ]", "[x]"));
        assert!(restored.contains("```rust\n"));
    }

    /// 真正渲染一遍：```rust 必须还是彩色的。
    ///
    /// 这条是回归测试 —— 关掉 `better_syntax_highlighting` 之后，高亮器不认小写全名，
    /// 少了 [`RenderSource`] 的标签改写，代码块会静悄悄退化成纯文本。
    #[test]
    fn code_blocks_keep_their_colors() {
        let doc = "```rust\npub fn main() -> u8 { let answer = 42; \"text\" }\n```\n";
        let source = RenderSource::new(doc);

        let ctx = egui::Context::default();
        let mut cache = egui_commonmark::CommonMarkCache::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 600.0),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                CommonMarkViewer::new().show(ui, &mut cache, &source.text);
            });
        });
        output.textures_delta.clear();

        let mut colors = Vec::new();
        for clipped in &output.shapes {
            collect_text_colors(&clipped.shape, &mut colors);
        }
        colors.sort_unstable_by_key(|color| color.to_array());
        colors.dedup();
        assert!(
            colors.len() > 1,
            "代码块只有一种颜色 {:?}，语法高亮没生效",
            colors
        );
    }

    fn collect_text_colors(shape: &egui::Shape, out: &mut Vec<egui::Color32>) {
        match shape {
            egui::Shape::Text(text) => {
                for section in &text.galley.job.sections {
                    if !section.byte_range.is_empty() {
                        out.push(section.format.color);
                    }
                }
            }
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect_text_colors(shape, out);
                }
            }
            _ => {}
        }
    }
}
