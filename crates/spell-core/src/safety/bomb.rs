use std::fmt;

use crate::limits::Limits;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitHit {
    TotalBytes,
    EntryBytes,
    EntryCount,
    Ratio,
    DictionarySize,
    Nesting,
    EntryPathDepth,
    EntryNameLength,
    ScanDepth,
    ScanEntries,
}

impl LimitHit {
    pub fn message(self) -> &'static str {
        match self {
            LimitHit::TotalBytes => "解压总量超过上限",
            LimitHit::EntryBytes => "单条目大小超过上限",
            LimitHit::EntryCount => "条目数超过上限",
            LimitHit::Ratio => "解压膨胀倍率异常（疑似压缩炸弹）",
            LimitHit::DictionarySize => "压缩字典超出内存上限",
            LimitHit::Nesting => "嵌套归档层数超过上限",
            LimitHit::EntryPathDepth => "归档内路径层级超过上限",
            LimitHit::EntryNameLength => "归档内名字过长",
            LimitHit::ScanDepth => "打包路径层级超过上限",
            LimitHit::ScanEntries => "打包条目数超过上限",
        }
    }
}

impl fmt::Display for LimitHit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

/// 流式预算：唯一计数点应在 `ExtractSink::on_data` 内。
#[derive(Debug, Clone)]
pub struct Budget {
    limits: Limits,
    written: u64,
    consumed: u64,
    entries: u32,
}

impl Budget {
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            written: 0,
            consumed: 0,
            entries: 0,
        }
    }

    pub fn on_entry(&mut self) -> Result<(), LimitHit> {
        self.entries = self.entries.saturating_add(1);
        if self.entries > self.limits.max_entries {
            return Err(LimitHit::EntryCount);
        }
        Ok(())
    }

    /// `entry_written` 是该条目此前已写字节数；`chunk` 是本块长度；
    /// `consumed_delta` 是为产生本块而消耗的压缩字节（固实包可为 0）。
    pub fn on_write(
        &mut self,
        entry_written: u64,
        chunk: u64,
        consumed_delta: u64,
    ) -> Result<(), LimitHit> {
        let entry_total = entry_written.saturating_add(chunk);
        if entry_total > self.limits.max_entry_bytes {
            return Err(LimitHit::EntryBytes);
        }

        let total = self.written.saturating_add(chunk);
        if total > self.limits.max_total_bytes {
            return Err(LimitHit::TotalBytes);
        }

        let consumed = self.consumed.saturating_add(consumed_delta);
        // 开头几 KB 的比率没有统计意义，等消耗量足够再判断。
        if consumed > 64 * 1024 {
            let ratio = total as f64 / consumed as f64;
            if ratio > self.limits.max_ratio_streamed as f64 {
                return Err(LimitHit::Ratio);
            }
        }

        self.written = total;
        self.consumed = consumed;
        Ok(())
    }

    pub fn written(&self) -> u64 {
        self.written
    }

    pub fn entries(&self) -> u32 {
        self.entries
    }

    pub fn ratio(&self) -> Option<f64> {
        if self.consumed == 0 {
            None
        } else {
            Some(self.written as f64 / self.consumed as f64)
        }
    }

    pub fn check_dict_size(&self, dict_size: u64) -> Result<(), LimitHit> {
        if dict_size > self.limits.max_dict_size {
            Err(LimitHit::DictionarySize)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Budget, LimitHit};
    use crate::limits::LimitProfile;

    #[test]
    fn rejects_total_bytes() {
        let mut budget = Budget::new(LimitProfile::Strict.preset());
        // 单条不超限（2 GiB），三条累计超过总量上限（5 GiB）。
        let chunk = 1_900 * 1024 * 1024u64;
        assert!(budget.on_write(0, chunk, 0).is_ok());
        assert!(budget.on_write(0, chunk, 0).is_ok());
        assert_eq!(budget.on_write(0, chunk, 0), Err(LimitHit::TotalBytes));
    }

    #[test]
    fn rejects_entry_bytes() {
        let mut budget = Budget::new(LimitProfile::Strict.preset());
        assert_eq!(
            budget.on_write(0, 3 * 1024 * 1024 * 1024, 0),
            Err(LimitHit::EntryBytes)
        );
    }

    #[test]
    fn rejects_ratio_bomb() {
        let mut budget = Budget::new(LimitProfile::Strict.preset());
        // 每消耗 512 B 写出 100 KiB，倍率约 200，超过 Strict 的 100。
        let mut written = 0u64;
        for _ in 0..1_000 {
            let entry_written = written;
            written += 100 * 1024;
            if budget.on_write(entry_written, 100 * 1024, 512) == Err(LimitHit::Ratio) {
                return;
            }
        }
        panic!("应当触发倍率限制");
    }

    #[test]
    fn counts_entries() {
        let mut budget = Budget::new(LimitProfile::Strict.preset());
        for _ in 0..50_000 {
            budget.on_entry().unwrap();
        }
        assert_eq!(budget.on_entry(), Err(LimitHit::EntryCount));
    }
}
