use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// 原子写：先写同目录临时文件，成功后替换目标。
///
/// 直接 `File::create(target)` 会截断用户原文件；解压失败/取消时原文件就没了。
pub fn write_atomic(
    target: &Path,
    write: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    let mut atomic = AtomicFile::create(target)?;
    write(atomic.file_mut())?;
    atomic.commit()
}

/// 提交时对持久化的要求。
///
/// 两者的共同点是**原子性**：临时文件写坏或中途取消，目标文件原封不动。
/// 区别只在断电场景下的额外保证。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Durability {
    /// 只 flush 到系统缓存就替换。
    ///
    /// 断电可能丢掉刚写完的这个文件，但不会留下半截内容，也不会影响其他文件。
    /// 解压落盘属于这一类：丢了重新解一次即可，而逐个文件 fsync 会让解压
    /// 速度掉到 7-Zip 的四成（204 个文件就是 204 次刷盘）。
    Flush,
    /// fsync 之后再替换，断电也能保证改名后的内容已经落盘。
    ///
    /// 归档写入属于这一类：产物是用户的唯一副本，损坏无法恢复。
    Sync,
}

/// 流式版本的 [`write_atomic`]：分块写入期间目标文件保持不变。
///
/// `Drop` 时若未 [`AtomicFile::commit`]，临时文件会被删除，原文件不受影响。
pub struct AtomicFile {
    temp: PathBuf,
    target: PathBuf,
    file: Option<File>,
    done: bool,
    /// 直写模式：`temp == target`，没有中间文件，`commit` 只是收尾。
    ///
    /// 只在**目标原本不存在**时使用（解压全新文件的主路径）。它比原子替换
    /// 少一次临时文件创建与一次改名，204 个文件就能省下近半的解压时间。
    /// 代价是进程在写入途中被杀会留下半截文件——这与 7-Zip、WinRAR 的行为
    /// 一致，且**绝不会**破坏已存在的文件；CRC 校验失败或中途出错时这里
    /// 会主动把文件删掉，因此「落盘的必然完整」仍然成立。
    direct: bool,
}

impl AtomicFile {
    pub fn create(target: &Path) -> io::Result<Self> {
        let parent = target
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "target has no parent"))?;
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }

        let temp = temp_path(parent);
        // 同时要 read/write：zip 追加需要回读已写内容，后续 reopen 也就能省掉。
        // `create_new` 保证不会跟随既有的（可能被替换成链接的）临时文件。
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&temp)?;
        Ok(Self {
            temp,
            target: target.to_path_buf(),
            file: Some(file),
            done: false,
            direct: false,
        })
    }

    /// 直写新建文件：目标不存在时直接写最终路径，不经过临时文件。
    ///
    /// 目标已存在时返回 `AlreadyExists`，调用方应退回 [`AtomicFile::create`]。
    /// `create_new` 保证不会顺着既有的链接写出去。
    pub fn create_direct(target: &Path) -> io::Result<Self> {
        let parent = target
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "target has no parent"))?;
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?;
        Ok(Self {
            temp: target.to_path_buf(),
            target: target.to_path_buf(),
            file: Some(file),
            done: false,
            direct: true,
        })
    }

    /// 复制现有目标到临时文件并回到起点（供 zip 追加使用）。
    ///
    /// 之后的修改全部落在临时文件；只有 `commit` 成功才会替换原文件。
    pub fn create_copy_of(target: &Path) -> io::Result<Self> {
        let mut source = File::open(target)?;
        let mut atomic = Self::create(target)?;
        io::copy(&mut source, atomic.file_mut())?;
        atomic.file_mut().seek(SeekFrom::Start(0))?;
        Ok(atomic)
    }

    /// 把临时文件指针移回起点。句柄始终复用，不按路径重新打开：
    /// 重新打开会多一个「临时文件被替换成链接」的窗口。
    pub fn rewind(&mut self) -> io::Result<()> {
        self.file_mut().seek(SeekFrom::Start(0))?;
        Ok(())
    }

    pub fn file_mut(&mut self) -> &mut File {
        self.file.as_mut().expect("AtomicFile already committed")
    }

    pub fn file(&self) -> Option<&File> {
        self.file.as_ref()
    }

    /// 临时文件打开后的真实路径（已解析重解析点）。
    ///
    /// 用它而不是原始路径来判断落点：父目录在检查之后被换成 junction 时，
    /// 路径文本看不出区别，句柄的真实位置却会变。
    pub fn resolved_temp_path(&self) -> Option<PathBuf> {
        self.file.as_ref().and_then(final_path_of_handle)
    }

    /// fsync 后原子替换目标（[`Durability::Sync`]）。失败时原文件保持不变。
    pub fn commit(self) -> io::Result<()> {
        self.commit_with(Durability::Sync)
    }

    /// 按给定的持久化要求原子替换目标。
    pub fn commit_with(mut self, durability: Durability) -> io::Result<()> {
        let result = (|| -> io::Result<()> {
            if let Some(mut file) = self.file.take() {
                match durability {
                    Durability::Sync => file.sync_all()?,
                    // 解压路径：只保证「要么完整、要么不存在」，不为每个文件付 fsync。
                    Durability::Flush => file.flush()?,
                }
                drop(file);
            }
            if self.direct {
                // 内容已经写在最终路径上，没有需要替换的东西。
                return Ok(());
            }
            // 只读目标：先清属性，否则 ReplaceFileW / MoveFileExW 会失败。
            clear_readonly(&self.target);
            replace_file(&self.temp, &self.target)
        })();

        if result.is_ok() {
            self.done = true;
        }
        result
    }
}

impl Drop for AtomicFile {
    fn drop(&mut self) {
        if !self.done {
            self.file.take();
            // 直写模式下 `temp` 就是目标：没提交就说明内容不完整或不合法，
            // 必须删掉，否则会留下一个半截文件冒充成功结果。
            let _ = std::fs::remove_file(&self.temp);
        }
    }
}

// 供 zip 等需要 Write + Seek 的写入后端直接复用原子覆盖语义。
impl Write for AtomicFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file_mut().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file_mut().flush()
    }
}

impl Read for AtomicFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file_mut().read(buf)
    }
}

impl Seek for AtomicFile {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.file_mut().seek(pos)
    }
}

/// 目标是否是重解析点（junction / symlink / mount point）。
///
/// 解压时必须拒绝顺着这类目录写出去（TOCTOU），否则恶意包可以先建一个
/// 指向 `C:/Windows` 的 junction，再把后续文件写进系统目录。
pub fn is_reparse_point(path: &Path) -> io::Result<bool> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err),
    };

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        Ok(meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
    }
    #[cfg(not(windows))]
    {
        Ok(meta.file_type().is_symlink())
    }
}

/// 目录的真实路径（解析所有重解析点）。根目录建立后调用一次，
/// 后续用文件句柄的真实路径与之比较即可发现「中途被换成链接」。
pub fn resolved_dir_path(path: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let file = open_dir_handle(path)?;
        final_path_of_handle(&file)
    }
    #[cfg(not(windows))]
    {
        std::fs::canonicalize(path).ok()
    }
}

fn clear_readonly(target: &Path) {
    let Ok(meta) = std::fs::symlink_metadata(target) else {
        return;
    };
    if !meta.file_type().is_file() || !meta.permissions().readonly() {
        return;
    }
    let mut permissions = meta.permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    let _ = std::fs::set_permissions(target, permissions);
}

/// 原子写临时文件的前缀。启动清理（`sweep_stale_temp`）靠它识别「自己的」残骸。
pub(crate) const TEMP_PREFIX: &str = ".thzip-spellarchive.";

fn temp_path(parent: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nonce = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    parent.join(format!(
        "{TEMP_PREFIX}{}.{}.tmp",
        std::process::id(),
        (nanos as u64) ^ nonce
    ))
}

/// 转到宽字符并补 `\\?\` 前缀，绕开 MAX_PATH 限制。
///
/// manifest 已声明 `longPathAware`，但那还依赖系统注册表开关；
/// 显式加前缀在任意 Windows 版本上都生效。
#[cfg(windows)]
pub(crate) fn wide_path(path: &Path) -> Vec<u16> {
    let text = path.to_string_lossy();
    let needs_prefix =
        !text.starts_with(r"\\?\") && !text.starts_with(r"\\.\") && text.len() >= 240;

    if !needs_prefix {
        return text.encode_utf16().chain(std::iter::once(0)).collect();
    }

    let mut prefixed = String::with_capacity(text.len() + 8);
    if let Some(rest) = text.strip_prefix(r"\\") {
        prefixed.push_str(r"\\?\UNC\");
        prefixed.push_str(rest);
    } else {
        prefixed.push_str(r"\\?\");
        prefixed.push_str(&text);
    }
    prefixed.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn open_dir_handle(path: &Path) -> Option<File> {
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    let wide = wide_path(path);
    // SAFETY: 路径已 NUL 结尾；成功时把句柄交给 File 管理生命周期。
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return None;
    }
    // SAFETY: handle 是刚取得的有效目录句柄，所有权转移给 File。
    Some(unsafe { File::from_raw_handle(handle as _) })
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn final_path_of_handle(file: &File) -> Option<PathBuf> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_NAME_NORMALIZED, GetFinalPathNameByHandleW, VOLUME_NAME_DOS,
    };

    let handle = file.as_raw_handle() as _;
    // SAFETY: 句柄由 File 持有且有效；先用 0 长度查询所需长度。
    let needed = unsafe {
        GetFinalPathNameByHandleW(
            handle,
            std::ptr::null_mut(),
            0,
            FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
        )
    };
    if needed == 0 {
        return None;
    }
    let mut buffer = vec![0u16; needed as usize + 1];
    // SAFETY: buffer 长度足够容纳返回值（含 NUL）。
    let written = unsafe {
        GetFinalPathNameByHandleW(
            handle,
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
        )
    };
    if written == 0 {
        return None;
    }
    let text = String::from_utf16_lossy(&buffer[..written as usize]);
    Some(PathBuf::from(text))
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn replace_file(src: &Path, dst: &Path) -> io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW, ReplaceFileW,
    };

    let src_w = wide_path(src);
    let dst_w = wide_path(dst);

    // ReplaceFileW 会保留原文件的 ACL；它要求目标已存在。
    if dst.exists() {
        let ok = unsafe {
            ReplaceFileW(
                dst_w.as_ptr(),
                src_w.as_ptr(),
                std::ptr::null(),
                0,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        if ok != 0 {
            return Ok(());
        }
    }

    let ok = unsafe {
        MoveFileExW(
            src_w.as_ptr(),
            dst_w.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(windows))]
fn replace_file(src: &Path, dst: &Path) -> io::Result<()> {
    std::fs::rename(src, dst)
}

#[cfg(test)]
mod tests {
    use super::{AtomicFile, Durability, write_atomic};
    use std::io::{Read, Seek, SeekFrom, Write};

    fn unique(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "thzip-spellarchive-test-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn replaces_existing_file() {
        let dir = unique("atomic");
        let target = dir.join("a.txt");

        std::fs::write(&target, b"old").unwrap();
        write_atomic(&target, |file| file.write_all(b"new")).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keeps_original_when_write_fails() {
        let dir = unique("atomic-fail");
        let target = dir.join("b.txt");

        std::fs::write(&target, b"keep").unwrap();
        let result = write_atomic(&target, |_| Err(std::io::Error::other("boom")));
        assert!(result.is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"keep");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 只读目标不应让提交失败：先清属性再替换。
    #[test]
    // 仅为了清掉只读位好让 remove_dir_all 删掉临时目录：这里的 `set_readonly(false)`
    // 传的是 Windows 语义的只读位，Unix 上没有等价概念，lint 提示不适用。
    #[allow(clippy::permissions_set_readonly_false)]
    fn overwrites_readonly_target() {
        let dir = unique("atomic-ro");
        let target = dir.join("ro.txt");
        std::fs::write(&target, b"old").unwrap();
        let mut permissions = std::fs::metadata(&target).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&target, permissions).unwrap();

        write_atomic(&target, |file| file.write_all(b"new")).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");

        // 只为了让 remove_dir_all 能删掉它。显式设 `readonly(false)` 在 Unix 上
        // 等于 chmod 666，clippy 会拦；Windows 上只读位不影响删除，
        // 因此这里只在需要时清除，且用文件自身权限复位。
        #[cfg(windows)]
        {
            let mut permissions = std::fs::metadata(&target).unwrap().permissions();
            permissions.set_readonly(false);
            std::fs::set_permissions(&target, permissions).unwrap();
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 直写模式：没有临时文件，提交后内容就在最终路径上。
    #[test]
    fn direct_write_lands_on_target_without_temp_file() {
        let dir = unique("atomic-direct");
        let target = dir.join("new.bin");

        let mut atomic = AtomicFile::create_direct(&target).unwrap();
        atomic.file_mut().write_all(b"payload").unwrap();
        atomic.commit_with(Durability::Flush).unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), b"payload");
        // 目录里只应有目标文件本身，没有任何 .tmp 残骸。
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "直写不应留下临时文件: {leftovers:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 直写模式下不提交就 Drop：必须把半截文件删掉，不能冒充成功结果。
    #[test]
    fn direct_write_removes_partial_on_drop() {
        let dir = unique("atomic-direct-abort");
        let target = dir.join("aborted.bin");

        {
            let mut atomic = AtomicFile::create_direct(&target).unwrap();
            atomic.file_mut().write_all(b"half").unwrap();
            // 不 commit，模拟 CRC 校验失败 / 取消。
        }
        assert!(!target.exists(), "未提交的直写文件必须被删除");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 直写只在目标不存在时可用；已存在必须报 AlreadyExists，
    /// 让调用方退回原子替换，从而不会覆盖用户原有文件。
    #[test]
    fn direct_write_refuses_existing_target() {
        let dir = unique("atomic-direct-exists");
        let target = dir.join("exists.bin");
        std::fs::write(&target, b"old").unwrap();

        let err = match AtomicFile::create_direct(&target) {
            Ok(_) => panic!("目标已存在时不应创建直写句柄"),
            Err(err) => err,
        };
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&target).unwrap(), b"old");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 复制后指针回到起点，句柄始终复用（不按路径重开）。
    #[test]
    fn copy_starts_at_beginning_and_reuses_handle() {
        let dir = unique("atomic-copy");
        let target = dir.join("c.bin");
        std::fs::write(&target, b"0123456789").unwrap();

        let mut atomic = AtomicFile::create_copy_of(&target).unwrap();
        let mut head = [0u8; 4];
        atomic.read_exact(&mut head).unwrap();
        assert_eq!(&head, b"0123");
        atomic.seek(SeekFrom::Start(10)).unwrap();
        atomic.write_all(b"abc").unwrap();
        atomic.commit().unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), b"0123456789abc");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
