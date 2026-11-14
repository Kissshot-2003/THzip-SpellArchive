use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

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

#[cfg(test)]
mod tests {
    use super::TempDir;

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
}
