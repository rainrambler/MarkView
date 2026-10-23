//! 中文字体加载。
//!
//! egui 自带字体只有拉丁字形，中文会渲染成空白方块，所以必须挂一个系统中文字体
//! 做 fallback。epaint 在字库解析失败时是直接 `panic!` 的（见 epaint/src/text/fonts.rs），
//! 因此这里先用 skrifa（epaint 内部用的同一个解析器）**试解析**，确认可用才交给 egui。

use std::sync::Arc;

/// 候选字体路径，按优先级排列。跨平台的常见位置都列上，第一个能解析的胜出。
const CANDIDATES: &[&str] = &[
    // macOS
    "/System/Library/Fonts/PingFang.ttc",
    "/System/Library/Fonts/Hiragino Sans GB.ttc",
    "/System/Library/Fonts/STHeiti Medium.ttc",
    "/System/Library/Fonts/STHeiti Light.ttc",
    "/System/Library/Fonts/Supplemental/Songti.ttc",
    "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
    // Linux
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/opentype/noto/NotoSerifCJK-Regular.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
    "/usr/share/fonts/truetype/arphic/uming.ttc",
    // Windows
    "C:/Windows/Fonts/msyh.ttc",
    "C:/Windows/Fonts/simhei.ttf",
    "C:/Windows/Fonts/simsun.ttc",
];

/// 装好字体后返回实际用到的字体路径，方便日志里说明情况。
pub fn install(ctx: &egui::Context) -> Option<&'static str> {
    let mut defs = egui::FontDefinitions::default();

    let Some(found) = find_usable_cjk_font() else {
        ctx.set_fonts(defs);
        return None;
    };

    const NAME: &str = "cjk_fallback";
    let tweak = egui::FontTweak {
        y_offset_factor: baseline_offset_factor(&defs, &found),
        ..Default::default()
    };
    defs.font_data.insert(
        NAME.to_owned(),
        Arc::new(egui::FontData {
            font: Arc::new(found.bytes),
            index: found.face_index,
            tweak,
        }),
    );
    // 追加为兜底：拉丁字形仍然走 egui 自带字体，中文缺字时落到这里。
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        defs.families
            .entry(family)
            .or_default()
            .push(NAME.to_owned());
    }

    ctx.set_fonts(defs);
    Some(found.path)
}

struct FoundFont {
    path: &'static str,
    face_index: u32,
    bytes: Vec<u8>,
}

/// 字体的垂直度量，单位是 em，与字号无关。
struct EmMetrics {
    ascent: f32,
    /// ascent - descent + leading，也就是 epaint 眼里的行高。
    row_height: f32,
}

fn em_metrics(bytes: &[u8], face_index: u32) -> Option<EmMetrics> {
    use skrifa::MetadataProvider as _;

    let font = skrifa::FontRef::from_index(bytes, face_index).ok()?;
    // epaint 也是这么取度量的（同一个 skrifa），照着算才能补得准。
    let metrics = font.metrics(
        skrifa::instance::Size::unscaled(),
        skrifa::instance::LocationRef::default(),
    );
    // `unscaled` 给的是字体设计单位，除以 units_per_em 换成 em
    let em = metrics.units_per_em as f32;
    if em == 0.0 {
        return None;
    }
    Some(EmMetrics {
        ascent: metrics.ascent / em,
        row_height: (metrics.ascent - metrics.descent + metrics.leading) / em,
    })
}

/// 让中文和拉丁文字在同一行里共用一个基线。
///
/// epaint 排一行里来自不同字体的字形时，不是直接按基线摆，而是"本字体的 ascent
/// + 行高差的一半"（见 epaint 的 `galley_from_rows`）：
///
/// ```text
/// y = 本字体 ascent + 0.5 * (主字体行高 - 本字体行高)
/// ```
///
/// 中文字体的 ascent/行高和拉丁字体差得很远（Hiragino Sans GB 的行高是 1.5em，
/// Ubuntu Light 只有 1.15em），于是中文会比同一行的英文、数字高出一截。
/// `FontTweak::y_offset_factor` 只平移字形、不参与排版，正好用来补上这个差值。
fn baseline_offset_factor(defs: &egui::FontDefinitions, found: &FoundFont) -> f32 {
    let Some(cjk) = em_metrics(&found.bytes, found.face_index) else {
        return 0.0;
    };

    // 正文走 Proportional、代码走 Monospace，两个主字体的度量略有差别。
    // 取平均让两边都对齐到亚像素，省得把同一份字体数据再塞进第二份 FontData。
    let mut sum = 0.0;
    let mut count = 0;
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let primary = defs
            .families
            .get(&family)
            .and_then(|names| names.first())
            .and_then(|name| defs.font_data.get(name))
            .and_then(|data| em_metrics(data.bytes(), data.index));
        if let Some(primary) = primary {
            sum += primary.ascent - (cjk.ascent + 0.5 * (primary.row_height - cjk.row_height));
            count += 1;
        }
    }

    if count == 0 { 0.0 } else { sum / count as f32 }
}

fn find_usable_cjk_font() -> Option<FoundFont> {
    for path in CANDIDATES {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };

        // .ttc 是字体集合，一个文件里有多套字形，逐个面探测。
        let faces = if bytes.starts_with(b"ttcf") { 8 } else { 1 };
        for face_index in 0..faces {
            if skrifa::FontRef::from_index(&bytes, face_index).is_ok() {
                return Some(FoundFont {
                    path,
                    face_index,
                    bytes,
                });
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一行里拉丁字符和中文的**视觉**基线。epaint 的 `pos.y` 不含 FontTweak 的平移，
    /// 这里按它自己的公式补回去（`round_ui` 是 1/32 点）。
    fn visual_baselines(
        ctx: &egui::Context,
        family: egui::FontFamily,
        size: f32,
        y_offset_factor: f32,
    ) -> Option<(f32, f32)> {
        let shift = ((size * y_offset_factor) / (1.0 / 32.0)).round() * (1.0 / 32.0);
        let galley = ctx.fonts_mut(|fonts| {
            fonts.layout_no_wrap(
                "a中".to_owned(),
                egui::FontId::new(size, family),
                egui::Color32::BLACK,
            )
        });
        let row = galley.rows.first()?;
        let latin = row.row.glyphs.iter().find(|g| g.chr == 'a')?.pos.y;
        let cjk = row.row.glyphs.iter().find(|g| g.chr == '中')?.pos.y;
        Some((latin, cjk + shift))
    }

    #[test]
    fn mixed_cjk_and_latin_share_a_baseline() {
        // 机器上没装中文字体的话，这条断言无从谈起
        let Some(found) = find_usable_cjk_font() else {
            return;
        };

        let factor = baseline_offset_factor(&egui::FontDefinitions::default(), &found);
        let ctx = egui::Context::default();
        install(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();

        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            for size in [12.0, 16.0, 24.0] {
                let (latin, cjk) = visual_baselines(&ctx, family.clone(), size, factor).unwrap();
                let diff = (cjk - latin).abs();
                // 剩下的差值来自 epaint 把行内坐标取整到整像素，亚像素级即可
                assert!(
                    diff <= 1.0,
                    "{family:?} @ {size}px: 中文基线与拉丁基线差了 {diff}px"
                );
            }
        }
    }
}
