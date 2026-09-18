//! 文档模型：负责磁盘 I/O、换行符与 BOM 的保真，以及"脏"标记。
//!
//! 这里不产出任何面向用户的句子 —— 错误以 [`DocError`] 的形式返回，由调用方
//! 按当前界面语言渲染成文案，这样模型层不用知道用户在说哪国话。

use std::fs;
use std::path::{Path, PathBuf};

use crate::i18n::{Strings, fill};

/// 「打开 / 另存为」对话框里认的 Markdown 扩展名。
///
/// 放在这里而不是散在对话框代码里，是为了让打开和保存两边永远不会对不上。
pub const MARKDOWN_EXTENSIONS: &[&str] = &["md", "markdown", "mdx", "txt"];

/// 读写失败的原因。
#[derive(Debug)]
pub enum DocError {
    /// 还没有路径，得先「另存为」。
    NoPath,
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl DocError {
    /// 渲染成当前界面语言的一句话。
    pub fn message(&self, s: &Strings) -> String {
        match self {
            Self::NoPath => s.error_no_path.to_owned(),
            Self::Read { path, source } => fill(s.error_read, &[
                ("path", &path.display().to_string()),
                ("err", &source.to_string()),
            ]),
            Self::Write { path, source } => fill(s.error_write, &[
                ("path", &path.display().to_string()),
                ("err", &source.to_string()),
            ]),
        }
    }
}

/// 文件原本使用的换行风格。编辑器内部统一用 `\n`，只在落盘时还原。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
}

impl LineEnding {
    pub fn label(self) -> &'static str {
        match self {
            Self::Lf => "LF",
            Self::CrLf => "CRLF",
        }
    }
}

/// 一个被打开的文本文件。
///
/// `dirty` 是显式标记而不是 `text != baseline` 比较：后者在每帧对大文档做
/// O(n) memcmp 纯属浪费，而这里只需要在编辑/保存这两个时刻维护一个 bool。
#[derive(Debug, Clone)]
pub struct Document {
    pub path: Option<PathBuf>,
    pub text: String,
    pub dirty: bool,
    pub line_ending: LineEnding,
    /// 原文件带头部 UTF-8 BOM，保存时补回去，避免污染 git diff。
    pub had_bom: bool,
    /// 原文件不是合法 UTF-8，内容是用替换字符读取的（保存会丢信息）。
    pub lossy: bool,
}

impl Default for Document {
    fn default() -> Self {
        Self::untitled()
    }
}

impl Document {
    /// 空白新文档。
    pub fn untitled() -> Self {
        Self {
            path: None,
            text: String::new(),
            dirty: false,
            line_ending: LineEnding::Lf,
            had_bom: false,
            lossy: false,
        }
    }

    /// 从磁盘读取。会自动剥离 BOM 并把 CRLF/CR 归一成 LF。
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DocError> {
        let path = path.as_ref().to_path_buf();
        let bytes = fs::read(&path).map_err(|source| DocError::Read {
            path: path.clone(),
            source,
        })?;

        let had_bom = bytes.starts_with(&[0xEF, 0xBB, 0xBF]);
        let body = if had_bom { &bytes[3..] } else { &bytes[..] };

        // 绝大多数 Markdown 都是 UTF-8；不是的话用有损解码兜底，
        // 至少让用户能看能改，并在状态栏把这件事说出来。
        let (mut text, lossy) = match std::str::from_utf8(body) {
            Ok(s) => (s.to_owned(), false),
            Err(_) => (String::from_utf8_lossy(body).into_owned(), true),
        };

        let line_ending = if text.contains("\r\n") {
            LineEnding::CrLf
        } else {
            LineEnding::Lf
        };
        if line_ending == LineEnding::CrLf {
            text = text.replace("\r\n", "\n");
        }
        // 老式 Mac 的裸 CR
        if text.contains('\r') {
            text = text.replace('\r', "\n");
        }

        Ok(Self {
            path: Some(path),
            text,
            dirty: false,
            line_ending,
            had_bom,
            lossy,
        })
    }

    /// 存回原路径。没有路径时报 `DocError::NoPath`，调用方应改用 [`Self::save_as`]。
    pub fn save(&mut self) -> Result<(), DocError> {
        let Some(path) = self.path.clone() else {
            return Err(DocError::NoPath);
        };
        self.write_to(&path)
    }

    /// 存到指定路径，并把文档路径切换到那里。
    pub fn save_as(&mut self, path: impl Into<PathBuf>) -> Result<(), DocError> {
        self.write_to(&path.into())
    }

    fn write_to(&mut self, path: &Path) -> Result<(), DocError> {
        let mut out = String::with_capacity(self.text.len() + 64);
        if self.had_bom {
            out.push('\u{FEFF}');
        }
        match self.line_ending {
            LineEnding::Lf => out.push_str(&self.text),
            // text 内部已经是纯 \n，所以这次替换不会产生 \r\r\n
            LineEnding::CrLf => out.push_str(&self.text.replace('\n', "\r\n")),
        }

        fs::write(path, out.as_bytes()).map_err(|source| DocError::Write {
            path: path.to_path_buf(),
            source,
        })?;

        self.path = Some(path.to_path_buf());
        self.dirty = false;
        // 已经按 UTF-8 重新写回，丢失的信息无法找回但状态要如实反映
        self.lossy = false;
        Ok(())
    }

    /// 文件名；未命名文档给个占位名。
    pub fn display_name(&self, s: &Strings) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| s.untitled_file_name.to_owned())
    }

    pub fn full_path(&self, s: &Strings) -> String {
        self.path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| s.no_path_placeholder.to_owned())
    }

    /// 文件所在目录（预览里解析相对图片路径要用）。
    pub fn dir(&self) -> Option<PathBuf> {
        self.path
            .as_ref()
            .and_then(|p| p.parent())
            .map(Path::to_path_buf)
    }
}
