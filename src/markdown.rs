//! 从 Markdown 源文本派生出来的东西：大纲、字数统计、HTML 导出。
//! 这些都只依赖 `text`，不碰 UI，方便单独测试。

use crate::math;

/// 一条标题。`line` 是 0 基的行号，点击大纲时用它跳转。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub line: usize,
}

/// 扫描 ATX 标题（`# foo`）。
///
/// 会跳过围栏代码块内部的内容 —— 否则一个代码块里写着 `# 注释` 的 shell 片段
/// 就会污染大纲。围栏要求至少 3 个 `` ` `` 或 `~`，且闭合围栏的字符与长度不短于开启的。
pub fn outline(text: &str) -> Vec<Heading> {
    let mut headings = Vec::new();
    // Some((围栏字符, 围栏长度))
    let mut fence: Option<(char, usize)> = None;

    for (line_no, raw) in text.lines().enumerate() {
        let trimmed = raw.trim();

        if let Some(fence_char) = trimmed.chars().next().filter(|c| *c == '`' || *c == '~') {
            let run = trimmed.chars().take_while(|c| *c == fence_char).count();
            if run >= 3 {
                match fence {
                    // 同类且不短于开启围栏 -> 闭合
                    Some((open_char, open_len)) if open_char == fence_char && run >= open_len => {
                        fence = None;
                    }
                    // 不在代码块里 -> 开启
                    None => fence = Some((fence_char, run)),
                    // 代码块内部的三个点，只是普通内容
                    _ => {}
                }
                continue;
            }
        }
        if fence.is_some() {
            continue;
        }

        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if !(1..=6).contains(&hashes) {
            continue;
        }
        let rest = &trimmed[hashes..];
        // CommonMark 要求 # 后面是空格或行尾，`#tag` 不算标题
        if !(rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t')) {
            continue;
        }

        headings.push(Heading {
            level: hashes as u8,
            text: strip_inline(rest.trim().trim_end_matches('#').trim()),
            line: line_no,
        });
    }

    headings
}

/// 去掉行内标记，只留下可读文字，供大纲显示。
fn strip_inline(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            // 强调/删除线/行内代码的标记直接吞掉，连续多个也一并吞掉
            '`' | '*' | '_' | '~' => {
                while chars.peek() == Some(&c) {
                    chars.next();
                }
            }
            '[' => {}
            ']' => {
                // `[文字](链接)` -> 只保留"文字"，丢掉括号里的目标
                if chars.peek() == Some(&'(') {
                    chars.next();
                    let mut depth = 1usize;
                    for c2 in chars.by_ref() {
                        match c2 {
                            '(' => depth += 1,
                            ')' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            // 反斜杠转义：原样保留被转义的字符
            '\\' => {
                if let Some(escaped) = chars.next() {
                    out.push(escaped);
                }
            }
            _ => out.push(c),
        }
    }

    out.trim().to_owned()
}

/// 字数/行数统计。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    pub lines: usize,
    /// 拉丁词数 + 中日韩字符数（CJK 每字算一个词，符合中文习惯）
    pub words: usize,
    pub chars: usize,
    pub cjk: usize,
    /// 估算阅读耗时（秒）
    pub reading_secs: u32,
}

pub fn stats(text: &str) -> Stats {
    let mut s = Stats {
        // "a\n" 在编辑器里是 2 行，所以按 split 数而不是 lines().count()
        lines: text.split('\n').count(),
        chars: text.chars().count(),
        ..Default::default()
    };

    let mut latin_words = 0usize;
    let mut in_word = false;
    for c in text.chars() {
        if is_cjk(c) {
            s.cjk += 1;
            in_word = false;
        } else if c.is_alphanumeric() {
            if !in_word {
                latin_words += 1;
                in_word = true;
            }
        } else if c == '\'' || c == '-' || c == '\u{2019}' {
            // 词内连字符/撇号：`state-of-the-art`、`don't` 算一个词
        } else {
            in_word = false;
        }
    }

    s.words = latin_words + s.cjk;
    // 中文 ~400 字/分钟，英文 ~220 词/分钟
    let minutes = (latin_words as f32 / 220.0) + (s.cjk as f32 / 400.0);
    s.reading_secs = (minutes * 60.0).round() as u32;
    s
}

/// 中日韩字符判定（不含全角标点，免得把标点也算成词）。
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF     // 平假名 / 片假名
        | 0x3400..=0x4DBF   // CJK 扩展 A
        | 0x4E00..=0x9FFF   // CJK 基本区
        | 0xF900..=0xFAFF   // CJK 兼容表意
        | 0xAC00..=0xD7AF   // 谚文音节
        | 0x1100..=0x11FF   // 谚文字母
        | 0x20000..=0x2FA1F // CJK 扩展 B 及以后
    )
}

/// 字节偏移 -> 字符序号。查找结果是字节区间，而光标用的是字符序号。
pub fn byte_to_char(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].chars().count()
}

/// 指定行（0 基）第一个字符的字符序号（不是字节序号）。
/// 用来把"大纲第 N 行"翻译成 TextEdit 光标位置。
pub fn line_char_offset(text: &str, line: usize) -> usize {
    if line == 0 {
        return 0;
    }
    let mut current = 0usize;
    let mut chars = 0usize;
    for c in text.chars() {
        chars += 1;
        if c == '\n' {
            current += 1;
            if current == line {
                return chars;
            }
        }
    }
    chars
}

/// 把 Markdown 转成一份自带样式的完整 HTML。
///
/// `lang` 写进 `<html lang="...">`，跟着界面语言走 —— 它会影响浏览器的
/// 断行规则、拼写检查和屏幕阅读器的发音。
///
/// `render_math` 为真时把 `$...$` / `$$...$$` 烘成内联 SVG：导出文件依然自包含，
/// 不挂 KaTeX、不跑 JS、不需要联网，缩放和打印都是矢量的。关掉则 `$` 原样留在文字里
/// —— 和预览区的「数学公式」开关是同一个意思。
pub fn to_html(text: &str, title: &str, lang: &str, render_math: bool) -> String {
    use pulldown_cmark::{Event, Options, Parser, html};

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_SMART_PUNCTUATION);
    if render_math {
        opts.insert(Options::ENABLE_MATH);
    }

    let mut body = String::with_capacity(text.len() * 3 / 2);
    html::push_html(
        &mut body,
        Parser::new_ext(text, opts).map(|event| match event {
            Event::InlineMath(tex) => math_event(&tex, false),
            Event::DisplayMath(tex) => math_event(&tex, true),
            other => other,
        }),
    );

    format!(
        "<!doctype html>\n\
         <html lang=\"{lang}\">\n\
         <head>\n\
         <meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{title}</title>\n\
         <style>\n{css}\n</style>\n\
         </head>\n\
         <body>\n\
         <main class=\"markdown-body\">\n{body}</main>\n\
         </body>\n\
         </html>\n",
        lang = escape_html(lang),
        title = escape_html(title),
        css = EXPORT_CSS,
    )
}

/// 一条公式要变成什么东西：能排就换成 SVG，排不出来就把源码原样留在正文里。
fn math_event(tex: &str, display: bool) -> pulldown_cmark::Event<'static> {
    use pulldown_cmark::{CowStr, Event};

    match math::to_html(tex, display) {
        Some(svg) => Event::Html(CowStr::from(svg)),
        // `Event::Text` 会被转义，源码里的 `<`、`&` 都不会变成标记
        None => Event::Text(CowStr::from(if display {
            format!("$${tex}$$")
        } else {
            format!("${tex}$")
        })),
    }
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// 导出 HTML 用的样式表：接近 GitHub 观感，并跟随系统深浅色。
const EXPORT_CSS: &str = r#"
:root {
  --fg: #1f2328; --bg: #ffffff; --muted: #59636e;
  --border: #d1d9e0; --code-bg: #f6f8fa; --link: #0969da;
}
@media (prefers-color-scheme: dark) {
  :root {
    --fg: #e6edf3; --bg: #0d1117; --muted: #9198a1;
    --border: #3d444d; --code-bg: #151b23; --link: #4493f8;
  }
}
* { box-sizing: border-box; }
body {
  margin: 0; background: var(--bg); color: var(--fg);
  font: 16px/1.7 -apple-system, BlinkMacSystemFont, "PingFang SC",
        "Hiragino Sans GB", "Microsoft YaHei", "Segoe UI", sans-serif;
}
.markdown-body { max-width: 900px; margin: 0 auto; padding: 40px 24px 96px; }
h1, h2, h3, h4, h5, h6 { line-height: 1.3; margin: 1.6em 0 .6em; font-weight: 600; }
h1 { font-size: 2em; border-bottom: 1px solid var(--border); padding-bottom: .3em; }
h2 { font-size: 1.5em; border-bottom: 1px solid var(--border); padding-bottom: .3em; }
h3 { font-size: 1.25em; }
a { color: var(--link); text-decoration: none; }
a:hover { text-decoration: underline; }
p, ul, ol, blockquote, table, pre { margin: 0 0 1em; }
ul, ol { padding-left: 1.6em; }
li + li { margin-top: .25em; }
li.task-list-item { list-style: none; margin-left: -1.4em; }
code {
  font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
  font-size: .875em; background: var(--code-bg);
  padding: .2em .4em; border-radius: 4px;
}
pre {
  background: var(--code-bg); border: 1px solid var(--border);
  border-radius: 6px; padding: 14px 16px; overflow: auto;
}
pre code { background: none; padding: 0; font-size: .85em; line-height: 1.5; }
blockquote {
  border-left: 4px solid var(--border); color: var(--muted);
  padding: 0 1em; margin-left: 0;
}
table { border-collapse: collapse; display: block; overflow: auto; width: max-content; max-width: 100%; }
th, td { border: 1px solid var(--border); padding: 6px 13px; }
th { font-weight: 600; background: var(--code-bg); }
tr:nth-child(2n) td { background: color-mix(in srgb, var(--code-bg) 50%, transparent); }
img { max-width: 100%; height: auto; }
/* 公式：字形已经是 path，颜色走 currentColor，所以深浅色都不用另存一份 */
.math svg { display: inline-block; vertical-align: baseline; }
.math-inline svg { margin: 0 .12em; }
.math-display { display: block; margin: 1.2em 0; }
.math-display svg { display: block; margin: 0 auto; max-width: 100%; height: auto; }
hr { border: 0; border-top: 1px solid var(--border); margin: 2em 0; }
del { color: var(--muted); }
@media print { .markdown-body { max-width: none; padding: 0; } }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_math_is_baked_into_an_svg() {
        let html = to_html("行内 $x^2$ 完", "t", "zh", true);
        assert!(html.contains(r#"class="math math-inline""#), "{html}");
        assert!(html.contains("<svg"), "公式没有变成 SVG");
        assert!(
            html.contains("currentColor"),
            "颜色应该跟着页面的文字颜色走"
        );
        assert!(
            html.contains(r#"aria-label="x^2""#),
            "公式源码要挂在 aria-label 上给屏读器"
        );
        assert!(!html.contains("$x^2$"), "源码不该再出现在正文里");
        assert!(html.contains("width=\""), "尺寸要写进 svg");
    }

    #[test]
    fn inline_math_carries_its_baseline_offset() {
        // 有深度（下标、分式）的公式要靠 vertical-align 压回文字基线上
        let html = to_html("下标 $x_1$", "t", "zh", true);
        assert!(html.contains(r#"style="vertical-align:-"#), "{html}");
    }

    #[test]
    fn display_math_gets_its_own_centred_block() {
        let html = to_html("$$\n\\frac{a}{b}\n$$", "t", "zh", true);
        assert!(html.contains(r#"class="math math-display""#), "{html}");
    }

    #[test]
    fn broken_math_keeps_its_source_text() {
        let html = to_html("坏公式 $\\frac{1}{$ 结束了", "t", "zh", true);
        assert!(!html.contains("<svg"), "坏公式不该输出 SVG");
        assert!(html.contains("frac"), "源码要留在原地");
    }

    #[test]
    fn math_can_be_left_as_plain_text() {
        let html = to_html("公式 $x^2$ 与 $$y$$", "t", "zh", false);
        assert!(!html.contains("<svg"), "关掉之后不该有 SVG");
        assert!(html.contains("$x^2$"), "关掉之后源码应该原样留着");
    }

    #[test]
    fn explicit_colours_in_math_survive() {
        let html = to_html(r"$\color{red}{x}$", "t", "zh", true);
        assert!(html.contains("<svg"), "{html}");
        assert!(
            html.contains("rgba(255,0,0,1)"),
            "\\color{{red}} 不该被 currentColor 顶掉：{html}"
        );
    }
}
