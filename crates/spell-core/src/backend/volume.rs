use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
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
    let Some((prefix, digits)) = name.rsplit_once('.') else {
        return Ok(vec![first.to_path_buf()]);
    };
    let width = digits.len();

    // 扫描目录而不是按顺序探测：能把“缺了中间某卷”的情况报清楚。
    let mut found: std::collections::BTreeMap<u32, PathBuf> = std::collections::BTreeMap::new();
    for entry in std::fs::read_dir(parent)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let Some(candidate) = file_name.to_str() else {
            continue;
        };
        let Some(rest) = candidate.strip_prefix(&format!("{prefix}.")) else {
            continue;
        };
        if rest.is_empty() || rest.len() > 4 || !rest.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        if let Ok(index) = rest.parse::<u32>() {
            if index > 0 {
                found.insert(index, entry.path());
            }
        }
    }

    if found.is_empty() {
        return Ok(vec![first.to_path_buf()]);
    }

    let max = *found.keys().next_back().unwrap_or(&1);
    let missing: Vec<String> = (1..=max)
        .filter(|index| !found.contains_key(index))
        .map(|index| format!("{index:0width$}"))
        .collect();
    if !missing.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("分卷不完整，缺少：{}", missing.join(", ")),
        ));
    }

    Ok(found.into_values().collect())
}

/// 把多个分卷当成一个连续的 Read + Seek 流。
///
/// 7z 分卷按字节切分，`Archive::read` 需要 Seek，所以不能简单串接 `Read`。
pub struct MultiVolumeReader {
    paths: Vec<PathBuf>,
    starts: Vec<u64>,
    total: u64,
    index: usize,
    file: File,
    pos: u64,
}

impl MultiVolumeReader {
    pub fn open(paths: &[PathBuf]) -> io::Result<Self> {
        if paths.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "没有分卷"));
        }
        let mut sizes = Vec::with_capacity(paths.len());
        let mut total = 0u64;
        for path in paths {
            let size = std::fs::metadata(path)?.len();
            sizes.push(size);
            total = total.saturating_add(size);
        }

        let mut starts = Vec::with_capacity(paths.len());
        let mut acc = 0u64;
        for size in sizes {
            starts.push(acc);
            acc = acc.saturating_add(size);
        }

        let file = File::open(&paths[0])?;
        Ok(Self {
            paths: paths.to_vec(),
            starts,
            total,
            index: 0,
            file,
            pos: 0,
        })
    }
}

impl Read for MultiVolumeReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() || self.pos >= self.total {
            return Ok(0);
        }
        loop {
            let read = self.file.read(buf)?;
            if read > 0 {
                self.pos = self.pos.saturating_add(read as u64);
                return Ok(read);
            }
            if self.index + 1 >= self.paths.len() {
                return Ok(0);
            }
            self.index += 1;
            self.file = File::open(&self.paths[self.index])?;
        }
    }
}

impl Seek for MultiVolumeReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let target = match pos {
            SeekFrom::Start(offset) => offset as i128,
            SeekFrom::Current(delta) => self.pos as i128 + delta as i128,
            SeekFrom::End(delta) => self.total as i128 + delta as i128,
        };
        if target < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "分卷流不能 seek 到负偏移",
            ));
        }
        let target = (target as u64).min(self.total);
        let index = self
            .starts
            .partition_point(|start| *start <= target)
            .saturating_sub(1);
        self.index = index;
        self.file = File::open(&self.paths[index])?;
        self.file
            .seek(SeekFrom::Start(target - self.starts[index]))?;
        self.pos = target;
        Ok(target)
    }
}

#[cfg(test)]
mod tests {
    use super::{MultiVolumeReader, is_volume_name};
    use std::io::{Read, Seek, SeekFrom};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn detects_7z_volumes() {
        assert!(is_volume_name("a.7z.001"));
        assert!(is_volume_name("档案.7z.012"));
        assert!(!is_volume_name("a.zip"));
        assert!(!is_volume_name("a.7z"));
        assert!(!is_volume_name("a.7z.abc"));
    }

    #[test]
    fn reports_missing_volume_numbers() {
        let dir = std::env::temp_dir().join(format!(
            "spellarchive-vol-missing-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        for index in [1u32, 2, 4] {
            std::fs::write(dir.join(format!("a.7z.{index:03}")), b"x").unwrap();
        }

        let err = super::collect_volumes(&dir.join("a.7z.001")).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        assert!(err.to_string().contains("003"), "{err}");
        assert!(!err.to_string().contains("002"), "{err}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn collects_consecutive_volumes_from_any_part() {
        let dir = std::env::temp_dir().join(format!(
            "spellarchive-vol-order-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        for index in [1u32, 2, 3] {
            std::fs::write(dir.join(format!("a.7z.{index:03}")), b"x").unwrap();
        }

        let volumes = super::collect_volumes(&dir.join("a.7z.002")).unwrap();
        assert_eq!(volumes.len(), 3);
        assert!(volumes[0].ends_with("a.7z.001"));
        assert!(volumes[2].ends_with("a.7z.003"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_and_seeks_across_volumes() {
        let dir = std::env::temp_dir().join(format!(
            "spellarchive-vol-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let data: Vec<u8> = (0..35u8).collect();
        let parts = [&data[..10], &data[10..20], &data[20..]];
        let mut paths = Vec::new();
        for (index, part) in parts.iter().enumerate() {
            let path = dir.join(format!("a.7z.{:03}", index + 1));
            std::fs::write(&path, part).unwrap();
            paths.push(path);
        }

        let mut reader = MultiVolumeReader::open(&paths).unwrap();
        let mut all = Vec::new();
        reader.read_to_end(&mut all).unwrap();
        assert_eq!(all, data);

        reader.seek(SeekFrom::Start(12)).unwrap();
        let mut chunk = [0u8; 4];
        reader.read_exact(&mut chunk).unwrap();
        assert_eq!(chunk, [12, 13, 14, 15]);

        reader.seek(SeekFrom::End(-3)).unwrap();
        let mut tail = Vec::new();
        reader.read_to_end(&mut tail).unwrap();
        assert_eq!(tail, vec![32, 33, 34]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
