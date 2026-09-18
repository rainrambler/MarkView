//! 中文字体加载。
//!
//! egui 自带字体只有拉丁字形，中文会渲染成空白方块，所以必须挂一个系统中文字体
//! 做 fallback。epaint 在字库解析失败时是直接 `panic!` 的（见 epaint/src/text/fonts.rs），
//! 因此这里先用 skrifa（epaint 内部用的同一个解析器）**试解析**，确认可用才交给 egui。

use std::borrow::Cow;
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

    let picked = match find_usable_cjk_font() {
        Some(found) => {
            const NAME: &str = "cjk_fallback";
            defs.font_data.insert(
                NAME.to_owned(),
                Arc::new(egui::FontData {
                    font: Cow::Owned(found.bytes),
                    index: found.face_index,
                    tweak: Default::default(),
                }),
            );
            // 追加为兜底：拉丁字形仍然走 egui 自带字体，中文缺字时落到这里。
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                defs.families
                    .entry(family)
                    .or_default()
                    .push(NAME.to_owned());
            }
            Some(found.path)
        }
        None => None,
    };

    ctx.set_fonts(defs);
    picked
}

struct FoundFont {
    path: &'static str,
    face_index: u32,
    bytes: Vec<u8>,
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
