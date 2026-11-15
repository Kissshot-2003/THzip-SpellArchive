use std::io;
use std::path::PathBuf;

use thzip_core::secret::SecretString;

/// 加密后的密码文件与 prefs.toml 同目录，方便便携模式一起搬走（但换机后解不开）。
const FILE_NAME: &str = "remembered-password.bin";

fn store_path() -> Option<PathBuf> {
    crate::prefs::config_path().map(|path| path.with_file_name(FILE_NAME))
}

/// 覆盖写入最近一次密码。DPAPI 的密钥由 Windows 按"本机 + 本用户"派生，
/// 因此文件本身泄露也无法在别的账户或机器上解密。
pub fn save(secret: &SecretString) -> io::Result<()> {
    let path = store_path().ok_or_else(|| io::Error::other("找不到用户配置目录"))?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let blob = protect(secret.expose().as_bytes())?;
    thzip_core::safety::write_atomic(&path, |file| {
        use std::io::Write;
        file.write_all(&blob)
    })
}

/// 读取并解密。文件不存在、被别的账户创建或 DPAPI 拒绝时一律返回 None，
/// 调用方退回手动输入，不需要区分具体原因。
pub fn load() -> Option<SecretString> {
    let path = store_path()?;
    let blob = std::fs::read(path).ok()?;
    let mut plain = unprotect(&blob).ok()?;
    let text = String::from_utf8(std::mem::take(&mut plain)).ok()?;
    // 中间字节缓冲不再需要，立即清零：解密结果只保留在 SecretString 里。
    plain.fill(0);
    Some(SecretString::new(text))
}

pub fn clear() {
    if let Some(path) = store_path() {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn protect(data: &[u8]) -> io::Result<Vec<u8>> {
    use windows_sys::Win32::Foundation::{GetLastError, LocalFree};
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };

    // SAFETY: 输入输出都是本栈帧内的合法 BLOB；szDataDescr 传 null 表示不要描述。
    // 成功后 output.pbData 由系统分配，必须用 LocalFree 归还。
    unsafe {
        let ok = CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        );
        if ok == 0 {
            return Err(io::Error::from_raw_os_error(GetLastError() as i32));
        }
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData as *mut core::ffi::c_void);
        Ok(bytes)
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn unprotect(data: &[u8]) -> io::Result<Vec<u8>> {
    use windows_sys::Win32::Foundation::{GetLastError, LocalFree};
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptUnprotectData,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };

    // SAFETY: 同 protect；ppszDataDescr 传 null，避免系统再分配一命名字符串。
    unsafe {
        let ok = CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        );
        if ok == 0 {
            return Err(io::Error::from_raw_os_error(GetLastError() as i32));
        }
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData as *mut core::ffi::c_void);
        Ok(bytes)
    }
}

#[cfg(not(windows))]
fn protect(_data: &[u8]) -> io::Result<Vec<u8>> {
    Err(io::Error::other("DPAPI 仅在 Windows 上可用"))
}

#[cfg(not(windows))]
fn unprotect(_data: &[u8]) -> io::Result<Vec<u8>> {
    Err(io::Error::other("DPAPI 仅在 Windows 上可用"))
}

#[cfg(test)]
mod tests {
    use super::{protect, unprotect};

    #[test]
    #[cfg(windows)]
    fn dpapi_round_trips_within_current_user() {
        let plain = "correct horse battery staple".as_bytes();
        let blob = protect(plain).expect("加密失败");
        assert_ne!(blob, plain, "密文不应等于明文");
        let restored = unprotect(&blob).expect("解密失败");
        assert_eq!(restored, plain);
    }
}
