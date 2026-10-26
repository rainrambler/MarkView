//! 公式渲染：把 `$...$` / `$$...$$` 里的 LaTeX 画成贴图。
//!
//! `egui_commonmark` 只给了 `render_math_fn` 这一个钩子（见 `ui::preview`）：
//! 它负责认出公式并把 TeX 源码递过来，怎么画完全由宿主决定。这里用 RaTeX
//! （KaTeX 的纯 Rust 移植）解析 + 排版，得到一条 `DisplayList`，再导出成 SVG、
//! 交给 resvg 光栅化成贴图 —— 和 `crate::mermaid` 一样是"算一次、缓存成纹理"。
//!
//! 为什么绕一趟 SVG：`DisplayList` 里的字形是"字体 + 码位 + 缩放"的轮廓，
//! 自己描出来就得再实现一遍字体加载和系统字体回退（`\text{中文}` 会用到），
//! 而 `ratex-svg` 已经做完了（`embed_glyphs` 时字形直接变成 `<path>`），
//! resvg 又是 merman 正在用的依赖，多这一层几乎不增加构建时间。
//!
//! 没有像 Mermaid 那样加防抖：单个公式 release 下一次 0.6 ms 左右、debug 下 5 ms，
//! 在公式里逐字打字时每敲一下只重画这一个公式，感觉不出来。

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use egui::{Color32, ColorImage, Rect, Sense, TextureHandle, TextureOptions, Ui, Vec2};
use ratex_layout::{LayoutOptions, layout, to_display_list};
use ratex_parser::parser::parse;
use ratex_svg::{SvgOptions, render_to_svg};
use ratex_types::color::Color;
use ratex_types::display_item::DisplayList;
use ratex_types::math_style::MathStyle;

/// 缓存里最多留几张贴图。公式比 Mermaid 多得多，给大一点；
/// 超了就把最久没画过的那张挤出去 —— 贴图是显存，不能只进不出。
const MAX_ENTRIES: usize = 128;

/// 单个公式的源码长度上限。再长就不是给人看的公式了，解析开销不值得。
const MAX_SOURCE: usize = 4096;

/// display 公式（`$$...$$`）上下各留多少空白（点）。
const DISPLAY_MARGIN: f32 = 6.0;

/// 单张贴图的像素上限。超过就是排版失控了（宏展开出来的巨无霸），
/// 与其让分配器去碰运气，不如退回显示源码。
const MAX_PIXELS: u64 = 16 * 1024 * 1024;

/// 一份排版结果的尺寸，单位 em（1 em = 一个字号），和 `DisplayList` 一致。
///
/// `height` 在基线之上（ascent），`depth` 在基线之下（descent）——
/// 行内公式靠 `depth` 才能和旁边的文字对齐。
#[derive(Clone, Copy, Debug, PartialEq)]
struct Metrics {
    height: f32,
    depth: f32,
}

/// 一张画好的公式。
struct Entry {
    texture: TextureHandle,
    metrics: Metrics,
    /// 最近一次被用到时的序号，驱逐用。
    tick: u64,
}

/// 公式贴图缓存。放在 [`crate::app::App`] 里，跨帧复用。
#[derive(Default)]
pub struct MathCache {
    entries: HashMap<Key, Entry>,
    tick: u64,
}

impl MathCache {
    /// 换了文档就该清空，否则会显示上一个文件里的公式。
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// 取缓存，没有就现画。返回值借用 `self`，所以调用方要先把需要的东西拷出来。
    fn get_or_render(
        &mut self,
        ctx: &egui::Context,
        key: &Key,
        color: Color,
    ) -> Result<&Entry, String> {
        self.tick += 1;

        if !self.entries.contains_key(key) {
            let (image, metrics) = render(&key.tex, key.display, color, key.em_px as f32)?;
            let name = format!("markview.math.{:016x}", hash(key));
            let texture = ctx.load_texture(name, image, TextureOptions::LINEAR);
            self.entries.insert(
                key.clone(),
                Entry {
                    texture,
                    metrics,
                    tick: self.tick,
                },
            );
            self.evict();
        }

        let tick = self.tick;
        let entry = self.entries.get_mut(key).expect("上面刚插入过或本来就有");
        entry.tick = tick;
        Ok(entry)
    }

    fn evict(&mut self) {
        // 只挤掉这一帧没用到的：一篇公式特别多的文档里，屏幕上那些每一帧都要用，
        // 挤掉它们就会变成"每帧重画"，比多占几 MB 显存糟糕得多
        let tick = self.tick;
        while self.entries.len() > MAX_ENTRIES {
            let Some(oldest) = self
                .entries
                .iter()
                .filter(|(_, entry)| entry.tick < tick)
                .min_by_key(|(_, entry)| entry.tick)
                .map(|(key, _)| key.clone())
            else {
                return;
            };
            self.entries.remove(&oldest);
        }
    }
}

/// 缓存键：同样的源码 + 同样的排版条件才复用同一张贴图。
#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    tex: String,
    display: bool,
    /// 文字颜色。深浅色主题换色就得重画，否则深色界面里会出现黑字。
    color: [u8; 4],
    /// 每 em 多少物理像素（取整）。亚像素级的差异不值得重画一张，
    /// 顺手也让同一个缩放档位下反复重排时命中同一份缓存。
    em_px: u32,
}

/// 画一个公式。`display` 为真表示 `$$...$$`（独占一行、居中）。
///
/// 传进来的是 [`RefCell`] 而不是 `&mut`：`egui_commonmark` 的回调是 `Fn`，
/// 只肯借出不可变引用，缓存的更新只能走内部可变性。
pub fn show(cache: &RefCell<MathCache>, ui: &mut Ui, tex: &str, display: bool) {
    let tex = tex.trim();
    if tex.is_empty() {
        return;
    }
    if tex.len() > MAX_SOURCE {
        show_source(ui, tex, display);
        return;
    }

    let color = ui.visuals().text_color();
    let font_id = egui::TextStyle::Body.resolve(ui.style());
    let em = font_id.size;
    let em_px = (em * ui.ctx().pixels_per_point()).round().clamp(6.0, 512.0);
    let (row_height, ascent) = row_metrics(ui, font_id);

    let key = Key {
        tex: tex.to_owned(),
        display,
        color: color.to_array(),
        em_px: em_px as u32,
    };

    // 借用要在这里结束：下面还要拿 ui 去分配空间
    let painted = {
        let mut cache = cache.borrow_mut();
        cache
            .get_or_render(ui.ctx(), &key, ratex_color(color))
            .map(|entry| (entry.texture.id(), entry.metrics, entry.texture.size_vec2()))
    };
    let (texture, metrics, texture_size) = match painted {
        Ok(painted) => painted,
        // 语法还没写对（打字到一半就是这种状态）：把源码原样摆出来，别让这块地方凭空消失
        Err(_) => {
            show_source(ui, tex, display);
            return;
        }
    };

    // 贴图是按 em_px 画的，这里是"每个贴图像素对应多少点"，交给 egui 缩放
    let scale = em / em_px;
    let size = texture_size * scale;
    let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));

    if display {
        // 占满一整行，于是后面的内容自然换到下一行；公式本身居中
        let available = ui.max_rect().width().max(ui.available_width());
        let fit = (available / size.x).clamp(0.1, 1.0);
        let scaled = size * fit;
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(available, scaled.y + DISPLAY_MARGIN * 2.0),
            Sense::hover(),
        );
        let image_rect = Rect::from_center_size(rect.center(), scaled);
        ui.painter().image(texture, image_rect, uv, Color32::WHITE);
        response.on_hover_text(tex);
        return;
    }

    // 比预览区还宽的公式（一长串推导）缩一缩，硬塞会直接横向溢出
    let fit = (ui.max_rect().width() / size.x).clamp(0.2, 1.0);
    let size = size * fit;
    let (height, top_offset) = inline_placement(metrics, em * fit, ascent, row_height);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(size.x, height), Sense::hover());
    let image_rect = Rect::from_min_size(egui::pos2(rect.left(), rect.top() + top_offset), size);
    ui.painter().image(texture, image_rect, uv, Color32::WHITE);
    response.on_hover_text(tex);
}

/// 行内公式的摆放：返回（这一行至少多高，图片顶部相对行顶的偏移）。
///
/// 公式的基线要落在文字基线上。egui 把换行行里的东西都摆在同一顶部线上，
/// 所以基线位置得自己算：文字基线在行顶下方 `ascent` 处，公式的基线也放到那里。
///
/// 比正文高的公式（行内分式、定界符）放不进这一行：超出部分一半往上顶、一半往下压，
/// 行高随之加高。全部往上顶会压到上一行的字，全部往下压又会让公式明显掉到文字基线
/// 以下，各分一半看着最自然。
fn inline_placement(metrics: Metrics, em: f32, ascent: f32, row_height: f32) -> (f32, f32) {
    let above = metrics.height * em;
    let baseline = ascent + (above - ascent).max(0.0) * 0.5;
    let height = (baseline + metrics.depth * em).max(row_height);
    (height, baseline - above)
}

/// 导出 HTML 时，一个 em 折成多少 SVG 用户单位。
///
/// 只影响坐标精度：尺寸最后是以 em 写在 `width`/`height` 上的，
/// 于是公式会跟着正文字号和浏览器缩放一起变，打印也还是矢量。
const EXPORT_EM: f64 = 16.0;

/// 把公式渲染成一段能直接塞进 HTML 的内联 SVG。
///
/// 返回 `None` 表示排不出来（语法错误、太大），调用方应该退回显示源码。
///
/// 颜色用 `currentColor`：导出的页面本身跟随 `prefers-color-scheme`，
/// 公式跟着文字颜色走就不用为深色主题再存一份。公式里显式写了 `\color{...}`
/// 的部分不受影响 —— 只把基准色换掉。
pub fn to_html(tex: &str, display: bool) -> Option<String> {
    let tex = tex.trim();
    if tex.is_empty() || tex.len() > MAX_SOURCE {
        return None;
    }

    let list = typeset(tex, display, Color::BLACK).ok()?;
    let svg = render_to_svg(
        &list,
        &SvgOptions {
            font_size: EXPORT_EM,
            padding: 0.0,
            embed_glyphs: true,
            ..Default::default()
        },
    );
    let svg = svg.replace("rgba(0,0,0,1)", "currentColor");

    // 行内公式要压在文字基线上，做法和 KaTeX 的输出一样：往下挪一个 depth
    let size = format!(
        r#" width="{}em" height="{}em""#,
        em(list.width as f32),
        em(list.total_height() as f32)
    );
    // 没有深度（公式都落在基线以上）就不用写这个 style
    let size = if display || list.depth <= 0.001 {
        size
    } else {
        format!(
            r#"{size} style="vertical-align:-{}em""#,
            em(list.depth as f32)
        )
    };
    let svg = resize_root(&svg, &size)?;

    let class = if display {
        "math math-display"
    } else {
        "math math-inline"
    };
    // 屏读器读不出 SVG 里的字形，把 TeX 源码挂在 aria-label 上：至少能听出个大概
    Some(format!(
        r#"<span class="{class}" role="img" aria-label="{}">{svg}</span>"#,
        escape_attribute(tex)
    ))
}

/// 用 `attrs` 替换 `<svg>` 开标签上原本的 `width`/`height`（ratex-svg 写的是 pt）。
///
/// 拿不到预期结构就返回 `None`：调用方退回源码，总比输出一段坏掉的 SVG 强。
fn resize_root(svg: &str, attrs: &str) -> Option<String> {
    let head_end = svg.find('>')?;
    let (head, rest) = svg.split_at(head_end);
    let cut = head.find(" width=")?;
    Some(format!("{}{attrs}{rest}", &head[..cut]))
}

/// 把 TeX 源码放进 HTML 属性里（标签、引号、& 都要转义）。
fn escape_attribute(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// em 数量写进 HTML 属性：三位小数够用了（16px 字号下 0.001em ≈ 0.016px）。
fn em(value: f32) -> String {
    let text = format!("{value:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text.is_empty() || text == "-" {
        "0".to_owned()
    } else {
        text.to_owned()
    }
}

/// 公式画不出来（或者太长）时，原样把源码摆出来。
fn show_source(ui: &mut Ui, tex: &str, display: bool) {
    let source = if display {
        format!("$${tex}$$")
    } else {
        format!("${tex}$")
    };
    ui.label(egui::RichText::new(source).color(ui.visuals().warn_fg_color));
}

/// 正文那一行文字的（行高, 基线到行顶的距离），单位点。
///
/// 公式要和文字对齐就得知道基线在哪：`ascent` 是基线的位置，行高决定这一行
/// 至少多高（比文字矮的公式不该把行压扁）。
fn row_metrics(ui: &Ui, font_id: egui::FontId) -> (f32, f32) {
    let fallback = ui.text_style_height(&egui::TextStyle::Body);
    let galley = ui
        .ctx()
        .fonts_mut(|fonts| fonts.layout_no_wrap("x".to_owned(), font_id, Color32::WHITE));
    let Some(placed) = galley.rows.first() else {
        return (fallback, fallback * 0.8);
    };
    let row = &placed.row;
    // 字形的 pos.y 就是这一行的基线位置
    let ascent = row
        .glyphs
        .first()
        .map_or(row.size.y * 0.8, |glyph| glyph.pos.y);
    (row.size.y.max(fallback), ascent)
}

/// 缓存键的哈希，用来给贴图起名字（同名贴图会互相顶掉，所以必须跟着键走）。
fn hash(key: &Key) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

fn ratex_color(color: Color32) -> Color {
    Color::new(
        color.r() as f32 / 255.0,
        color.g() as f32 / 255.0,
        color.b() as f32 / 255.0,
        color.a() as f32 / 255.0,
    )
}

/// 解析 + 排版，得到一条画图指令列表（长度单位都是 em）。
fn typeset(tex: &str, display: bool, color: Color) -> Result<DisplayList, String> {
    let nodes = parse(tex).map_err(|err| format!("{err:?}"))?;
    let options = LayoutOptions {
        style: if display {
            MathStyle::Display
        } else {
            MathStyle::Text
        },
        color,
        ..Default::default()
    };
    let list = to_display_list(&layout(&nodes, &options));
    if list.width <= 0.0 || list.total_height() <= 0.0 {
        return Err("公式是空的".to_owned());
    }
    Ok(list)
}

/// 排版 + 光栅化。`em_px` 是一个字号有多少物理像素。
fn render(
    tex: &str,
    display: bool,
    color: Color,
    em_px: f32,
) -> Result<(ColorImage, Metrics), String> {
    let list = typeset(tex, display, color)?;

    let svg = render_to_svg(
        &list,
        &SvgOptions {
            font_size: em_px as f64,
            padding: 0.0,
            // 字形直接写成 `<path>`，这样光栅化时不需要字体环境
            embed_glyphs: true,
            ..Default::default()
        },
    );
    let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default())
        .map_err(|err| err.to_string())?;

    let size = tree.size();
    let width = (size.width().ceil() as u32).max(1);
    let height = (size.height().ceil() as u32).max(1);
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err("公式太大".to_owned());
    }
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(width, height).ok_or_else(|| "公式太大".to_owned())?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );

    // tiny-skia 和 epaint 用的都是预乘 sRGB，可以直接搬
    let image =
        ColorImage::from_rgba_premultiplied([width as usize, height as usize], pixmap.data());
    let metrics = Metrics {
        height: list.height as f32,
        depth: list.depth as f32,
    };
    Ok((image, metrics))
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

    /// 在无头 egui 里渲染三帧（前两帧让排版稳定、并把贴图建起来），
    /// 返回第三帧的（文字, 公式贴图）图元。
    fn render(source: &str) -> (Vec<(egui::Rect, String)>, Vec<egui::Rect>) {
        let cache = std::rc::Rc::new(RefCell::new(MathCache::default()));
        let mut commonmark = CommonMarkCache::default();
        let mut text = source.to_owned();
        let ctx = egui::Context::default();
        let mut result = (Vec::new(), Vec::new());

        for frame in 0..3 {
            let math = cache.clone();
            let math_fn = move |ui: &mut Ui, tex: &str, inline: bool| show(&math, ui, tex, !inline);
            let math_fn: Option<&egui_commonmark::RenderMathFn> = Some(&math_fn);

            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                ui.set_max_width(600.0);
                CommonMarkViewer::new()
                    .default_width(Some(600))
                    .max_image_width(Some(600))
                    .render_math_fn(math_fn)
                    .show_mut(ui, &mut commonmark, &mut text);
            });
            if frame == 2 {
                assert!(output.textures_delta.set.is_empty(), "第三帧不该再上传贴图");
                result = collect(&output.shapes);
            }
            // 不放掉纹理增量的话，epaint 的 debug 断言会在 drop 时 panic
            output.textures_delta.clear();
        }

        result
    }

    fn collect(
        shapes: &[egui::epaint::ClippedShape],
    ) -> (Vec<(egui::Rect, String)>, Vec<egui::Rect>) {
        fn walk(
            shape: &egui::epaint::Shape,
            texts: &mut Vec<(egui::Rect, String)>,
            images: &mut Vec<egui::Rect>,
        ) {
            match shape {
                egui::epaint::Shape::Text(text) => {
                    // 换行布局把横向偏移记成了首个 section 的 leading_space，
                    // 不是 shape 自己的 pos
                    let leading = text
                        .galley
                        .job
                        .sections
                        .first()
                        .map_or(0.0, |section| section.leading_space);
                    let min = egui::pos2(text.pos.x + leading, text.pos.y);
                    texts.push((
                        egui::Rect::from_min_size(min, text.galley.size()),
                        text.galley.job.text.clone(),
                    ));
                }
                // 公式是用 painter.image 画的，落下来就是一张带贴图的 mesh
                egui::epaint::Shape::Mesh(mesh)
                    if mesh.texture_id != egui::TextureId::default() =>
                {
                    images.push(mesh.calc_bounds());
                }
                egui::epaint::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, texts, images);
                    }
                }
                _ => {}
            }
        }

        let mut texts = Vec::new();
        let mut images = Vec::new();
        for clipped in shapes {
            walk(&clipped.shape, &mut texts, &mut images);
        }
        (texts, images)
    }

    fn rect_of(texts: &[(egui::Rect, String)], needle: &str) -> egui::Rect {
        texts
            .iter()
            .find(|(_, text)| text.contains(needle))
            .unwrap_or_else(|| panic!("没有找到 {needle:?} 的文字图元"))
            .0
    }

    fn metrics(height: f32, depth: f32) -> Metrics {
        Metrics { height, depth }
    }

    #[test]
    fn short_formula_sits_on_the_text_baseline() {
        // 0.8em 高的公式（x^2 这种）不该把行撑高，基线正好落在 ascent 上
        let (height, top) = inline_placement(metrics(0.8, 0.2), 10.0, 8.0, 12.0);
        assert_eq!(height, 12.0);
        assert!((top - 0.0).abs() < 0.01);
        // 图片顶部 + 基线以上部分 = 基线
        assert!((top + 0.8 * 10.0 - 8.0).abs() < 0.01);
    }

    #[test]
    fn tall_formula_shares_its_overflow_between_both_sides() {
        // 2em 高的行内分式：比 ascent(8) 高 12，一半往上顶(6)、一半往下压(6)
        let (height, top) = inline_placement(metrics(2.0, 0.5), 10.0, 8.0, 12.0);
        assert!((height - 19.0).abs() < 0.01, "height = {height}");
        assert!((top + 6.0).abs() < 0.01, "top = {top}");
    }

    #[test]
    fn inline_math_stays_on_the_same_line() {
        let (texts, images) = render("前 $x^2$ 后");
        assert_eq!(images.len(), 1, "行内公式应该画出一张贴图");

        let before = rect_of(&texts, "前");
        let after = rect_of(&texts, "后");
        let formula = images[0];

        // 同一行：竖直方向上有重叠
        assert!(
            formula.min.y < before.max.y && formula.max.y > before.min.y,
            "公式 {formula:?} 不在文字 {before:?} 那一行"
        );
        // 顺序：前、公式、后
        assert!(formula.min.x >= before.max.x - 1.0, "公式跑到了前文左边");
        assert!(after.min.x >= formula.max.x - 1.0, "后文没有排在公式右边");
        // 基线对齐：公式底边最多比文字底边低一点（公式自带的 descent 可能比文字深）
        assert!(
            formula.max.y <= before.max.y + 2.0,
            "公式 {formula:?} 掉到文字 {before:?} 下面去了"
        );
    }

    #[test]
    fn display_math_takes_its_own_line() {
        let (texts, images) = render("前\n\n$$\\frac{1}{2}$$\n\n后");
        assert_eq!(images.len(), 1, "块级公式应该画出一张贴图");

        let before = rect_of(&texts, "前");
        let after = rect_of(&texts, "后");
        let formula = images[0];

        assert!(
            formula.min.y >= before.max.y - 1.0,
            "块级公式没有换到下一行：{formula:?} / {before:?}"
        );
        assert!(
            after.min.y >= formula.max.y - 1.0,
            "块级公式后面的文字没有换行：{after:?} / {formula:?}"
        );
        // 居中：左右留白差不多
        let left = formula.min.x;
        let right = 600.0 - formula.max.x;
        assert!(
            (left - right).abs() < 2.0,
            "块级公式没有居中：左 {left} / 右 {right}"
        );
    }

    #[test]
    fn dark_theme_draws_light_glyphs() {
        // 深色主题下公式必须跟着变浅：光栅化时把文字颜色写进了 SVG，
        // 不是画完再反色，所以这里直接看贴图里最亮的像素
        let cache = std::rc::Rc::new(RefCell::new(MathCache::default()));
        let mut commonmark = CommonMarkCache::default();
        let mut text = "前 $x^2$ 后".to_owned();
        let ctx = egui::Context::default();
        ctx.set_visuals(egui::Visuals::dark());

        let mut brightest = 0u8;
        for _ in 0..2 {
            let math = cache.clone();
            let math_fn = move |ui: &mut Ui, tex: &str, inline: bool| show(&math, ui, tex, !inline);
            let math_fn: Option<&egui_commonmark::RenderMathFn> = Some(&math_fn);
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                ui.set_max_width(600.0);
                CommonMarkViewer::new()
                    .default_width(Some(600))
                    .max_image_width(Some(600))
                    .render_math_fn(math_fn)
                    .show_mut(ui, &mut commonmark, &mut text);
            });
            for parts in output.textures_delta.set.values() {
                for delta in parts {
                    let egui::epaint::ImageData::Color(image) = &delta.image;
                    for pixel in &image.pixels {
                        brightest = brightest.max(pixel.r().max(pixel.g()).max(pixel.b()));
                    }
                }
            }
            output.textures_delta.clear();
        }

        assert!(
            brightest > 128,
            "深色主题下公式应该是浅色的，实际最亮的像素只有 {brightest}"
        );
    }

    #[test]
    fn broken_math_falls_back_to_the_source() {
        let (texts, images) = render("前 $\\frac{1}{$ 后");
        assert!(images.is_empty(), "坏公式不该画出贴图");
        assert!(
            texts.iter().any(|(_, text)| text.contains("frac")),
            "坏公式应该把源码留在原地"
        );
    }
}
