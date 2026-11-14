use std::path::{Component, Path, PathBuf};

use crate::limits::Limits;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SanitizeError {
    #[error("条目名为空")]
    Empty,
    #[error("拒绝绝对路径")]
    Absolute,
    #[error("拒绝包含 `..` 的路径")]
    Parent,
    #[error("路径层级超限")]
    DepthExceeded,
    #[error("路径总长度超限")]
    PathTooLong,
    #[error("文件名过长")]
    NameTooLong,
    #[error("文件名是 Windows 设备保留名")]
    Reserved,
    #[error("文件名包含非法字符: {0:?}")]
    IllegalChar(char),
}

/// 把归档内的名字净化为**相对路径**。任何一段非法即整体拒绝该条目，
/// 不做静默修正（静默修正会掩盖攻击）。
pub fn sanitize_rel(raw_name: &str, limits: &Limits) -> Result<PathBuf, SanitizeError> {
    if raw_name.is_empty() {
        return Err(SanitizeError::Empty);
    }

    let unified = raw_name.replace('\\', "/");
    if unified.starts_with('/') {
        return Err(SanitizeError::Absolute);
    }
    let bytes = unified.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return Err(SanitizeError::Absolute);
    }

    let mut rel = PathBuf::new();
    let mut depth = 0usize;

    for segment in unified.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return Err(SanitizeError::Parent);
        }
        if let Some(bad) = find_illegal(segment) {
            return Err(SanitizeError::IllegalChar(bad));
        }

        // Win32 会静默截断段尾的句点与空格，导致两个不同条目落到同一文件。
        let trimmed = segment.trim_end_matches([' ', '.']);
        if trimmed.is_empty() {
            return Err(SanitizeError::Empty);
        }
        if is_reserved_name(trimmed) {
            return Err(SanitizeError::Reserved);
        }
        if trimmed.len() > 255 {
            return Err(SanitizeError::NameTooLong);
        }

        depth += 1;
        if depth > limits.max_path_depth {
            return Err(SanitizeError::DepthExceeded);
        }
        rel.push(trimmed);
    }

    if rel.as_os_str().is_empty() {
        return Err(SanitizeError::Empty);
    }
    Ok(rel)
}

/// 拼到目标根目录下，并验证总长度。只允许 `Normal` 组件，因此结构上不可能逃逸。
pub fn resolve_under(root: &Path, rel: &Path, limits: &Limits) -> Result<PathBuf, SanitizeError> {
    for component in rel.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(SanitizeError::Parent);
        }
    }

    let joined = root.join(rel);
    if joined.as_os_str().len() > limits.max_path_len {
        return Err(SanitizeError::PathTooLong);
    }
    Ok(joined)
}

pub fn sanitize_and_resolve(
    root: &Path,
    raw_name: &str,
    limits: &Limits,
) -> Result<PathBuf, SanitizeError> {
    let rel = sanitize_rel(raw_name, limits)?;
    resolve_under(root, &rel, limits)
}

fn find_illegal(segment: &str) -> Option<char> {
    segment
        .chars()
        .find(|c| matches!(c, ':' | '*' | '?' | '<' | '>' | '|' | '"') || (*c as u32) < 0x20)
}

fn is_reserved_name(segment: &str) -> bool {
    let base = segment.split('.').next().unwrap_or(segment);
    let upper = base.to_ascii_uppercase();

    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    for prefix in ["COM", "LPT"] {
        if let Some(rest) = upper.strip_prefix(prefix) {
            if rest.len() == 1 && rest.as_bytes()[0].is_ascii_digit() {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> Limits {
        Limits::default()
    }

    #[test]
    fn keeps_normal_paths() {
        assert_eq!(
            sanitize_rel("dir/sub/file.txt", &limits()).unwrap(),
            PathBuf::from("dir/sub/file.txt")
        );
    }

    #[test]
    fn normalizes_separators_and_empty_segments() {
        assert_eq!(
            sanitize_rel("dir\\\\sub//file.txt", &limits()).unwrap(),
            PathBuf::from("dir/sub/file.txt")
        );
    }

    #[test]
    fn rejects_absolute_paths() {
        assert_eq!(
            sanitize_rel("/etc/passwd", &limits()),
            Err(SanitizeError::Absolute)
        );
        assert_eq!(
            sanitize_rel("C:/Windows/x", &limits()),
            Err(SanitizeError::Absolute)
        );
        assert_eq!(
            sanitize_rel("\\\\server\\share", &limits()),
            Err(SanitizeError::Absolute)
        );
    }

    #[test]
    fn rejects_parent_traversal() {
        assert_eq!(sanitize_rel("../x", &limits()), Err(SanitizeError::Parent));
        assert_eq!(
            sanitize_rel("a/../../b", &limits()),
            Err(SanitizeError::Parent)
        );
        assert_eq!(
            resolve_under(Path::new("out"), Path::new("../x"), &limits()),
            Err(SanitizeError::Parent)
        );
    }

    #[test]
    fn rejects_ntfs_ads_and_illegal_chars() {
        assert_eq!(sanitize_rel("a:", &limits()), Err(SanitizeError::Absolute));
        assert_eq!(
            sanitize_rel("file.txt:stream", &limits()),
            Err(SanitizeError::IllegalChar(':'))
        );
        assert_eq!(
            sanitize_rel("a?b", &limits()),
            Err(SanitizeError::IllegalChar('?'))
        );
        assert_eq!(
            sanitize_rel("a\u{0001}b", &limits()),
            Err(SanitizeError::IllegalChar('\u{0001}'))
        );
    }

    #[test]
    fn rejects_windows_device_names() {
        for name in ["CON", "con.txt", "PRN", "AUX", "NUL", "COM1", "lpt9.log"] {
            assert_eq!(
                sanitize_rel(name, &limits()),
                Err(SanitizeError::Reserved),
                "{name} 应被拒绝"
            );
        }
        assert!(sanitize_rel("console.txt", &limits()).is_ok());
        assert!(sanitize_rel("COM10.txt", &limits()).is_ok());
    }

    #[test]
    fn trims_trailing_dots_and_spaces() {
        assert_eq!(
            sanitize_rel("a./b ", &limits()).unwrap(),
            PathBuf::from("a/b")
        );
    }

    #[test]
    fn rejects_empty_names() {
        assert_eq!(sanitize_rel("", &limits()), Err(SanitizeError::Empty));
        assert_eq!(sanitize_rel(".", &limits()), Err(SanitizeError::Empty));
        assert_eq!(sanitize_rel("...", &limits()), Err(SanitizeError::Empty));
    }

    #[test]
    fn enforces_depth_limit() {
        let deep = (0..40)
            .map(|i| format!("d{i}"))
            .collect::<Vec<_>>()
            .join("/");
        assert_eq!(
            sanitize_rel(&deep, &LimitProfile::Strict.preset()),
            Err(SanitizeError::DepthExceeded)
        );
    }

    use crate::limits::LimitProfile;

    #[test]
    fn resolves_inside_root() {
        let root = Path::new("C:/out");
        let joined = sanitize_and_resolve(root, "a/b.txt", &limits()).unwrap();
        assert_eq!(joined, PathBuf::from("C:/out").join("a/b.txt"));
        assert!(joined.starts_with(root));
    }
}
