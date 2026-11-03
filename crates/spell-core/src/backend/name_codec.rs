use encoding_rs::{GBK, SHIFT_JIS, UTF_8};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameEncoding {
    Utf8,
    Gbk,
    ShiftJis,
    Lossy,
}

#[derive(Debug, Clone)]
pub struct DecodedName {
    pub text: String,
    pub encoding: NameEncoding,
    /// 解码过程中出现替换字符，说明还原不可靠，UI 应给出提示。
    pub lossy: bool,
}

/// zip 文件名还原：UTF-8 优先，其次在 GBK / Shift-JIS 中选「替换字符更少」的一个。
///
/// 真实实现还需检查 zip 的 UTF-8 标志位（bit 11）与 Info-ZIP Unicode Path
/// 扩展（0x7075），这两处判断属于 zip 后端，不在这里。
pub fn decode(raw: &[u8]) -> DecodedName {
    if let Ok(text) = std::str::from_utf8(raw) {
        return DecodedName {
            text: text.to_owned(),
            encoding: NameEncoding::Utf8,
            lossy: false,
        };
    }

    let (gbk_text, _, gbk_errors) = GBK.decode(raw);
    let (sjis_text, _, sjis_errors) = SHIFT_JIS.decode(raw);

    let gbk_bad = replacement_count(&gbk_text) + u32::from(gbk_errors);
    let sjis_bad = replacement_count(&sjis_text) + u32::from(sjis_errors);

    if gbk_bad <= sjis_bad {
        DecodedName {
            text: gbk_text.into_owned(),
            encoding: NameEncoding::Gbk,
            lossy: gbk_bad > 0,
        }
    } else {
        DecodedName {
            text: sjis_text.into_owned(),
            encoding: NameEncoding::ShiftJis,
            lossy: sjis_bad > 0,
        }
    }
}

fn replacement_count(text: &str) -> u32 {
    text.chars().filter(|c| *c == '\u{FFFD}').count() as u32
}

/// 供 zip 后端使用：有 UTF-8 标志位时走严格 UTF-8。
pub fn decode_strict_utf8(raw: &[u8]) -> Option<String> {
    let (text, _, had_errors) = UTF_8.decode(raw);
    if had_errors {
        None
    } else {
        Some(text.into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::{NameEncoding, decode};

    #[test]
    fn keeps_utf8() {
        let decoded = decode("测试/文件.txt".as_bytes());
        assert_eq!(decoded.encoding, NameEncoding::Utf8);
        assert_eq!(decoded.text, "测试/文件.txt");
        assert!(!decoded.lossy);
    }

    #[test]
    fn decodes_gbk() {
        let raw = &[0xB2, 0xE2, 0xCA, 0xD4, 0x2E, 0x74, 0x78, 0x74];
        assert_eq!(decode(raw).text, "测试.txt");
    }
}
