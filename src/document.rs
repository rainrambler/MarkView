//! 文档模型：负责磁盘 I/O、换行符与 BOM 的保真，以及"脏"标记。

use std::fs;
use std::path::{Path, PathBuf};

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
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref().to_path_buf();
        let bytes = fs::read(&path).map_err(|e| format!("无法读取 {}：{e}", path.display()))?;

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

    /// 存回原路径。没有路径时报错，调用方应改用 [`Self::save_as`]。
    pub fn save(&mut self) -> Result<(), String> {
        let Some(path) = self.path.clone() else {
            return Err("这个文档还没有路径，请先用「另存为」".to_owned());
        };
        self.write_to(&path)
    }

    /// 存到指定路径，并把文档路径切换到那里。
    pub fn save_as(&mut self, path: impl Into<PathBuf>) -> Result<(), String> {
        self.write_to(&path.into())
    }

    fn write_to(&mut self, path: &Path) -> Result<(), String> {
        let mut out = String::with_capacity(self.text.len() + 64);
        if self.had_bom {
            out.push('\u{FEFF}');
        }
        match self.line_ending {
            LineEnding::Lf => out.push_str(&self.text),
            // text 内部已经是纯 \n，所以这次替换不会产生 \r\r\n
            LineEnding::CrLf => out.push_str(&self.text.replace('\n', "\r\n")),
        }

        fs::write(path, out.as_bytes()).map_err(|e| format!("无法写入 {}：{e}", path.display()))?;

        self.path = Some(path.to_path_buf());
        self.dirty = false;
        // 已经按 UTF-8 重新写回，丢失的信息无法找回但状态要如实反映
        self.lossy = false;
        Ok(())
    }

    /// 文件名；未命名文档给个占位名。
    pub fn display_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "未命名.md".to_owned())
    }

    pub fn full_path(&self) -> String {
        self.path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "（尚未保存）".to_owned())
    }

    /// 文件所在目录（预览里解析相对图片路径要用）。
    pub fn dir(&self) -> Option<PathBuf> {
        self.path
            .as_ref()
            .and_then(|p| p.parent())
            .map(Path::to_path_buf)
    }
}
