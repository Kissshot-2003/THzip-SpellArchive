use std::fmt;

use crate::limits::Limits;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitHit {
    TotalBytes,
    EntryBytes,
    EntryCount,
    Ratio,
    DictionarySize,
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
    entries: u32,
}

impl Budget {
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            written: 0,
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

    /// `entry_written` 是该条目此前已写字节数；`chunk` 是本块长度。
    pub fn on_write(&mut self, entry_written: u64, chunk: u64) -> Result<(), LimitHit> {
        let entry_total = entry_written.saturating_add(chunk);
        if entry_total > self.limits.max_entry_bytes {
            return Err(LimitHit::EntryBytes);
        }

        let total = self.written.saturating_add(chunk);
        if total > self.limits.max_total_bytes {
            return Err(LimitHit::TotalBytes);
        }

        self.written = total;
        Ok(())
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
        assert!(budget.on_write(0, chunk).is_ok());
        assert!(budget.on_write(0, chunk).is_ok());
        assert_eq!(budget.on_write(0, chunk), Err(LimitHit::TotalBytes));
    }

    #[test]
    fn rejects_entry_bytes() {
        let mut budget = Budget::new(LimitProfile::Strict.preset());
        assert_eq!(
            budget.on_write(0, 3 * 1024 * 1024 * 1024),
            Err(LimitHit::EntryBytes)
        );
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
