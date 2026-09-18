//! 查找栏的状态与匹配逻辑（`⌘F`）。

/// 一次匹配：字节区间 `[start, end)`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    pub start: usize,
    pub end: usize,
}

/// 匹配数量上限，防止在大文件里对单字符查询炸出几十万条。
const MAX_MATCHES: usize = 20_000;

#[derive(Debug, Default)]
pub struct FindState {
    pub open: bool,
    pub query: String,
    pub case_sensitive: bool,
    pub matches: Vec<Match>,
    /// 当前匹配在 `matches` 中的下标。
    pub current: usize,
    /// 查询串或正文变了 -> 需要重新扫描。避免每帧都扫全文。
    dirty: bool,
    /// 刚被 `⌘F` 打开，下一帧把键盘焦点交给输入框。
    pub just_opened: bool,
}

impl FindState {
    /// 打开查找栏（保留上一次的查询串）。
    pub fn open(&mut self) {
        self.open = true;
        self.dirty = true;
        self.just_opened = true;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    /// 正文或查询串发生变化，标脏等待下一次 [`Self::rebuild`]。
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// 若需要则重新扫描全部匹配。每帧最多调用一次。
    pub fn rebuild(&mut self, text: &str) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        self.matches.clear();

        if self.query.is_empty() {
            self.current = 0;
            return;
        }

        if self.case_sensitive {
            scan(text, &self.query, &mut self.matches);
        } else {
            // 注意：某些语言（如土耳其语 İ）小写化会改变字节长度，这里按
            // "长度不变" 的常见情况处理，错位只会让高亮偏一个字符，不会 panic。
            let hay = text.to_lowercase();
            let needle = self.query.to_lowercase();
            if hay.len() == text.len() {
                scan(&hay, &needle, &mut self.matches);
            } else {
                scan(text, &self.query, &mut self.matches);
            }
        }

        if self.current >= self.matches.len() {
            self.current = 0;
        }
    }

    /// 跳到上/下一个匹配。
    pub fn step(&mut self, delta: isize) {
        if self.matches.is_empty() {
            return;
        }
        let n = self.matches.len() as isize;
        self.current = (((self.current as isize + delta) % n + n) % n) as usize;
    }

    pub fn current_match(&self) -> Option<Match> {
        self.matches.get(self.current).copied()
    }
}

fn scan(haystack: &str, needle: &str, out: &mut Vec<Match>) {
    if needle.is_empty() {
        return;
    }
    let mut from = 0usize;
    while let Some(offset) = haystack[from..].find(needle) {
        let start = from + offset;
        let end = start + needle.len();
        out.push(Match { start, end });
        if out.len() >= MAX_MATCHES {
            return;
        }
        // 空模式不会出现（上面已挡），所以 end > start，前进是安全的
        from = end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_all_occurrences() {
        let mut f = FindState {
            query: "ab".into(),
            case_sensitive: true,
            dirty: true,
            ..Default::default()
        };
        f.rebuild("abab ab");
        assert_eq!(f.matches.len(), 3);
    }

    #[test]
    fn case_insensitive_matches() {
        let mut f = FindState {
            query: "RUST".into(),
            dirty: true,
            ..Default::default()
        };
        f.rebuild("I love rust and Rust");
        assert_eq!(f.matches.len(), 2);
    }

    #[test]
    fn step_wraps_around() {
        let mut f = FindState {
            query: "x".into(),
            dirty: true,
            ..Default::default()
        };
        f.rebuild("x x x");
        assert_eq!(f.current, 0);
        f.step(1);
        assert_eq!(f.current, 1);
        f.step(-1);
        assert_eq!(f.current, 0);
        f.step(-1);
        assert_eq!(f.current, 2);
    }

    #[test]
    fn empty_query_yields_nothing() {
        let mut f = FindState {
            dirty: true,
            ..Default::default()
        };
        f.rebuild("anything");
        assert!(f.matches.is_empty());
        assert!(f.current_match().is_none());
    }
}
