/// 资源与安全限额。三档预设，用户可在设置页微调。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_total_bytes: u64,
    pub max_entry_bytes: u64,
    pub max_entries: u32,
    pub max_ratio_streamed: u32,
    pub max_path_depth: usize,
    pub max_path_len: usize,
    pub max_dict_size: u64,
    pub max_nesting: u8,
}

impl Default for Limits {
    fn default() -> Self {
        LimitProfile::Default.preset()
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
                max_nesting: 0,
            },
            LimitProfile::Default => Limits {
                max_total_bytes: 20 * 1024 * 1024 * 1024,
                max_entry_bytes: 8 * 1024 * 1024 * 1024,
                max_entries: 200_000,
                max_ratio_streamed: 200,
                max_path_depth: 64,
                max_path_len: 32_767,
                max_dict_size: 512 * 1024 * 1024,
                max_nesting: 2,
            },
            LimitProfile::Relaxed => Limits {
                max_total_bytes: 200 * 1024 * 1024 * 1024,
                max_entry_bytes: 64 * 1024 * 1024 * 1024,
                max_entries: 1_000_000,
                max_ratio_streamed: 1_000,
                max_path_depth: 128,
                max_path_len: 32_767,
                max_dict_size: 1024 * 1024 * 1024,
                max_nesting: 4,
            },
        }
    }
}
