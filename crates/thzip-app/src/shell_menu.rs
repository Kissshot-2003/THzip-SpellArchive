use std::io;
use std::path::Path;

use crate::i18n::{Locale, TextKey, builtin};

/// 文件右键菜单：key、显示名文案、命令行开关（空表示直接打开）。
const FILE_MENUS: [(&str, TextKey, &str); 4] = [
    (
        r"Software\Classes\*\shell\THzip-SpellArchive",
        TextKey::ShellOpen,
        "",
    ),
    (
        r"Software\Classes\*\shell\THzip-SpellArchive.ExtractHere",
        TextKey::ShellExtractHere,
        "--extract-here",
    ),
    (
        r"Software\Classes\*\shell\THzip-SpellArchive.ExtractTo",
        TextKey::ShellExtractTo,
        "--extract-to",
    ),
    (
        r"Software\Classes\*\shell\THzip-SpellArchive.AddToZip",
        TextKey::ShellAddToZip,
        "--add",
    ),
];

/// 目录右键菜单：只提供“添加到压缩包”。
const DIR_MENUS: [(&str, TextKey, &str); 1] = [(
    r"Software\Classes\Directory\shell\THzip-SpellArchive.AddToZip",
    TextKey::ShellAddToZip,
    "--add",
)];

/// 旧名 SpellArchive 的菜单键，只用于一次性迁移。
const LEGACY_FILE_MENUS: [&str; 4] = [
    r"Software\Classes\*\shell\SpellArchive",
    r"Software\Classes\*\shell\SpellArchive.ExtractHere",
    r"Software\Classes\*\shell\SpellArchive.ExtractTo",
    r"Software\Classes\*\shell\SpellArchive.AddToZip",
];
const LEGACY_DIR_MENUS: [&str; 1] = [r"Software\Classes\Directory\shell\SpellArchive.AddToZip"];

#[cfg(windows)]
pub fn is_installed() -> bool {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    FILE_MENUS
        .iter()
        .chain(DIR_MENUS.iter())
        .any(|(base, _, _)| hkcu.open_subkey(base).is_ok())
}

/// 已安装右键菜单时，把注册表里的 exe 路径刷新成当前进程路径，并按当前语言更新文案。
///
/// 改名后的首次启动会把旧 SpellArchive 菜单迁移到新键：先装新菜单，再删除旧键。
#[cfg(windows)]
pub fn refresh_if_installed(locale: Locale) -> io::Result<()> {
    let exe = std::env::current_exe()?;
    if is_installed() {
        install(&exe, locale)?;
        if legacy_installed() {
            remove_legacy()?;
        }
        return Ok(());
    }
    if legacy_installed() {
        install(&exe, locale)?;
        remove_legacy()?;
    }
    Ok(())
}

#[cfg(windows)]
fn legacy_installed() -> bool {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    LEGACY_FILE_MENUS
        .iter()
        .chain(LEGACY_DIR_MENUS.iter())
        .any(|base| hkcu.open_subkey(base).is_ok())
}

#[cfg(windows)]
fn remove_legacy() -> io::Result<()> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for base in LEGACY_FILE_MENUS.iter().chain(LEGACY_DIR_MENUS.iter()) {
        match hkcu.delete_subkey_all(base) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

#[cfg(windows)]
pub fn install(exe: &Path, locale: Locale) -> io::Result<()> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let icon = format!("\"{}\"", exe.display());
    for (base, text_key, flag) in FILE_MENUS.iter().chain(DIR_MENUS.iter()) {
        let (key, _) = hkcu.create_subkey(base)?;
        key.set_value("", &builtin(locale, *text_key).to_string())?;
        key.set_value("Icon", &icon)?;
        let command = if flag.is_empty() {
            format!("\"{}\" \"%1\"", exe.display())
        } else {
            format!("\"{}\" {} \"%1\"", exe.display(), flag)
        };
        let (cmd, _) = hkcu.create_subkey(format!("{base}\\command"))?;
        cmd.set_value("", &command)?;
    }
    Ok(())
}

#[cfg(windows)]
pub fn uninstall() -> io::Result<()> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for (base, _, _) in FILE_MENUS.iter().chain(DIR_MENUS.iter()) {
        match hkcu.delete_subkey_all(base) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn is_installed() -> bool {
    false
}

#[cfg(not(windows))]
pub fn refresh_if_installed(_locale: Locale) -> io::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
pub fn install(_exe: &Path, _locale: Locale) -> io::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
pub fn uninstall() -> io::Result<()> {
    Ok(())
}
