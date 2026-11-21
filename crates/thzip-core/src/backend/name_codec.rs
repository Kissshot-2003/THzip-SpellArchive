use encoding_rs::{GBK, SHIFT_JIS, UTF_8};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NameEncoding {
    #[default]
    Utf8,
    Gbk,
    ShiftJis,
    /// 原始字节已转义输出（`raw` 模式），或解码结果不可还原。
    Lossy,
}

/// CLI / GUI 的编码强制指定。`Auto` 走打分判定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NameOverride {
    #[default]
    Auto,
    Utf8,
    Gbk,
    /// 用户习惯名；底层实际为 encoding_rs 的 SHIFT_JIS。
    ShiftJis,
    /// 不解码，保留原始字节并以 `%XX` 转义输出。
    Raw,
}

impl NameEncoding {
    pub fn label(self) -> &'static str {
        match self {
            Self::Utf8 => "utf8",
            Self::Gbk => "gbk",
            Self::ShiftJis => "cp932",
            Self::Lossy => "raw",
        }
    }
}

impl NameOverride {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Utf8 => "utf8",
            Self::Gbk => "gbk",
            Self::ShiftJis => "cp932",
            Self::Raw => "raw",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "utf8" | "utf-8" => Some(Self::Utf8),
            "gbk" | "cp936" => Some(Self::Gbk),
            "cp932" | "sjis" | "shift_jis" | "shift-jis" => Some(Self::ShiftJis),
            "raw" => Some(Self::Raw),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DecodedName {
    pub text: String,
    pub encoding: NameEncoding,
    /// 解码过程中出现替换字符，说明还原不可靠，UI 应给出提示。
    pub lossy: bool,
    pub confidence: u8,
}

/// 编码判定结果。分数越高越可信；`alternates` 供 UI 切换。
#[derive(Debug, Clone)]
pub struct NameGuess {
    pub encoding: NameEncoding,
    pub confidence: u8,
    pub alternates: Vec<(NameEncoding, u8)>,
    /// 判定后得到的名字（或 raw 模式下的转义串）。
    pub text: String,
}

/// zip 文件名还原：UTF-8 有效时直接采用，否则在 GBK / Shift-JIS 之间打分。
///
/// zip 的 UTF-8 标志位（bit 11）与 Info-ZIP Unicode Path 扩展（0x7075）
/// 由 zip crate 在处理 `ZipFile` 时完成，并反映在 `ZipFileData::is_utf8` 上；
/// 这一路优先级高于本函数，见 `backend/zip.rs`。
pub fn guess(raw: &[u8]) -> NameGuess {
    if let Ok(text) = std::str::from_utf8(raw) {
        return NameGuess {
            encoding: NameEncoding::Utf8,
            confidence: 100,
            alternates: Vec::new(),
            text: text.to_owned(),
        };
    }

    let (gbk_text, _, gbk_errors) = GBK.decode(raw);
    let (sjis_text, _, sjis_errors) = SHIFT_JIS.decode(raw);

    let gbk_score = score_text(&gbk_text, gbk_errors, false);
    let sjis_score = score_text(&sjis_text, sjis_errors, true);

    if gbk_score >= sjis_score {
        NameGuess {
            encoding: NameEncoding::Gbk,
            confidence: gbk_score,
            alternates: vec![(NameEncoding::ShiftJis, sjis_score)],
            text: gbk_text.into_owned(),
        }
    } else {
        NameGuess {
            encoding: NameEncoding::ShiftJis,
            confidence: sjis_score,
            alternates: vec![(NameEncoding::Gbk, gbk_score)],
            text: sjis_text.into_owned(),
        }
    }
}

/// 按用户指定强制解码。`Raw` 用 `%XX` 转义保留原始字节，`%` 自身转义为 `%25`，
/// 因此转义结果可逆（见 `unescape_raw`）。
pub fn decode_with(raw: &[u8], override_encoding: NameOverride) -> NameGuess {
    match override_encoding {
        NameOverride::Auto => guess(raw),
        NameOverride::Utf8 => {
            let (text, _, had_errors) = UTF_8.decode(raw);
            NameGuess {
                encoding: NameEncoding::Utf8,
                confidence: if had_errors { 0 } else { 100 },
                alternates: Vec::new(),
                text: text.into_owned(),
            }
        }
        NameOverride::Gbk => {
            let (text, _, _) = GBK.decode(raw);
            NameGuess {
                encoding: NameEncoding::Gbk,
                confidence: 100,
                alternates: Vec::new(),
                text: text.into_owned(),
            }
        }
        NameOverride::ShiftJis => {
            let (text, _, _) = SHIFT_JIS.decode(raw);
            NameGuess {
                encoding: NameEncoding::ShiftJis,
                confidence: 100,
                alternates: Vec::new(),
                text: text.into_owned(),
            }
        }
        NameOverride::Raw => NameGuess {
            encoding: NameEncoding::Lossy,
            confidence: 0,
            alternates: Vec::new(),
            text: escape_raw(raw),
        },
    }
}

/// 把原始字节转义成可逆的 ASCII 串。字母数字与 `-_.` 保留，其余按 `%XX` 转义。
pub fn escape_raw(raw: &[u8]) -> String {
    let mut out = String::with_capacity(raw.len().saturating_mul(3));
    for &byte in raw {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.') {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push_str(&format!("{byte:02X}"));
        }
    }
    out
}

/// `escape_raw` 的逆运算，供测试验证可逆性。
pub fn unescape_raw(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = bytes.get(index + 1..index + 3)?;
            let hi = (hex[0] as char).to_digit(16)?;
            let lo = (hex[1] as char).to_digit(16)?;
            out.push(((hi << 4) | lo) as u8);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    Some(out)
}

/// 打分：基于已解码文本，不再做二次解码。返回 0-100。
fn score_text(text: &str, had_errors: bool, sjis_candidate: bool) -> u8 {
    // 留出了加分空间：满分 100 会让假名/伪汉字等信号失去区分度。
    let mut score = 70i32;
    let replacement = text.chars().filter(|c| *c == '\u{FFFD}').count() as i32;
    score -= replacement * 40;
    if had_errors {
        score -= 15;
    }

    // 只用全角假名（平假名/片假名）作为 Shift-JIS 信号。半角片假名在 GBK 误解
    // 中文名时也会大量出现，没有区分度，不计分。
    let mut full_kana = 0i32;
    let mut cjk = 0i32;
    let mut bad = 0i32;
    let mut pseudo_hanzi = 0i32;

    for ch in text.chars() {
        let code = ch as u32;
        if (0x3040..=0x30FF).contains(&code) {
            full_kana += 1;
        } else if (0x4E00..=0x9FFF).contains(&code) {
            cjk += 1;
        }
        // GBK 误解 Shift-JIS 时常见的"伪汉字"残留。
        if matches!(ch, '‚' | 'Ì' | 'Í' | 'Î' | 'Ï' | 'ｌ' | '@' | 'ｶ' | 'ﾞ' | 'ﾟ') {
            pseudo_hanzi += 1;
        }
        if (0xE000..=0xF8FF).contains(&code) || (code < 0x20 && !matches!(ch, '\t' | '\n' | '\r')) {
            bad += 1;
        }
    }

    // 全角假名是 Shift-JIS 的决定性信号；GBK 侧用"伪汉字"残留特征反向扣分。
    if sjis_candidate {
        score += (full_kana * 12).min(40);
    } else {
        score -= (pseudo_hanzi * 9).min(45);
    }
    score -= bad * 12;
    score += (cjk / 4).min(5);

    score.clamp(0, 100) as u8
}

/// 兼容旧调用：返回判定后的文本与编码。
pub fn decode(raw: &[u8]) -> DecodedName {
    let guess = guess(raw);
    DecodedName {
        text: guess.text,
        encoding: guess.encoding,
        lossy: guess.confidence < 60 || guess.encoding == NameEncoding::Lossy,
        confidence: guess.confidence,
    }
}

/// 供 zip 后端使用：严格 UTF-8 解码，有错误则返回 None。
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
    use super::{NameEncoding, NameOverride, decode, escape_raw, guess, unescape_raw};

    #[test]
    fn keeps_utf8() {
        let decoded = decode("测试/文件.txt".as_bytes());
        assert_eq!(decoded.encoding, NameEncoding::Utf8);
        assert_eq!(decoded.text, "测试/文件.txt");
        assert!(!decoded.lossy);
        assert_eq!(decoded.confidence, 100);
    }

    #[test]
    fn decodes_gbk() {
        let raw = &[0xB2, 0xE2, 0xCA, 0xD4, 0x2E, 0x74, 0x78, 0x74];
        assert_eq!(decode(raw).text, "测试.txt");
    }

    #[test]
    fn raw_escape_is_reversible_including_percent() {
        let raw = b"%41\x82\xA0.\x81\x40";
        let escaped = escape_raw(raw);
        assert!(!escaped.contains('\u{FFFD}'));
        assert_eq!(unescape_raw(&escaped).unwrap(), raw);
    }

    #[test]
    fn kana_biases_towards_shift_jis() {
        // 「テスト.txt」的 Shift-JIS 字节。
        let raw = &[0x83, 0x65, 0x83, 0x58, 0x83, 0x67, 0x2E, 0x74, 0x78, 0x74];
        let g = guess(raw);
        assert_eq!(g.encoding, NameEncoding::ShiftJis, "scores: {g:?}");
        assert!(g.confidence >= 60, "scores: {g:?}");
        assert_eq!(g.alternates.len(), 1);
    }

    #[test]
    fn override_utf8_and_raw() {
        let raw = "文件.txt".as_bytes();
        assert_eq!(
            decode_with_test(raw, NameOverride::Utf8).encoding,
            NameEncoding::Utf8
        );
        let raw_guess = decode_with_test(raw, NameOverride::Raw);
        assert_eq!(raw_guess.encoding, NameEncoding::Lossy);
        assert_eq!(unescape_raw(&raw_guess.text).unwrap(), raw);
    }

    fn decode_with_test(raw: &[u8], o: NameOverride) -> super::NameGuess {
        super::decode_with(raw, o)
    }
}
