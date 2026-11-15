use super::*;

impl ThzipApp {
    pub fn filtered_entries(&self) -> Vec<usize> {
        let needle = self.search.trim().to_lowercase();
        let mut rows: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| search_matches(&needle, &entry.display))
            .map(|(index, _)| index)
            .collect();

        rows.sort_by(|&a, &b| {
            let left = &self.entries[a];
            let right = &self.entries[b];
            let order = match self.sort_key {
                SortKey::Name => natural_cmp(&left.display, &right.display),
                SortKey::Size => left.size.cmp(&right.size),
                SortKey::Packed => left.packed.cmp(&right.packed),
                SortKey::Ratio => left
                    .ratio()
                    .partial_cmp(&right.ratio())
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortKey::Method => left.method.label().cmp(right.method.label()),
                SortKey::Time => left.mtime.cmp(&right.mtime),
            };
            if self.sort_asc {
                order
            } else {
                order.reverse()
            }
            .then_with(|| natural_cmp(&left.display, &right.display))
        });
        rows
    }

    pub fn select_all_filtered(&mut self) {
        for index in self.filtered_entries() {
            self.selected.insert(index);
        }
    }
}

/// 自然排序：`file2` 排在 `file10` 前；数字段按数值比较。
pub(crate) fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let left: Vec<char> = a.chars().collect();
    let right: Vec<char> = b.chars().collect();
    let (mut i, mut j) = (0usize, 0usize);

    while i < left.len() && j < right.len() {
        if left[i].is_ascii_digit() && right[j].is_ascii_digit() {
            let l_start = i;
            while i < left.len() && left[i].is_ascii_digit() {
                i += 1;
            }
            let r_start = j;
            while j < right.len() && right[j].is_ascii_digit() {
                j += 1;
            }

            let l_num: Vec<char> = left[l_start..i]
                .iter()
                .copied()
                .skip_while(|c| *c == '0')
                .collect();
            let r_num: Vec<char> = right[r_start..j]
                .iter()
                .copied()
                .skip_while(|c| *c == '0')
                .collect();

            let cmp = l_num
                .len()
                .cmp(&r_num.len())
                .then_with(|| l_num.cmp(&r_num));
            if cmp != Ordering::Equal {
                return cmp;
            }
            let zero_cmp = (i - l_start).cmp(&(j - r_start));
            if zero_cmp != Ordering::Equal {
                return zero_cmp;
            }
        } else {
            let lc = left[i].to_ascii_lowercase();
            let rc = right[j].to_ascii_lowercase();
            if lc != rc {
                return lc.cmp(&rc);
            }
            i += 1;
            j += 1;
        }
    }

    left.len().cmp(&right.len()).then_with(|| a.cmp(b))
}

/// 搜索语法：空串全匹配；`re:` 前缀按正则（不区分大小写）；含 `*`/`?` 按通配；否则子串。
pub(crate) fn search_matches(query: &str, text: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let text = text.to_lowercase();
    if let Some(pattern) = query.strip_prefix("re:") {
        return regex::RegexBuilder::new(pattern)
            .case_insensitive(true)
            .build()
            .map(|regex| regex.is_match(&text))
            .unwrap_or_else(|_| text.contains(pattern));
    }
    if query.contains('*') || query.contains('?') {
        wildcard_match(query, &text)
    } else {
        text.contains(query)
    }
}

/// 简单的 `*` / `?` 通配匹配；调用方负责大小写归一化。
pub(crate) fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut resume = 0usize;

    while ti < text.len() {
        if pi < pattern.len() && (pattern[pi] == '?' || pattern[pi] == text[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < pattern.len() && pattern[pi] == '*' {
            star = Some(pi);
            resume = ti;
            pi += 1;
        } else if let Some(star_pos) = star {
            pi = star_pos + 1;
            resume += 1;
            ti = resume;
        } else {
            return false;
        }
    }

    while pi < pattern.len() && pattern[pi] == '*' {
        pi += 1;
    }
    pi == pattern.len()
}
