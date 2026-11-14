#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayGuard {
    pub text: String,
    /// 命中双向控制字符/零宽字符：扩展名可能被伪装。
    pub suspicious: bool,
}

/// 文件名的显示层防护。只改显示，不改落盘字节。
///
/// U+202E（RLO）可把 `exe.txt` 显示成 `txt.exe`；零宽字符可让同名文件看起来重名。
/// 策略：替换成可见标记，并标记 `suspicious` 供 UI 加警示。
pub fn guard_display(raw: &str) -> DisplayGuard {
    let mut text = String::with_capacity(raw.len());
    let mut suspicious = false;

    for ch in raw.chars() {
        let code = ch as u32;
        let flagged = matches!(
            code,
            0x202A..=0x202E | 0x2066..=0x2069 | 0x200B..=0x200F | 0x2060 | 0xFEFF
        );
        if flagged {
            suspicious = true;
            text.push_str(&format!("⟦U+{code:04X}⟧"));
        } else {
            text.push(ch);
        }
    }

    DisplayGuard { text, suspicious }
}

#[cfg(test)]
mod tests {
    use super::guard_display;

    #[test]
    fn keeps_plain_names() {
        let guarded = guard_display("普通文件.txt");
        assert_eq!(guarded.text, "普通文件.txt");
        assert!(!guarded.suspicious);
    }

    #[test]
    fn flags_rlo_spoofing() {
        let guarded = guard_display("evil\u{202E}txt.exe");
        assert!(guarded.suspicious);
        assert!(guarded.text.contains("U+202E"));
        assert!(!guarded.text.contains('\u{202E}'));
    }

    #[test]
    fn flags_zero_width() {
        assert!(guard_display("a\u{200B}b").suspicious);
    }
}
