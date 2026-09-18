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

    let strings = prefs.language.strings();
    let mut changed = false;
    let content_width = (ui.available_width() - PAD_X * 2.0).max(160.0);
    // 提前算好基地址：闭包里就只借用 prefs，下面才能同时可变借用 doc.text
    let base_uri = base_uri(doc.dir().as_deref());

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
