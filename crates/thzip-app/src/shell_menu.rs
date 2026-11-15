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

/// "打开方式"用的 ProgID。只登记 OpenWithProgids，不改各扩展名的默认值，
/// 因此不会抢走用户当前的默认程序，只是在列表里多一个候选。
const PROGID: &str = "THzip-SpellArchive.Archive";
const OPEN_WITH_EXTENSIONS: [&str; 10] = [
    ".zip", ".7z", ".rar", ".tar", ".gz", ".tgz", ".bz2", ".xz", ".zst", ".lz4",
];
const APP_NAME: &str = "THzip-SpellArchive";

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
    use std::borrow::Cow;

    use winreg::RegKey;
    use winreg::RegValue;
    use winreg::enums::{HKEY_CURRENT_USER, RegType};

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let icon = format!("\"{}\"", exe.display());
    let command = format!("\"{}\" \"%1\"", exe.display());
    let display_name = builtin(locale, TextKey::ShellOpen).to_string();

    for (base, text_key, flag) in FILE_MENUS.iter().chain(DIR_MENUS.iter()) {
        let (key, _) = hkcu.create_subkey(base)?;
        key.set_value("", &builtin(locale, *text_key).to_string())?;
        key.set_value("Icon", &icon)?;
        let entry_command = if flag.is_empty() {
            command.clone()
        } else {
            format!("\"{}\" {} \"%1\"", exe.display(), flag)
        };
        let (cmd, _) = hkcu.create_subkey(format!("{base}\\command"))?;
        cmd.set_value("", &entry_command)?;
    }

    // ProgID：描述"用本程序打开"的动词，供 OpenWithProgids 引用。
    let (progid, _) = hkcu.create_subkey(format!(r"Software\Classes\{PROGID}"))?;
    progid.set_value("", &display_name)?;
    progid.set_value("FriendlyAppName", &APP_NAME)?;
    progid.set_value("DefaultIcon", &format!("{icon},0"))?;

    let (progid_cmd, _) =
        hkcu.create_subkey(format!(r"Software\Classes\{PROGID}\shell\open\command"))?;
    progid_cmd.set_value("", &command)?;

    // Applications\<exe>：让"打开方式"对话框能列出本程序并记住用户选择。
    let app_key = format!(
        r"Software\Classes\Applications\{}",
        exe.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "thzip-spellarchive.exe".to_owned())
    );
    let (app, _) = hkcu.create_subkey(&app_key)?;
    app.set_value("", &display_name)?;
    app.set_value("FriendlyAppName", &APP_NAME)?;
    app.set_value("DefaultIcon", &format!("{icon},0"))?;
    let (app_cmd, _) = hkcu.create_subkey(format!(r"{app_key}\shell\open\command"))?;
    app_cmd.set_value("", &command)?;
    let (supported, _) = hkcu.create_subkey(format!(r"{app_key}\SupportedTypes"))?;
    for extension in OPEN_WITH_EXTENSIONS {
        supported.set_value(extension, &"")?;
    }

    // 只写 OpenWithProgids 值（REG_NONE 空值），不动扩展名的默认关联。
    let empty = RegValue {
        bytes: Cow::Borrowed(&[]),
        vtype: RegType::REG_NONE,
    };
    for extension in OPEN_WITH_EXTENSIONS {
        let (open_with, _) =
            hkcu.create_subkey(format!(r"Software\Classes\{extension}\OpenWithProgids"))?;
        open_with.set_raw_value(PROGID, &empty)?;
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

    // 只删自己写入的 OpenWithProgids 值，保留其他程序登记的候选。
    for extension in OPEN_WITH_EXTENSIONS {
        if let Ok(open_with) = hkcu.open_subkey_with_flags(
            format!(r"Software\Classes\{extension}\OpenWithProgids"),
            winreg::enums::KEY_WRITE,
        ) {
            let _ = open_with.delete_value(PROGID);
        }
    }

    for key in [format!(r"Software\Classes\{PROGID}"), applications_key()] {
        match hkcu.delete_subkey_all(&key) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

/// `Applications` 下的键名取自 exe 文件名；取不到时退回默认名。
#[cfg(windows)]
fn applications_key() -> String {
    let name = std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "thzip-spellarchive.exe".to_owned());
    format!(r"Software\Classes\Applications\{name}")
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
