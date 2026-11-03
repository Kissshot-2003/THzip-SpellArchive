use std::io;
use std::path::{Path, PathBuf};

/// `foo.7z.001` -> true
pub fn is_volume_name(name: &str) -> bool {
    let Some((_, digits)) = name.rsplit_once('.') else {
        return false;
    };
    !digits.is_empty()
        && digits.len() <= 4
        && digits.chars().all(|c| c.is_ascii_digit())
        && name[..name.len() - digits.len() - 1]
            .to_ascii_lowercase()
            .ends_with(".7z")
        && digits != "7z"
}

/// 收集同组分卷：`data.7z.001`、`data.7z.002` ...
///
/// 7z 分卷本质是按字节切分，读取时拼接成逻辑流即可；zip 的 `.z01` 是另一套格式，不支持。
pub fn collect_volumes(first: &Path) -> io::Result<Vec<PathBuf>> {
    let name = first
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "分卷文件名无效"))?;
    if !is_volume_name(name) {
        return Ok(vec![first.to_path_buf()]);
    }

    let parent = first
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "分卷没有父目录"))?;
    let stem = &name[..name.len() - 3];

    let mut volumes = Vec::new();
    for index in 1..=9_999u32 {
        let candidate = parent.join(format!("{stem}{index:03}"));
        if !candidate.exists() {
            break;
        }
        volumes.push(candidate);
    }

    if volumes.is_empty() {
        volumes.push(first.to_path_buf());
    }
    Ok(volumes)
}

#[cfg(test)]
mod tests {
    use super::is_volume_name;

    #[test]
    fn detects_7z_volumes() {
        assert!(is_volume_name("a.7z.001"));
        assert!(is_volume_name("档案.7z.012"));
        assert!(!is_volume_name("a.zip"));
        assert!(!is_volume_name("a.7z"));
        assert!(!is_volume_name("a.7z.abc"));
    }
}
