//! Mermaid 渲染：把 ```mermaid 围栏交给纯 Rust 的 `merman` 画成位图。
//!
//! 为什么要自己切段：`egui_commonmark` 没有"自定义代码块渲染"的钩子
//! （它只开放了行内 HTML 和公式两个回调），围栏代码块一律走语法高亮那条路。
//! 所以预览区先把正文按 `mermaid` 围栏切成若干段：Markdown 段照旧交给
//! `CommonMarkViewer`，Mermaid 段由这里渲染成贴图（见 `ui::preview`）。
//!
//! 渲染是同步的纯 CPU 活，结果按 (源码, 深浅色, 缩放比, 底色) 缓存下来，
//! 这样只有真正改过的那一段会重算；而这个重算并不便宜（debug 构建下小图一次半秒左右，
//! 进程里第一次还要额外等 resvg 加载系统字体），所以改动之后还会再等一小会儿 ——
//! 打字期间拿上一次的结果顶着，手停下来才重画（见 `REDRAW_DELAY`）。
//!
//! 配色走 merman 的 host theme（就是它给编辑器准备的那套接口，Zed 用的也是这个）：
//! 默认管线照搬 Mermaid 的行为，会往根节点上写 `background-color:white`，
//! 深色界面里那就是一块白板。

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use egui::{Color32, TextureHandle, TextureOptions, Ui};
use merman::render::raster::RasterOptions;
use merman::render::{HeadlessRenderer, HostThemeProfile, HostThemeRootBackground};

/// 缓存里最多留几张贴图。超了就把最久没用过的那张挤出去 —— 贴图是显存，不能只进不出。
const MAX_ENTRIES: usize = 16;

/// 源码变了之后至少等这么久才重画。
///
/// 在 Mermaid 块里逐字打字时，每敲一下都重跑一次解析+布局+光栅化太亏：
/// 这里先拿上一次画好的图顶着，手停下来再换成新的。
const REDRAW_DELAY: Duration = Duration::from_millis(250);

/// 正文被 `mermaid` 围栏切开之后的一段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Segment<'a> {
    /// 普通 Markdown，原样交给 `egui_commonmark`。
    Markdown(&'a str),
    /// 一个 Mermaid 块。`raw` 是连围栏在内的原文（写回用），`source` 是围栏内部的源码。
    Mermaid { raw: &'a str, source: &'a str },
}

impl<'a> Segment<'a> {
    /// 这一段在原文里的样子。所有段拼起来就是原文，一个字节都不多不少。
    ///
    /// 只有测试用它 —— 预览是照着两边的分支各自拼 `raw` / 渲染结果的，
    /// 但"拼回去等于原文"这条不变量得有人钉住，否则分段渲染迟早会悄悄丢字。
    #[cfg(test)]
    fn raw(self) -> &'a str {
        match self {
            Self::Markdown(raw) => raw,
            Self::Mermaid { raw, .. } => raw,
        }
    }
}

/// 一个开启中的 Mermaid 块。
#[derive(Clone, Copy)]
struct Fence {
    ch: char,
    len: usize,
    /// 开启围栏那一行的起点（字节）
    start: usize,
    /// 源码起点：开启围栏那一行的末尾
    content_start: usize,
}

/// 把正文切成 Markdown / Mermaid 交替的段。
///
/// 只有"信息串的第一个词是 mermaid"的围栏才算 Mermaid（大小写不敏感），
/// 其余围栏连同里面写的东西一起留在 Markdown 段里 —— 否则一篇讲 Mermaid 的文章
/// 里那个示例代码块自己就被画成图了。围栏的判定方式和 `markdown::outline` 保持一致，
/// 免得大纲跳过某段、预览却不跳过。
pub fn split(text: &str) -> Vec<Segment<'_>> {
    let mut segments = Vec::new();
    collect(text, |segment| {
        segments.push(segment);
        true
    });
    segments
}

/// 正文里有没有 Mermaid 块。预览据此决定走整篇渲染还是分段渲染。
pub fn has_mermaid(text: &str) -> bool {
    // 回调返回 false 表示"不用再看了"，于是 collect 返回 false
    !collect(text, |segment| {
        !matches!(segment, Segment::Mermaid { .. })
    })
}

/// 扫一遍正文，每切出一段就调一次 `on_segment`（返回 false 表示提前结束）。
///
/// 返回值表示"是否走到了文末"。切段逻辑集中在这里，`split` 和 `has_mermaid`
/// 只是两种收尾方式，省得同一套围栏规则写两遍。
fn collect<'a>(text: &'a str, mut on_segment: impl FnMut(Segment<'a>) -> bool) -> bool {
    // 当前 Markdown 段的起点（字节）
    let mut md_start = 0usize;
    // 正在收集的 Mermaid 块
    let mut open: Option<Fence> = None;
    // 普通围栏（非 mermaid）：只是借它跳过内部内容，
    // 否则里面的 ``` 会被误当成一个 Mermaid 块的开始
    let mut fence: Option<(char, usize)> = None;

    let mut line_start = 0usize;
    for line in text.split_inclusive('\n') {
        let line_end = line_start + line.len();
        let trimmed = line.trim();

        if let Some(block) = open {
            if is_close(trimmed, block.ch, block.len) {
                if md_start < block.start
                    && !on_segment(Segment::Markdown(&text[md_start..block.start]))
                {
                    return false;
                }
                // 源码不含闭合围栏行，也不含它前面那个换行（CRLF 也要去掉）
                if !on_segment(Segment::Mermaid {
                    raw: &text[block.start..line_end],
                    source: text[block.content_start..line_start].trim_end_matches(['\n', '\r']),
                }) {
                    return false;
                }
                md_start = line_end;
                open = None;
            }
            line_start = line_end;
            continue;
        }

        if let Some((ch, len)) = fence {
            if is_close(trimmed, ch, len) {
                fence = None;
            }
            line_start = line_end;
            continue;
        }

        if let Some((ch, len, info)) = fence_open(trimmed) {
            if is_mermaid_info(info) {
                open = Some(Fence {
                    ch,
                    len,
                    start: line_start,
                    content_start: line_end,
                });
            } else {
                fence = Some((ch, len));
            }
        }
        line_start = line_end;
    }

    // 收尾
    if let Some(block) = open {
        // 围栏一直没闭合：就当它到文件末尾为止，让渲染去报错，总比整段消失强
        if md_start < block.start && !on_segment(Segment::Markdown(&text[md_start..block.start])) {
            return false;
        }
        return on_segment(Segment::Mermaid {
            raw: &text[block.start..],
            source: text[block.content_start..].trim_end_matches(['\n', '\r']),
        });
    }

    if md_start < text.len() {
        return on_segment(Segment::Markdown(&text[md_start..]));
    }
    true
}

/// 是否是开启围栏的行。返回 (围栏字符, 长度, 信息串)。
fn fence_open(trimmed: &str) -> Option<(char, usize, &str)> {
    let ch = trimmed
        .chars()
        .next()
        .filter(|c| *c == '`' || *c == '~')?;
    let len = trimmed.chars().take_while(|c| *c == ch).count();
    // 反引号/波浪号都是 ASCII，`len` 既是字符数也是字节偏移
    (len >= 3).then(|| (ch, len, &trimmed[len..]))
}

/// 是否是闭合围栏的行：同类字符、不短于开启围栏、后面没有别的东西。
fn is_close(trimmed: &str, ch: char, len: usize) -> bool {
    let run = trimmed.chars().take_while(|c| *c == ch).count();
    run >= len && trimmed[run..].trim().is_empty()
}

/// 围栏信息串里第一个词是不是 `mermaid`（` ```mermaid ` 和 ` ```mermaid title=x ` 都算）。
fn is_mermaid_info(info: &str) -> bool {
    info.split_whitespace()
        .next()
        .is_some_and(|word| word.eq_ignore_ascii_case("mermaid"))
}

/// 一张画好的图。
struct Ready {
    texture: TextureHandle,
    /// 显示尺寸（逻辑点）。位图是按缩放比画的，这里已经除回去了。
    size: egui::Vec2,
}

enum Entry {
    Ready(Ready),
    Failed(String),
}

/// 正文里第 N 个 Mermaid 块的状态。按块号排队 —— 块号会随着编辑漂移，
/// 但对不上也只是多渲一次，不会出错。
struct BlockState {
    /// 这一帧看到的源码哈希
    wanted: u64,
    /// 上一次拿定主意的源码哈希（不管结论是画出来了还是画不出来）。
    /// 只要有它，就说明"这一块渲染过"，正在改的时候可以先顶着旧的等一等。
    attempted: Option<u64>,
    /// 上一次真画出来的源码哈希。顶着显示时优先用它 ——
    /// 打字打到一半语法错一会儿，也不该把已经画好的图抖掉。
    last_ok: Option<u64>,
    /// `wanted` 是哪一刻变的
    changed_at: Instant,
}

impl BlockState {
    fn new(hash: u64, now: Instant) -> Self {
        Self {
            wanted: hash,
            attempted: None,
            last_ok: None,
            changed_at: now,
        }
    }
}

/// Mermaid 渲染缓存与绘制入口。
///
/// 缓存键里带上深浅色、缩放比和底色：任何一样变了，图的配色或清晰度就跟着变。
pub struct MermaidCache {
    renderer: HeadlessRenderer,
    /// 值里第二个数是最后一次被用到的 `tick`，淘汰时挑最小的。
    entries: HashMap<u64, (Entry, u64)>,
    /// 按块号排的状态，见 [`BlockState`]。
    blocks: Vec<BlockState>,
    tick: u64,
}

impl Default for MermaidCache {
    fn default() -> Self {
        Self {
            renderer: HeadlessRenderer::new(),
            entries: HashMap::new(),
            blocks: Vec::new(),
            tick: 0,
        }
    }
}

impl MermaidCache {
    /// 换文档时清掉：贴图留着也用不上，还占显存。
    pub fn clear(&mut self) {
        self.entries.clear();
        self.blocks.clear();
    }

    /// 画正文里第 `block` 个 Mermaid 块。返回 `Err` 时调用方应该退回显示源码。
    pub fn show(
        &mut self,
        ui: &mut Ui,
        block: usize,
        source: &str,
        width: f32,
    ) -> Result<(), String> {
        let ctx = ui.ctx().clone();
        let ppp = ctx.pixels_per_point();
        let dark = ui.visuals().dark_mode;
        // 用页面底色当画布：图看起来就是文档的一部分，而不是贴上去的一块白纸
        let canvas = ui.visuals().panel_fill;
        let key = fingerprint(source, dark, ppp, canvas);
        let now = Instant::now();

        self.tick += 1;
        if block >= self.blocks.len() {
            self.blocks.resize_with(block + 1, || BlockState::new(key, now));
        }
        // 借用一个作用域里就还掉：下面还要 `&self` 去渲染，不能一直握着这个可变借用
        {
            let state = &mut self.blocks[block];
            if state.wanted != key {
                state.wanted = key;
                state.changed_at = now;
            }
        }

        // 该画哪一张？三种情况：
        //   1. 当前源码的结论已经在缓存里了（画出来了，或者已知画不出来）-> 直接用；
        //   2. 这一块刚被改过、还在 REDRAW_DELAY 之内 -> 先拿上一次的结果顶着，不在这儿重画。
        //      优先上一次画成功的图（打字打到一半语法错一会儿，不该把图抖掉）；
        //      从零开始敲的时候没有"上次成功的图"，那就顶上一次的失败结论，
        //      调用方自然会退回显示源码；
        //   3. 其余情况（第一次出现 / 手停下来了）-> 真去渲染。
        let (attempted, last_ok, changed_at) = {
            let state = &self.blocks[block];
            (state.attempted, state.last_ok, state.changed_at)
        };
        let idle = now.saturating_duration_since(changed_at);
        let stale = if self.entries.contains_key(&key) || idle >= REDRAW_DELAY {
            None
        } else {
            last_ok
                .filter(|previous| self.entries.contains_key(previous))
                .or_else(|| attempted.filter(|previous| self.entries.contains_key(previous)))
        };

        let draw = match stale {
            Some(previous) => {
                // 到点了自己来一帧，不然手停下来就没人催重画了
                ctx.request_repaint_after(REDRAW_DELAY - idle);
                previous
            }
            None => {
                let entry = match self.render(&ctx, key, source, dark, ppp, canvas) {
                    Ok(ready) => Entry::Ready(ready),
                    // 失败也缓存：否则每帧都要重跑一遍注定失败的解析
                    Err(err) => Entry::Failed(err),
                };
                self.store(key, entry);
                key
            }
        };

        {
            let state = &mut self.blocks[block];
            state.attempted = Some(draw);
            if matches!(self.entries.get(&draw), Some((Entry::Ready(_), _))) {
                state.last_ok = Some(draw);
            }
        }

        let Some((entry, used)) = self.entries.get_mut(&draw) else {
            return Err("缓存里没有这张图".to_owned());
        };
        *used = self.tick;

        match entry {
            Entry::Ready(ready) => {
                let size = fit(ready.size, width);
                // add_sized 会把图横向居中，比左对齐好看
                ui.add_sized(
                    egui::vec2(width, size.y),
                    egui::Image::new(&ready.texture).fit_to_exact_size(size),
                );
                Ok(())
            }
            Entry::Failed(err) => Err(err.clone()),
        }
    }

    fn render(
        &self,
        ctx: &egui::Context,
        key: u64,
        source: &str,
        dark: bool,
        ppp: f32,
        canvas: Color32,
    ) -> Result<Ready, String> {
        // 配色跟着 egui 走。host theme 会编译成一份 MermaidConfig + 输出管线，
        // 每次渲染都重建一遍，但只有缓存没命中时才会走到这里，不心疼。
        let profile = theme(dark, canvas);
        let renderer = self.renderer.clone().with_host_theme(&profile);

        let options = RasterOptions::default()
            // 界面缩放 2 倍时就多画一倍像素，省得图被放大后发虚
            .with_scale(ppp)
            // 根节点自己会铺一层 canvas，这里再兜一道：万一哪天管线不再写根背景，
            // 也不至于得到一张透明 PNG
            .with_background(css(canvas));

        let png = renderer
            .render_png_sync(source, &options)
            .map_err(|err| err.to_string())?
            .ok_or_else(|| "这段源码不像 Mermaid 图".to_owned())?;

        let image = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
            .map_err(|err| format!("PNG 解码失败：{err}"))?
            .into_rgba8();
        let (width, height) = image.dimensions();
        let texture = ctx.load_texture(
            format!("mdviewer.mermaid.{key:016x}"),
            egui::ColorImage::from_rgba_unmultiplied(
                [width as usize, height as usize],
                image.as_raw(),
            ),
            TextureOptions::LINEAR,
        );

        Ok(Ready {
            texture,
            // 位图按 ppp 倍画，除以 ppp 才是它该占的逻辑点尺寸
            size: egui::vec2(width as f32 / ppp, height as f32 / ppp),
        })
    }

    /// 放一张新贴图，顺手把最久没用过的那张挤出去。
    fn store(&mut self, key: u64, entry: Entry) {
        if self.entries.len() >= MAX_ENTRIES && !self.entries.contains_key(&key) {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .map(|(key, _)| *key);
            if let Some(oldest) = oldest {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(key, (entry, self.tick));
    }
}

/// 按宿主配色拼一份 host theme。
///
/// 用内置的编辑器浅色/深色预设打底（它已经给了成对的节点色、边框色、状态色），
/// 只把"底色"换成 egui 的页面色，节点色再从底色往外推一点 —— 否则预设的深色节点色
/// 和页面色几乎一样，方块会看不见。
fn theme(dark: bool, canvas: Color32) -> HostThemeProfile {
    let mut profile = if dark {
        HostThemeProfile::editor_dark()
    } else {
        HostThemeProfile::editor_light()
    };

    // 深色界面往白里推、浅色界面往黑里推
    let step = if dark { 0.08 } else { -0.08 };
    profile.roles.canvas = Some(css(canvas));
    profile.roles.surface = Some(shade(canvas, step));
    profile.roles.surface_alt = Some(shade(canvas, step * 1.75));
    // 连线上的文字（比如 `-->|是|`）需要一块贴住画布的底，否则线会从字里穿过去
    profile.roles.edge_label_background = Some(css(canvas));
    // 默认的根背景是 Mermaid 那套写死的 white
    profile.output.root_background = HostThemeRootBackground::Canvas;

    profile
}

/// 把颜色往白（正数）或黑（负数）方向推，返回 `#rrggbb`。
fn shade(color: Color32, amount: f32) -> String {
    let mix = |channel: u8| {
        let value = f32::from(channel);
        let target = if amount >= 0.0 { 255.0 } else { 0.0 };
        (value + (target - value) * amount.abs())
            .round()
            .clamp(0.0, 255.0) as u8
    };
    css(Color32::from_rgb(
        mix(color.r()),
        mix(color.g()),
        mix(color.b()),
    ))
}

/// 把"影响这张图长什么样"的输入揉成一个哈希。
fn fingerprint(source: &str, dark: bool, ppp: f32, canvas: Color32) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    dark.hash(&mut hasher);
    // 缩放比只留两位小数，免得浮点抖动把缓存全打散
    ((ppp * 100.0).round() as u32).hash(&mut hasher);
    canvas.to_array().hash(&mut hasher);
    hasher.finish()
}

/// 图比可用宽度还宽就等比缩到宽度；否则保持原始大小（不放大，免得糊）。
fn fit(size: egui::Vec2, width: f32) -> egui::Vec2 {
    if size.x <= 0.0 {
        return size;
    }
    size * (width / size.x).min(1.0)
}

/// `Color32` -> `#rrggbb`。merman 的底色参数要的是 CSS 颜色串。
fn css(color: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 切段的第一条不变量：所有段的原文拼起来必须和输入一模一样。
    /// 预览是按段写回源文本的，这条一破就是丢字。
    fn segments(text: &str) -> Vec<Segment<'_>> {
        let segments = split(text);
        let rebuilt: String = segments.iter().copied().map(Segment::raw).collect();
        assert_eq!(rebuilt, text, "拼回去和原文不一致");
        segments
    }

    #[test]
    fn plain_markdown_is_one_segment() {
        let text = "# 标题\n\n正文，没有围栏。\n";
        assert_eq!(segments(text).len(), 1);
        assert!(!has_mermaid(text));
    }

    #[test]
    fn mermaid_fence_splits_into_three() {
        let text = "前\n\n```mermaid\nflowchart TD\n  A --> B\n```\n\n后\n";
        let found = segments(text);
        assert_eq!(found.len(), 3);
        assert_eq!(found[0], Segment::Markdown("前\n\n"));
        assert_eq!(
            found[1],
            Segment::Mermaid {
                raw: "```mermaid\nflowchart TD\n  A --> B\n```\n",
                source: "flowchart TD\n  A --> B",
            }
        );
        assert_eq!(found[2], Segment::Markdown("\n后\n"));
        assert!(has_mermaid(text));
    }

    #[test]
    fn info_string_may_carry_more_words() {
        let found = segments("```mermaid title=x\nA --> B\n```\n");
        assert_eq!(found.len(), 1);
        assert!(has_mermaid("```mermaid title=x\nA --> B\n```\n"));
    }

    /// 普通围栏里的 ```mermaid 只是示例代码，不能被当成图。
    #[test]
    fn fence_inside_a_code_block_is_not_mermaid() {
        let text = "````text\n```mermaid\nA --> B\n```\n````\n";
        let found = segments(text);
        assert_eq!(found.len(), 1);
        assert!(!has_mermaid(text));
    }

    /// 围栏没闭合（打字到一半就是这种状态）：按到文件末尾处理，别把内容吞掉。
    #[test]
    fn unterminated_fence_runs_to_eof() {
        let text = "说明\n\n```mermaid\nflowchart TD\n  A --> B";
        let found = segments(text);
        assert_eq!(found.len(), 2);
        assert_eq!(
            found[1],
            Segment::Mermaid {
                raw: "```mermaid\nflowchart TD\n  A --> B",
                source: "flowchart TD\n  A --> B",
            }
        );
    }

    /// CRLF 文档里源码不能把 `\r` 也带进去。
    #[test]
    fn crlf_line_endings_are_stripped() {
        let found = segments("```mermaid\r\nA --> B\r\n```\r\n");
        assert_eq!(
            found[0],
            Segment::Mermaid {
                raw: "```mermaid\r\nA --> B\r\n```\r\n",
                source: "A --> B",
            }
        );
    }

    /// 波浪号围栏一样算。
    #[test]
    fn tilde_fence_works() {
        assert!(has_mermaid("~~~mermaid\npie\n  \"a\" : 1\n~~~\n"));
    }

    /// 真拿 merman 渲一次。链路很长（host theme -> SVG -> resvg -> PNG -> 纹理），
    /// 这一步顺便钉住两件事：真的画得出来，以及画在宿主给的底色上。
    ///
    /// 后半条不是废话 —— 默认管线会往根节点写 `background-color:white`，
    /// 深色界面里就是一块白板（第一版就是这个毛病）。
    #[test]
    fn renders_on_the_host_canvas() {
        let source = "flowchart LR\n    A[编辑器] --> B{围栏}\n";

        for (dark, canvas) in [
            (false, Color32::from_gray(248)),
            (true, Color32::from_gray(27)),
        ] {
            let png = HeadlessRenderer::new()
                .with_host_theme(&theme(dark, canvas))
                .render_png_sync(
                    source,
                    &RasterOptions::default()
                        .with_scale(1.0)
                        .with_background(css(canvas)),
                )
                .expect("渲染报错")
                .expect("没认出这是一张图");

            let image = image::load_from_memory(&png).expect("PNG 解不开").to_rgba8();
            assert!(image.width() > 8 && image.height() > 8);
            // 角落一定是画布本身，不可能是节点
            let expected = image::Rgba([canvas.r(), canvas.g(), canvas.b(), 255]);
            assert_eq!(image.get_pixel(2, 2), &expected, "dark={dark} 的底色不对");
        }
    }
}
