//! 设置页限额字段的界面层适配。
//!
//! 字段表、展示名、字节单位判断与按字段读写全部由 `thzip_core::limits`
//! 提供（`LimitField::ALL` / `Limits::get` / `Limits::set` / `normalize`）。
//! 这里只保留界面侧需要的东西：预设列表与归一化入口。字段名的本地化文案
//! 在 `crate::i18n::limit_field_label`，不在这里重复一份。
//!
//! 之前界面层自己维护过一份 7 字段定义，core 扩到 9 个字段时设置页直接漏了
//! 两项，因此现在把定义权完全交回 core。

pub use thzip_core::limits::LimitField;
use thzip_core::limits::{LimitProfile, Limits};

/// 三档预设，设置页按此顺序列出。
pub const PROFILES: [LimitProfile; 3] = [
    LimitProfile::Strict,
    LimitProfile::Default,
    LimitProfile::Relaxed,
];

/// 修正互相矛盾的组合。实现在 core，界面只管调用。
pub fn normalize(limits: &mut Limits) {
    limits.normalize();
}

#[cfg(test)]
mod tests {
    use super::{LimitField, PROFILES, normalize};
    use thzip_core::limits::Limits;

    #[test]
    fn fields_round_trip_through_limits() {
        let mut limits = Limits::default();
        for field in LimitField::ALL {
            let before = limits.get(field);
            assert!(limits.set(field, before));
            assert_eq!(limits.get(field), before);
        }
    }

    #[test]
    fn rejects_entries_above_u32() {
        let mut limits = Limits::default();
        assert!(!limits.set(LimitField::Entries, u64::from(u32::MAX) + 1));
        assert_eq!(limits.max_entries, Limits::default().max_entries);
    }

    #[test]
    fn normalize_clamps_entry_size_and_minimums() {
        let mut limits = Limits::default();
        limits.set(LimitField::TotalBytes, 1024);
        limits.set(LimitField::Entries, 0);
        normalize(&mut limits);
        assert_eq!(limits.max_entry_bytes, 1024);
        assert_eq!(limits.max_entries, 1);
        assert_eq!(limits.max_path_len, 32_767);
    }

    /// 每一个设置页会展示的字段都必须可写回；漏字段就是设置项被静默忽略。
    #[test]
    fn every_field_is_editable() {
        let mut limits = Limits::default();
        for field in LimitField::ALL {
            let probe = limits.get(field).saturating_add(1);
            assert!(limits.set(field, probe), "{field:?} 无法写回");
        }
        assert_eq!(PROFILES.len(), 3);
    }
}
