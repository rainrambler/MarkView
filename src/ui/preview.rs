//! 预览区：用 egui_commonmark 渲染 Markdown。
//!
//! 这里用 `show_mut` 而不是 `show`，于是预览里的任务列表复选框是可以点的，
//! 点了会直接改回源文本（也就是"所见即所得"的那一点点）。
//!
//! 正文里出现 ```mermaid 时走另一条路：`egui_commonmark` 没有自定义代码块渲染的钩子，
//! 所以按围栏把正文切成若干段 —— Markdown 段照旧交给它，Mermaid 段交给
//! `crate::mermaid` 渲染成贴图。没有 Mermaid 的文档仍然整篇一次渲染。

use egui::{ScrollArea, Ui};
use egui_commonmark::CommonMarkViewer;

use crate::app::{Action, App};
use crate::document::Document;
use crate::mermaid::{self, Segment};

/// 预览内容左右各留多少内边距，图片最大宽度据此计算。
const PAD_X: f32 = 28.0;

pub fn show(app: &mut App, ui: &mut Ui, _actions: &mut Vec<Action>) {
    let App {
        doc,
        cache,
        prefs,
        mermaid,
        ..
    } = app;

    let mut changed = false;
    let content_width = (ui.available_width() - PAD_X * 2.0).max(160.0);
    // 提前算好基地址：闭包里就只借用 prefs，下面才能同时可变借用 doc.text
    let base_uri = base_uri(doc);

    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(PAD_X as i8, 16))
        .show(ui, |ui| {
            ScrollArea::vertical()
                .id_salt("mdviewer.preview.scroll")
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
                    for segment in mermaid::split(&doc.text) {
                        match segment {
                            Segment::Markdown(src) => {
                                // 纯空白段没有可渲染的东西，原样留下
                                if src.trim().is_empty() {
                                    rebuilt.push_str(src);
                                    continue;
                                }
                                // 段是借来的，渲染要可变借用，先拷一份出来
                                let mut scratch = src.to_owned();
                                viewer().show_mut(ui, cache, &mut scratch);
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
                                    viewer().show_mut(ui, cache, &mut raw.to_owned());
                                    ui.colored_label(
                                        ui.visuals().warn_fg_color,
                                        format!("Mermaid：{err}"),
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

/// 给预览里的相对图片路径拼一个 `file://` 基地址。
fn base_uri(doc: &Document) -> String {
    match doc.dir() {
        Some(dir) => {
            let path = dir.to_string_lossy();
            // 结尾必须有斜杠，否则会和文件名粘在一起
            if path.ends_with('/') {
                format!("file://{path}")
            } else {
                format!("file://{path}/")
            }
        }
        None => "file://".to_owned(),
    }
}
