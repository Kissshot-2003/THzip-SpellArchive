/// 资源与安全限额。三档预设，用户可在设置页逐项调整（见 `Limits::set`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_total_bytes: u64,
    pub max_entry_bytes: u64,
    pub max_entries: u32,
    pub max_ratio_streamed: u32,
    pub max_path_depth: usize,
    pub max_path_len: usize,
    pub max_dict_size: u64,
    /// 套娃解压的最大嵌套层数。本项目不自动递归展开归档，
    /// 该值用于限制「一次操作内允许处理的嵌套归档层数」并驱动 UI 提示。
    pub max_nesting: usize,
    /// 解压前要求目标盘至少保留的可用空间。低于此值即使总量够也拒绝，
    /// 避免把系统盘写满。`0` 表示只校验总量。
    pub min_free_space: u64,
}

impl Default for Limits {
    fn default() -> Self {
        LimitProfile::Default.preset()
    }
}

/// 限额项标识，供设置页按字段读写。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitField {
    TotalBytes,
    EntryBytes,
    Entries,
    RatioStreamed,
    PathDepth,
    PathLen,
    DictSize,
    Nesting,
    MinFreeSpace,
}

impl LimitField {
    pub const ALL: [LimitField; 9] = [
        LimitField::TotalBytes,
        LimitField::EntryBytes,
        LimitField::Entries,
        LimitField::RatioStreamed,
        LimitField::PathDepth,
        LimitField::PathLen,
        LimitField::DictSize,
        LimitField::Nesting,
        LimitField::MinFreeSpace,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LimitField::TotalBytes => "解压总量上限",
            LimitField::EntryBytes => "单条目大小上限",
            LimitField::Entries => "条目数上限",
            LimitField::RatioStreamed => "膨胀倍率上限",
            LimitField::PathDepth => "路径层级上限",
            LimitField::PathLen => "路径长度上限",
            LimitField::DictSize => "压缩字典上限",
            LimitField::Nesting => "套娃层数上限",
            LimitField::MinFreeSpace => "保留可用空间",
        }
    }

    /// 该字段是否以字节为单位（UI 用 MiB/GiB 展示）。
    pub fn is_bytes(self) -> bool {
        matches!(
            self,
            LimitField::TotalBytes
                | LimitField::EntryBytes
                | LimitField::DictSize
                | LimitField::MinFreeSpace
        )
    }
}

impl Limits {
    pub fn get(&self, field: LimitField) -> u64 {
        match field {
            LimitField::TotalBytes => self.max_total_bytes,
            LimitField::EntryBytes => self.max_entry_bytes,
            LimitField::Entries => u64::from(self.max_entries),
            LimitField::RatioStreamed => u64::from(self.max_ratio_streamed),
            LimitField::PathDepth => self.max_path_depth as u64,
            LimitField::PathLen => self.max_path_len as u64,
            LimitField::DictSize => self.max_dict_size,
            LimitField::Nesting => self.max_nesting as u64,
            LimitField::MinFreeSpace => self.min_free_space,
        }
    }

    /// 按字段写回。超出字段可表示范围时返回 false（不做静默截断）。
    pub fn set(&mut self, field: LimitField, value: u64) -> bool {
        match field {
            LimitField::TotalBytes => self.max_total_bytes = value,
            LimitField::EntryBytes => self.max_entry_bytes = value,
            LimitField::Entries => match u32::try_from(value) {
                Ok(v) => self.max_entries = v,
                Err(_) => return false,
            },
            LimitField::RatioStreamed => match u32::try_from(value) {
                Ok(v) => self.max_ratio_streamed = v,
                Err(_) => return false,
            },
            LimitField::PathDepth => self.max_path_depth = value as usize,
            LimitField::PathLen => self.max_path_len = value as usize,
            LimitField::DictSize => self.max_dict_size = value,
            LimitField::Nesting => self.max_nesting = value as usize,
            LimitField::MinFreeSpace => self.min_free_space = value,
        }
        true
    }

    /// 单条目上限不得高于总量上限，否则后者永远不会先触发。
    pub fn normalize(&mut self) {
        if self.max_entry_bytes > self.max_total_bytes {
            self.max_entry_bytes = self.max_total_bytes;
        }
        if self.max_entries == 0 {
            self.max_entries = 1;
        }
        if self.max_path_depth == 0 {
            self.max_path_depth = 1;
        }
        if self.max_path_len < 260 {
            self.max_path_len = 260;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitProfile {
    Strict,
    Default,
    Relaxed,
}

impl LimitProfile {
    pub fn preset(self) -> Limits {
        match self {
            LimitProfile::Strict => Limits {
                max_total_bytes: 5 * 1024 * 1024 * 1024,
                max_entry_bytes: 2 * 1024 * 1024 * 1024,
                max_entries: 50_000,
                max_ratio_streamed: 100,
                max_path_depth: 32,
                max_path_len: 8_192,
                max_dict_size: 128 * 1024 * 1024,
                max_nesting: 2,
                min_free_space: 512 * 1024 * 1024,
            },
            LimitProfile::Default => Limits {
                max_total_bytes: 20 * 1024 * 1024 * 1024,
                max_entry_bytes: 8 * 1024 * 1024 * 1024,
                max_entries: 200_000,
                max_ratio_streamed: 200,
                max_path_depth: 64,
                max_path_len: 32_767,
                max_dict_size: 512 * 1024 * 1024,
                max_nesting: 4,
                min_free_space: 1024 * 1024 * 1024,
            },
            LimitProfile::Relaxed => Limits {
                max_total_bytes: 200 * 1024 * 1024 * 1024,
                max_entry_bytes: 64 * 1024 * 1024 * 1024,
                max_entries: 1_000_000,
                max_ratio_streamed: 1_000,
                max_path_depth: 128,
                max_path_len: 32_767,
                max_dict_size: 1024 * 1024 * 1024,
                max_nesting: 8,
                min_free_space: 2 * 1024 * 1024 * 1024,
            },
        }
    }

    /// 三档预设 + 名称，供设置页列出。
    pub fn all() -> [LimitProfile; 3] {
        [
            LimitProfile::Strict,
            LimitProfile::Default,
            LimitProfile::Relaxed,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            LimitProfile::Strict => "严格",
            LimitProfile::Default => "默认",
            LimitProfile::Relaxed => "宽松",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LimitField, LimitProfile, Limits};

    #[test]
    fn fields_round_trip() {
        let mut limits = Limits::default();
        for field in LimitField::ALL {
            let before = limits.get(field);
            assert!(limits.set(field, before), "{field:?} 写回失败");
            assert_eq!(limits.get(field), before, "{field:?} 往返不一致");
        }
    }

    #[test]
    fn rejects_out_of_range_values() {
        let mut limits = Limits::default();
        assert!(!limits.set(LimitField::Entries, u64::from(u32::MAX) + 1));
        assert_eq!(limits.max_entries, Limits::default().max_entries);
    }

    #[test]
    fn normalizes_entry_limit_against_total() {
        let mut limits = Limits::default();
        limits.set(LimitField::TotalBytes, 1024);
        limits.normalize();
        assert_eq!(limits.max_entry_bytes, 1024);
    }

    #[test]
    fn presets_are_ordered() {
        let strict = LimitProfile::Strict.preset();
        let default = LimitProfile::Default.preset();
        let relaxed = LimitProfile::Relaxed.preset();
        assert!(strict.max_total_bytes < default.max_total_bytes);
        assert!(default.max_total_bytes < relaxed.max_total_bytes);
        assert!(strict.max_nesting < relaxed.max_nesting);
        assert!(strict.min_free_space < relaxed.min_free_space);
    }
}
