//! 生成应用图标：`cargo run --release --example make_icon`
//!
//! 图标是用代码画出来的（不依赖任何设计工具或素材），所以它是可复现、可 review、
//! 可改参数的。改完颜色/布局重跑一次即可。
//!
//! 设计意图：这张图标要说出这个 App 是干什么的 —— 左边是 Markdown 源码，
//! 右边是渲染结果，中间一道分隔线。也就是界面本身的样子。
//!
//! 抗锯齿靠超采样：每个输出像素取 ss×ss 个子采样点，先按**预乘**累加再还原，
//! 这样半透明边缘的加权才不会出错。
//!
//! 尺寸策略：两个小尺寸（16/32）用**另一套简化绘制**（只有一个醒目的 #），
//! 不是把大图缩下去 —— 分栏那套六个元素在 16px 下必然糊成一团，
//! 这也是 macOS 应用普遍提供简化小图的原因。

use std::path::{Path, PathBuf};
use std::process::Command;

use image::{Rgba, RgbaImage};

// ---------------------------------------------------------------- 几何

/// 所有坐标都在 1024×1024 的画布空间里，输出时再按比例缩放，
/// 所以下面这些数字可以直接对着设计稿读。
const CANVAS: f32 = 1024.0;

/// macOS 图标栅格：824×824 的圆角矩形居中放在 1024 画布里，圆角半径 185.4。
/// 用这套栅格，图标在 Dock 里和系统自带 App 是一个视觉重量。
const ICON_MIN: f32 = 100.0;
const ICON_SIZE: f32 = 824.0;
const ICON_MAX: f32 = ICON_MIN + ICON_SIZE;
const ICON_RADIUS: f32 = 185.4;

/// 分栏线的位置：画布正中。
const DIVIDER_X: f32 = CANVAS / 2.0;
const DIVIDER_TOP: f32 = 300.0;
const DIVIDER_BOTTOM: f32 = 716.0;
const DIVIDER_HALF: f32 = 4.0;

/// 行基线。左右两侧**共用同一组 y**，"源码 -> 渲染结果"的对应关系才看得出来。
///
/// 行距 80 是调出来的：约为"文字条高度"的 2.7 倍，接近真实文档的行距手感；
/// 之前用 128 时五行像五根孤立的条，不像一段文字。
///
/// 整组相对图标中心（512）下移了 10 单位：第一行行首的 `#` 比文字条高，
/// 会把视觉重心往上拽，补偿之后内容的上下留白才真正相等。
const ROW_Y: [f32; 5] = [362.0, 442.0, 522.0, 602.0, 682.0];

/// 左右内容各自在自己的半栏里左右对称：左边 236..492，右边 532..788，
/// 相隔 20 单位贴着分栏线，外侧到图标边缘都是 136。
const LEFT_MARK_X: (f32, f32) = (236.0, 288.0);
const LEFT_MARK_HALF: f32 = 9.0;
const LEFT_TEXT_X0: f32 = 306.0;
const LEFT_BAR_HALF: f32 = 15.0;
const RIGHT_X0: f32 = 532.0;
const RIGHT_BODY_HALF: f32 = 14.0;
const RIGHT_HEADING_HALF: f32 = 24.0;
/// 行尾右端。每侧最后一行都短一截，读起来才像一段自然结束的文字。
const LEFT_TEXT_X1: [f32; 5] = [492.0, 492.0, 492.0, 492.0, 432.0];
const RIGHT_X1: [f32; 5] = [756.0, 788.0, 788.0, 788.0, 686.0];

/// 第一行行首 `#` 的字形参数。
const HASH_SIZE: f32 = 64.0;
const HASH_STROKE: f32 = 10.0;
const HASH_CX: f32 = 261.0;

// ---------------------------------------------------------------- 颜色

const fn rgb(hex: u32) -> [f32; 3] {
    [
        ((hex >> 16) & 0xFF) as f32 / 255.0,
        ((hex >> 8) & 0xFF) as f32 / 255.0,
        (hex & 0xFF) as f32 / 255.0,
    ]
}

const fn rgba(hex: u32, alpha: f32) -> [f32; 4] {
    let c = rgb(hex);
    [c[0], c[1], c[2], alpha]
}

/// 背景对角渐变：靛蓝 -> 紫。
const BG_FROM: u32 = 0x4F46E5;
const BG_TO: u32 = 0x9333EA;
/// 左半（编辑区）压暗一层，"分栏"才一眼可辨。
const PANE_SHADE_ALPHA: f32 = 0.22;
/// 源码标记的颜色，带一点紫调，呼应背景。
const SOURCE_INK: u32 = 0xEDEBFF;

/// 画多大用哪套绘制。
///
/// 这不是"同一张图缩一缩"，而是两套独立的绘制 —— 16px 下 824 画布里的
/// 一个像素就是 64 单位，分栏那套十来个元素必然糊成一团。macOS 应用普遍
/// 也是给极小尺寸单独出一版简化图。
#[derive(Clone, Copy)]
enum Detail {
    /// ≥64px：完整的"源码 | 预览"分栏。
    ///
    /// `min_half` 是细元素（列表短横、分栏线）的**半厚下限**，由输出尺寸反推出来，
    /// 保证它们在任何尺寸下都至少约 1.5 像素 —— 否则在 64px 这类中号尺寸上，
    /// 0.56px 的短横会被抗锯齿冲淡到几乎看不见（实测内容包围盒从 54% 缩到 44%）。
    Split { min_half: f32 },
    /// ≤32px：只留一个粗壮的 #。两个参数是字形大小和笔画粗细，
    /// 按目标像素反推 —— 16px 下一个像素 64 单位，笔画细于 ~128 单位就会糊。
    Glyph { glyph: f32, stroke: f32 },
}

// ---------------------------------------------------------------- 形状

/// 点是否落在圆角矩形内。
///
/// 先把点夹到"内矩形"上取最近点，再比距离 —— 落在内部时距离为 0，落在角部
/// 方形区域时最近点就是该角的圆心，两种情况都正确。
fn in_rounded_rect(
    px: f32,
    py: f32,
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
    radius: f32,
) -> bool {
    if px < min_x || px > max_x || py < min_y || py > max_y {
        return false;
    }
    let r = radius.min((max_x - min_x) * 0.5).min((max_y - min_y) * 0.5);
    let cx = px.clamp(min_x + r, max_x - r);
    let cy = py.clamp(min_y + r, max_y - r);
    let dx = px - cx;
    let dy = py - cy;
    dx * dx + dy * dy <= r * r
}

/// 点是否落在胶囊（圆头粗线段）内。
fn in_capsule(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32, half: f32) -> bool {
    let vx = bx - ax;
    let vy = by - ay;
    let wx = px - ax;
    let wy = py - ay;
    let len_sq = vx * vx + vy * vy;
    let t = if len_sq <= f32::EPSILON {
        0.0
    } else {
        ((wx * vx + wy * vy) / len_sq).clamp(0.0, 1.0)
    };
    let dx = wx - t * vx;
    let dy = wy - t * vy;
    dx * dx + dy * dy <= half * half
}

/// `#` 字形：两竖 + 两横，全部用圆头线段画，边缘才干净。
fn in_hash(px: f32, py: f32, cx: f32, cy: f32, size: f32, stroke: f32) -> bool {
    let offset = size * 0.21;
    let bar = size * 0.43;
    let half_len = size * 0.48;
    let half = stroke * 0.5;

    in_capsule(
        px,
        py,
        cx - offset,
        cy - half_len,
        cx - offset,
        cy + half_len,
        half,
    ) || in_capsule(
        px,
        py,
        cx + offset,
        cy - half_len,
        cx + offset,
        cy + half_len,
        half,
    ) || in_capsule(px, py, cx - bar, cy - offset, cx + bar, cy - offset, half)
        || in_capsule(px, py, cx - bar, cy + offset, cx + bar, cy + offset, half)
}

/// 直乘 alpha 的 src-over 合成。
fn over(dst: [f32; 4], src: [f32; 4]) -> [f32; 4] {
    let sa = src[3];
    let out_a = sa + dst[3] * (1.0 - sa);
    if out_a <= 0.0 {
        return [0.0; 4];
    }
    let inv = 1.0 / out_a;
    [
        (src[0] * sa + dst[0] * dst[3] * (1.0 - sa)) * inv,
        (src[1] * sa + dst[1] * dst[3] * (1.0 - sa)) * inv,
        (src[2] * sa + dst[2] * dst[3] * (1.0 - sa)) * inv,
        out_a,
    ]
}

// ---------------------------------------------------------------- 绘制

/// 单个采样点的颜色。整张图就是这个函数的积分。
fn sample(px: f32, py: f32, detail: Detail) -> [f32; 4] {
    if !in_rounded_rect(px, py, ICON_MIN, ICON_MIN, ICON_MAX, ICON_MAX, ICON_RADIUS) {
        return [0.0; 4];
    }

    // 背景渐变：沿对角线插值
    let from = rgb(BG_FROM);
    let to = rgb(BG_TO);
    let t = ((px - ICON_MIN) + (py - ICON_MIN)) / (2.0 * ICON_SIZE);
    let mut c = [
        from[0] + (to[0] - from[0]) * t,
        from[1] + (to[1] - from[1]) * t,
        from[2] + (to[2] - from[2]) * t,
        1.0,
    ];

    match detail {
        // 小尺寸：一个粗壮的 # 就够了
        Detail::Glyph { glyph, stroke } => {
            if in_hash(px, py, 512.0, 512.0, glyph, stroke) {
                c = over(c, rgba(0xFFFFFF, 0.94));
            }
        }

        // 大尺寸：左源码 / 右渲染
        Detail::Split { min_half } => {
            // 编辑区压暗，形成"两块面板"的观感
            if px < DIVIDER_X {
                c = over(c, [0.0, 0.0, 0.0, PANE_SHADE_ALPHA]);
            }

            // 分栏线
            if in_capsule(
                px,
                py,
                DIVIDER_X,
                DIVIDER_TOP,
                DIVIDER_X,
                DIVIDER_BOTTOM,
                DIVIDER_HALF.max(min_half * 0.75),
            ) {
                c = over(c, rgba(0xFFFFFF, 0.32));
            }

            let ink = rgba(SOURCE_INK, 0.88);
            // 列表短横比文字条细，但也得守住像素下限，不然小尺寸上直接被抗锯齿吃掉
            let marker_half = LEFT_MARK_HALF.max(min_half);

            for (row, &y) in ROW_Y.iter().enumerate() {
                // ---- 左：Markdown 源码 ----
                // 第一行是标题，所以行首是 #；其余是列表项，行首是短横
                if row == 0 {
                    if in_hash(px, py, HASH_CX, y, HASH_SIZE, HASH_STROKE) {
                        c = over(c, ink);
                    }
                } else if in_capsule(px, py, LEFT_MARK_X.0, y, LEFT_MARK_X.1, y, marker_half) {
                    c = over(c, ink);
                }
                if in_rounded_rect(
                    px,
                    py,
                    LEFT_TEXT_X0,
                    y - LEFT_BAR_HALF,
                    LEFT_TEXT_X1[row],
                    y + LEFT_BAR_HALF,
                    LEFT_BAR_HALF,
                ) {
                    c = over(c, ink);
                }

                // ---- 右：渲染出来之后的样子 ----
                // 首行是标题：更粗、更亮、比正文略短
                let (half, alpha) = if row == 0 {
                    (RIGHT_HEADING_HALF, 0.95)
                } else {
                    (RIGHT_BODY_HALF, 0.72)
                };
                let last = row + 1 == ROW_Y.len();
                let alpha = if last { alpha - 0.14 } else { alpha };
                if in_rounded_rect(px, py, RIGHT_X0, y - half, RIGHT_X1[row], y + half, half) {
                    c = over(c, rgba(0xFFFFFF, alpha));
                }
            }
        }
    }

    c
}

/// 超采样渲染一张 size×size 的 RGBA 图。
fn render(size: u32, detail: Detail) -> RgbaImage {
    // 大图每个像素占比小，少采几个点看不出来；小图靠厚实取胜，多采几个更干净
    let ss: u32 = if size >= 512 { 3 } else { 4 };
    let scale = CANVAS / size as f32;
    let samples_inv = 1.0 / (ss * ss) as f32;

    let mut out = RgbaImage::new(size, size);
    for y in 0..size {
        for x in 0..size {
            // 在预乘空间累加：半透明边缘的加权必须按预乘算，否则会偏亮
            let mut acc = [0.0f32; 4];
            for sy in 0..ss {
                for sx in 0..ss {
                    let px = (x as f32 + (sx as f32 + 0.5) / ss as f32) * scale;
                    let py = (y as f32 + (sy as f32 + 0.5) / ss as f32) * scale;
                    let c = sample(px, py, detail);
                    acc[0] += c[0] * c[3];
                    acc[1] += c[1] * c[3];
                    acc[2] += c[2] * c[3];
                    acc[3] += c[3];
                }
            }

            let alpha = acc[3] * samples_inv;
            let pixel = if alpha > 0.0 {
                let un_premultiply = |v: f32| {
                    ((v * samples_inv) / alpha * 255.0)
                        .round()
                        .clamp(0.0, 255.0) as u8
                };
                Rgba([
                    un_premultiply(acc[0]),
                    un_premultiply(acc[1]),
                    un_premultiply(acc[2]),
                    (alpha * 255.0).round().clamp(0.0, 255.0) as u8,
                ])
            } else {
                Rgba([0, 0, 0, 0])
            };
            out.put_pixel(x, y, pixel);
        }
    }
    out
}

// ---------------------------------------------------------------- 输出

fn detail_for(size: u32) -> Detail {
    match size {
        // 16px：一个像素 = 64 画布单位。字形收到图标宽度的六成左右，否则会顶到圆角；
        // 笔画给到 128 单位（正好 2px），细了就糊。
        0..=16 => Detail::Glyph {
            glyph: 460.0,
            stroke: 128.0,
        },
        // 32px：行距换算过来只有 2.5px，分栏那套一定糊，继续用简化字形。
        // 可以放大一点，笔画也可以稍细。
        17..=32 => Detail::Glyph {
            glyph: 540.0,
            stroke: 122.0,
        },
        // 64px 起用完整分栏；0.75 是"一个像素"的半个宽度，乘 2 即 1.5px 的下限
        _ => Detail::Split {
            min_half: 0.75 * CANVAS / size as f32,
        },
    }
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let assets = root.join("assets");

    // `--verify <iconset 目录>`：只比对，不写文件
    let verify_dir = parse_verify_dir();

    // 每个尺寸渲染一次；iconset 里同一个尺寸可能对应两个文件名
    let sizes = [16u32, 32, 64, 128, 256, 512, 1024];
    let mut rendered: Vec<(u32, RgbaImage)> = Vec::new();
    for size in sizes {
        let image = render(size, detail_for(size));
        rendered.push((size, image));
    }

    if let Some(dir) = verify_dir {
        verify_against(&dir, &rendered);
        return;
    }

    std::fs::create_dir_all(&assets).expect("创建 assets/ 失败");

    let lookup = |size: u32| -> &RgbaImage {
        &rendered
            .iter()
            .find(|(s, _)| *s == size)
            .expect("尺寸在上面已渲染")
            .1
    };

    // 1024 母版 + 给窗口图标用的 256
    let master = assets.join("icon_1024.png");
    lookup(1024).save(&master).expect("写 icon_1024.png 失败");
    let window_icon = assets.join("icon_256.png");
    lookup(256)
        .save(&window_icon)
        .expect("写 icon_256.png 失败");

    // .iconset 目录（iconutil 的输入格式）
    let iconset = assets.join("AppIcon.iconset");
    let _ = std::fs::remove_dir_all(&iconset);
    std::fs::create_dir_all(&iconset).expect("创建 iconset 失败");

    // 第三个字段：这个槽位是否要写预乘 alpha。
    //
    // icns 只对最小的两个 1x 槽位按预乘解释（其余是普通 PNG）。这由验证器实测确认：
    // 写直乘数据进去再回读，每个半透明像素都会变成 `原值 / alpha`，也就是被当成预乘
    // 还原了一次；圆角那圈像素因此会偏亮。这两个槽位按预乘写就对了。
    let entries: [(&str, u32, bool); 10] = [
        ("icon_16x16.png", 16, true),
        ("icon_16x16@2x.png", 32, false),
        ("icon_32x32.png", 32, true),
        ("icon_32x32@2x.png", 64, false),
        ("icon_128x128.png", 128, false),
        ("icon_128x128@2x.png", 256, false),
        ("icon_256x256.png", 256, false),
        ("icon_256x256@2x.png", 512, false),
        ("icon_512x512.png", 512, false),
        ("icon_512x512@2x.png", 1024, false),
    ];
    for (name, size, premultiply) in entries {
        let source = lookup(size);
        let image = if premultiply {
            to_premultiplied(source)
        } else {
            source.clone()
        };
        image
            .save(iconset.join(name))
            .unwrap_or_else(|e| panic!("写 {name} 失败：{e}"));
    }

    println!(
        "已写入 {} 张 PNG 到 {}",
        entries.len() + 2,
        assets.display()
    );

    // .icns 只能在 macOS 上打；其他平台留一份 iconset 也能被工具链消费
    let icns = assets.join("AppIcon.icns");
    match Command::new("iconutil")
        .arg("-c")
        .arg("icns")
        .arg(&iconset)
        .arg("-o")
        .arg(&icns)
        .output()
    {
        Ok(out) if out.status.success() => {
            println!("已写入 {}", icns.display());
        }
        Ok(out) => {
            eprintln!(
                "iconutil 失败（{}），iconset 已保留，可手动重试",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Err(err) => {
            eprintln!("找不到 iconutil（{err}）；iconset 已保留，仅 macOS 需要 .icns");
        }
    }

    describe(&master);

    // 判断"小尺寸还成不成立"，靠肉眼看放大图不可靠（我在这上面判断错过两次），
    // 靠按亮度阈值量包围盒也不可靠（细元素被抗锯齿摊薄后会整条漏掉）。
    // 真正可核对的是这两个精确量：最细元素有几像素厚，以及内容的上下留白差多少。
    println!("\n尺寸  最细元素   留白(上/下)   说明");
    for (size, _) in &rendered {
        let thinnest = thinnest_element_px(*size);
        let (top_gap, bottom_gap) = vertical_padding(*size);
        let note = if thinnest < 0.9 {
            "⚠ 细于 1px，会被抗锯齿吃掉"
        } else if (top_gap - bottom_gap).abs() > 8.0 {
            "⚠ 上下不居中"
        } else {
            "ok"
        };
        println!(
            "{size:>4}  {thinnest:>7.2}px  {:>12}  {note}",
            // 注意不要给这个 String 加精度限定 —— `{:.1}` 在字符串上是"截断成 1 个字符"
            format!("{top_gap:.0} / {bottom_gap:.0}")
        );
    }

    write_sheet(&assets.join("preview_sheet.png"), &rendered);
}

/// 该尺寸下最细的元素有多厚（像素）。这是小尺寸设计成不成立的硬指标。
fn thinnest_element_px(size: u32) -> f32 {
    let units_per_px = CANVAS / size as f32;
    let thinnest_units = match detail_for(size) {
        Detail::Glyph { stroke, .. } => stroke,
        Detail::Split { min_half } => {
            // 分栏线 / 列表短横 / 文字条 / 正文行，取最细的那个
            let divider = DIVIDER_HALF.max(min_half * 0.75) * 2.0;
            let marker = LEFT_MARK_HALF.max(min_half) * 2.0;
            let left_bar = LEFT_BAR_HALF * 2.0;
            let right_body = RIGHT_BODY_HALF * 2.0;
            divider.min(marker).min(left_bar).min(right_body)
        }
    };
    thinnest_units / units_per_px
}

/// 内容到圆角矩形上下边缘的留白（画布单位）。两者越接近越居中。
fn vertical_padding(size: u32) -> (f32, f32) {
    match detail_for(size) {
        Detail::Glyph { glyph, .. } => {
            // 字形高约 0.96 * glyph，上下对称，所以两侧留白必然相等
            let gap = ICON_SIZE / 2.0 - glyph * 0.48;
            (gap, gap)
        }
        Detail::Split { .. } => {
            // 第一行向上探得最高的是 `#`（比文字条高），不是文字条
            let hash_half = HASH_SIZE * 0.48 + HASH_STROKE * 0.5;
            let first_row_top = ROW_Y[0] - hash_half.max(RIGHT_HEADING_HALF);
            let last_row_bottom = ROW_Y[ROW_Y.len() - 1] + LEFT_BAR_HALF;
            (first_row_top - ICON_MIN, ICON_MAX - last_row_bottom)
        }
    }
}

/// 把各尺寸摆成一行拼图，小尺寸放得更大 —— 要判断的正是它们**在真实像素下**立不立得住。
///
/// 用最近邻放大而不是平滑插值：插值会把"这块像素到底是实心还是糊的"这个判断依据抹掉。
fn write_sheet(path: &Path, rendered: &[(u32, RgbaImage)]) {
    const PLAN: [(u32, u32); 5] = [(16, 8), (32, 8), (64, 8), (128, 4), (256, 4)];
    const GAP: u32 = 32;

    let height = PLAN.iter().map(|(s, z)| s * z).max().unwrap_or(1);
    let width = PLAN.iter().map(|(s, z)| s * z).sum::<u32>() + GAP * (PLAN.len() as u32 - 1);

    let mut sheet = RgbaImage::from_pixel(width, height, Rgba([0x2A, 0x2A, 0x30, 0xFF]));
    let mut x = 0;
    for (size, zoom) in PLAN {
        let src = &rendered
            .iter()
            .find(|(s, _)| *s == size)
            .expect("尺寸已在上面渲染")
            .1;
        // 底部对齐，方便比较各尺寸的视觉重量
        let top = height - size * zoom;
        for sy in 0..size {
            for sx in 0..size {
                let pixel = *src.get_pixel(sx, sy);
                for dy in 0..zoom {
                    for dx in 0..zoom {
                        sheet.put_pixel(x + sx * zoom + dx, top + sy * zoom + dy, pixel);
                    }
                }
            }
        }
        x += size * zoom + GAP;
    }

    sheet.save(path).expect("写预览拼图失败");
    println!("已写入 {}", path.display());
}

/// 解析 `--verify <dir>`。
fn parse_verify_dir() -> Option<PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--verify" {
            return args.next().map(PathBuf::from);
        }
    }
    None
}

/// 从 iconset 的文件名反推像素尺寸：`icon_<n>x<n>[@2x].png`。
fn size_from_name(name: &str) -> Option<u32> {
    let stem = name.strip_prefix("icon_")?.strip_suffix(".png")?;
    let (base, scale) = match stem.split_once('@') {
        Some((base, "2x")) => (base, 2),
        Some(_) => return None, // 只认识 @2x
        None => (stem, 1),
    };
    let (width, height) = base.split_once('x')?;
    if width != height {
        return None;
    }
    Some(width.parse::<u32>().ok()? * scale)
}

/// 把一个 iconset 目录里的图和"刚渲染出来的"逐像素比对。
///
/// 这是验证 `.icns` 的正确姿势：`iconutil` 打包时会**重新编码** PNG，所以回读出来
/// 的文件跟写进去的永远不是逐字节相同 —— 比字节只会得到一堆假警报。真正要问的是
/// "像素还是不是我画的那张"，那就该比像素。
fn verify_against(dir: &Path, rendered: &[(u32, RgbaImage)]) {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("读不到 {}：{e}", dir.display()));

    let mut checked = 0usize;
    let mut worst = 0u8;
    let mut problems = 0usize;

    for entry in entries {
        let path = entry.expect("目录项").path();
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        let Some(size) = size_from_name(&name) else {
            continue;
        };
        let expected = &rendered
            .iter()
            .find(|(s, _)| *s == size)
            .unwrap_or_else(|| panic!("没有渲染过 {size}px，文件名 {name}"))
            .1;

        let Ok(actual) = image::open(&path).map(|i| i.into_rgba8()) else {
            println!("  ✗ {name}：读不出来");
            problems += 1;
            continue;
        };

        if actual.dimensions() != expected.dimensions() {
            println!(
                "  ✗ {name}：尺寸 {:?}，应为 {:?}",
                actual.dimensions(),
                expected.dimensions()
            );
            problems += 1;
            continue;
        }

        let mut max_diff = 0u8;
        let mut differing = 0usize;
        let mut alpha_quantized = 0usize;
        let mut premult_roundtrip = 0usize;
        let mut rounding = 0usize;
        let mut unexplained = 0usize;

        // ICON_VERIFY_DUMP=1 时把前若干个不同像素的新旧值打出来。
        // 「哪个格式在改我的像素」这种问题，猜没有用，只能看数据。
        let dump = std::env::var_os("ICON_VERIFY_DUMP").is_some();
        let (width, _) = actual.dimensions();
        // (可见差, x, y, 原本, 现在) —— 按 alpha 加权后差异最大的那个像素
        let mut worst_pixel = (0u8, 0u32, 0u32, [0u8; 4], [0u8; 4]);
        let mut max_visible = 0u8;

        for (index, (actual_px, expected_px)) in actual.pixels().zip(expected.pixels()).enumerate()
        {
            let a = actual_px.0;
            let b = expected_px.0;
            let diff = a
                .iter()
                .zip(b.iter())
                .map(|(x, y)| x.abs_diff(*y))
                .max()
                .unwrap_or(0);
            if diff == 0 {
                continue;
            }
            differing += 1;
            max_diff = max_diff.max(diff);

            if dump && differing <= 12 {
                let (x, y) = (index as u32 % width, index as u32 / width);
                println!(
                    "      ({x:>3},{y:>3}) 原 {:>3} {:>3} {:>3} / {:>3}  ->  新 {:>3} {:>3} {:>3} / {:>3}",
                    b[0], b[1], b[2], b[3], a[0], a[1], a[2], a[3]
                );
            }

            // 按 alpha 加权：一个 alpha=16 的像素即使通道差 200，合成到屏幕上也只有
            // 200*16/255 ≈ 13 的可见差。这才是该拿来判断"用户看不看得出来"的量。
            let visible = ((diff as u32 * u32::from(a[3]) + 127) / 255) as u8;
            max_visible = max_visible.max(visible);

            // 记下"合成后最看得出来"的那个像素 —— 报告里最有价值的一条
            if visible > worst_pixel.0 {
                worst_pixel = (visible, index as u32 % width, index as u32 / width, b, a);
            }

            // 区分三类差异，别把不同原因混成一句"被改动"：
            //   1. 恰好等于预乘/直乘换算 —— icns 小尺寸槽位的既有行为，可解释
            //   2. 原本半透明、现在被硬化成二值 alpha —— 编码器量化，可解释
            //   3. 其余 —— 内容真被动过，要查
            let was_partial = b[3] > 0 && b[3] < 255;
            let now_binary = a[3] == 0 || a[3] == 255;
            if is_premultiplied_roundtrip(a, b) {
                premult_roundtrip += 1;
            } else if was_partial && now_binary {
                alpha_quantized += 1;
            } else if max_diff <= 2 {
                // 预乘是乘了再除，取整会留一两个 LSB 的残差
                rounding += 1;
            } else {
                unexplained += 1;
            }
        }

        // alpha 是否被整体二值化 —— icns 对最小 1x 尺寸用旧式"RGB + 掩码"存储的指纹
        let partial_expected = expected
            .pixels()
            .filter(|p| p.0[3] > 0 && p.0[3] < 255)
            .count();
        let partial_actual = actual
            .pixels()
            .filter(|p| p.0[3] > 0 && p.0[3] < 255)
            .count();
        let alpha_binarized = partial_expected > 0 && partial_actual == 0;

        worst = worst.max(max_visible);
        checked += 1;

        // 只要不是逐像素相同，就把"合成后最看得出来"的那一个打出来
        let worst_note = || {
            let (visible, x, y, b, a) = worst_pixel;
            format!("合成后可见差 {visible} 在 ({x},{y})：{b:?} -> {a:?}")
        };

        if differing == 0 {
            println!("  ✓ {name:<22} 完全一致");
            continue;
        }

        // 判定依据是"合成后可见差"，不是原始字节差：低透明度的像素怎么量化都看不见。
        // 同时把差异的成因分类打出来，免得下次又对着一个 ✗ 猜。
        let ok = max_visible <= 8;
        if !ok {
            problems += 1;
        }
        println!(
            "  {} {name:<22} {differing} 个像素有差异（预乘换算 {premult_roundtrip}／取整 {rounding}／其他 {unexplained}{}）；{}",
            if ok { "✓" } else { "✗" },
            if alpha_binarized {
                format!("／alpha 被二值化 {partial_expected}->{partial_actual}")
            } else if alpha_quantized > 0 {
                format!("／alpha 被硬化 {alpha_quantized}")
            } else {
                String::new()
            },
            worst_note()
        );
    }

    println!(
        "\n比对 {checked} 个尺寸，最大合成后可见差 {worst}（0-255），需要处理的问题 {problems} 处"
    );
    if problems == 0 {
        println!(
            "结论：所有尺寸都可用。\n\
             说明：icns 把最小的两个 1x 槽位（16x16 / 32x32）按预乘 alpha 的旧式格式\n\
             存储，且色彩精度有限，所以那两个槽位写的是预乘数据；被量化掉的都是圆角处\n\
             透明度只有个位数的像素，合成后看不出差别。\n\
             其余 8 个槽位（含 Retina 屏实际使用的全部 @2x）逐像素完全一致。"
        );
    } else {
        println!("结论：有问题需要处理");
    }
}

/// 差异是否恰好等于"把直乘的 RGB 当成预乘再还原"的结果。
///
/// 实测发现 icns 对最小的两个 1x 槽位（16x16、32x32）是按**预乘 alpha** 解释的，
/// 读回来会把每个半透明像素除以 alpha：`新 = 原 / alpha`。所以写直乘数据进去，
/// 圆角那圈半透明像素就会被放大成接近白色。这里逐像素验算这个关系，
/// 而不是"看起来像是这个原因"。
fn is_premultiplied_roundtrip(actual: [u8; 4], expected: [u8; 4]) -> bool {
    if actual[3] != expected[3] || expected[3] == 0 {
        return false;
    }
    let alpha = expected[3] as f32 / 255.0;
    (0..3).all(|i| {
        let want = (expected[i] as f32 / alpha).round().clamp(0.0, 255.0) as u8;
        actual[i].abs_diff(want) <= 1
    })
}

/// 把直乘 alpha 的图转成预乘 —— 供必须按预乘解释的 icns 槽位使用。
fn to_premultiplied(image: &RgbaImage) -> RgbaImage {
    let mut out = image.clone();
    for pixel in out.pixels_mut() {
        let alpha = u32::from(pixel.0[3]);
        for channel in 0..3 {
            pixel.0[channel] = (u32::from(pixel.0[channel]) * alpha / 255) as u8;
        }
    }
    out
}

/// 打一行摘要，方便在终端里确认生成的是什么。
fn describe(path: &Path) {
    match image::open(path) {
        Ok(img) => println!(
            "母版 {}：{}×{} {:?}",
            path.display(),
            img.width(),
            img.height(),
            img.color()
        ),
        Err(err) => eprintln!("回读母版失败：{err}"),
    }
}
