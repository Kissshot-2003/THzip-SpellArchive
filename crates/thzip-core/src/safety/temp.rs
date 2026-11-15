use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 独占创建的临时目录：创建失败或目标已存在时换一个名字重试，
/// Drop 时递归删除。避免复用可预测路径导致本地竞态。
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn create(prefix: &str) -> io::Result<Self> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        for _ in 0..32 {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0);
            let nonce = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("{prefix}-{}-{nanos}-{nonce}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(err) => return Err(err),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not create a unique temp directory",
        ))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// 清理目录里由本程序遗留的原子写临时文件。
///
/// 进程崩溃或断电时 `AtomicFile::Drop` 不会执行，`.thzip-spellarchive.*.tmp`
/// 会留在解压目标目录里。启动时扫一遍常见目标目录即可回收。
///
/// 只认「前缀 + 后缀」都匹配的文件，并且要求 `min_age` 已过——这样不会误删
/// 另一个正在运行的实例刚创建、还没落盘的临时文件。返回删除成功的数量。
pub fn sweep_stale_temp(dir: &Path, min_age: Duration) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let now = SystemTime::now();
    let mut removed = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with(crate::safety::atomic_fs::TEMP_PREFIX) || !name.ends_with(".tmp") {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let age = meta
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .unwrap_or_default();
        if age < min_age {
            continue;
        }
        if std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::{TempDir, sweep_stale_temp};

    #[test]
    fn creates_unique_dirs_and_cleans_up() {
        let first = TempDir::create("thzip-temp-test").unwrap();
        let second = TempDir::create("thzip-temp-test").unwrap();
        assert_ne!(first.path(), second.path());
        assert!(first.path().is_dir());
        let path = first.path().to_path_buf();
        drop(first);
        assert!(!path.exists());
    }

    /// 只清自己的残骸：前缀不符、后缀不符、太新的文件都必须留下。
    #[test]
    fn sweeps_only_old_own_temp_files() {
        let dir = TempDir::create("thzip-sweep-test").unwrap();
        let base = dir.path().to_path_buf();

        let ours = base.join(".thzip-spellarchive.123.456.tmp");
        let other_prefix = base.join(".other-tool.123.456.tmp");
        let other_suffix = base.join(".thzip-spellarchive.123.456.part");
        let keep = base.join("payload.bin");
        for path in [&ours, &other_prefix, &other_suffix, &keep] {
            std::fs::write(path, b"x").unwrap();
        }

        // min_age = 0：刚创建的文件也算「够老」，这里只验证前缀/后缀过滤。
        assert_eq!(sweep_stale_temp(&base, std::time::Duration::ZERO), 1);
        assert!(!ours.exists());
        assert!(other_prefix.exists());
        assert!(other_suffix.exists());
        assert!(keep.exists());

        // 门槛足够大时，即使是我们自己的残骸也不动。
        std::fs::write(&ours, b"x").unwrap();
        assert_eq!(
            sweep_stale_temp(&base, std::time::Duration::from_secs(3600)),
            0
        );
        assert!(ours.exists());
    }
}
