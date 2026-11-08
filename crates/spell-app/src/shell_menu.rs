use std::io;
use std::path::Path;

/// 文件右键菜单：key、显示名、命令行开关（空表示直接打开）。
const FILE_MENUS: [(&str, &str, &str); 4] = [
    (
        r"Software\Classes\*\shell\SpellArchive",
        "用 SpellArchive 打开",
        "",
    ),
    (
        r"Software\Classes\*\shell\SpellArchive.ExtractHere",
        "解压到当前文件夹",
        "--extract-here",
    ),
    (
        r"Software\Classes\*\shell\SpellArchive.ExtractTo",
        "解压到单独文件夹",
        "--extract-to",
    ),
    (
        r"Software\Classes\*\shell\SpellArchive.AddToZip",
        "添加到 zip 压缩包",
        "--add",
    ),
];

/// 目录右键菜单：只提供“添加到压缩包”。
const DIR_MENUS: [(&str, &str, &str); 1] = [(
    r"Software\Classes\Directory\shell\SpellArchive.AddToZip",
    "添加到 zip 压缩包",
    "--add",
)];

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

/// 已安装右键菜单时，把注册表里的 exe 路径刷新成当前进程路径。
#[cfg(windows)]
pub fn refresh_if_installed() -> io::Result<()> {
    if !is_installed() {
        return Ok(());
    }
    let exe = std::env::current_exe()?;
    install(&exe)
}

#[cfg(windows)]
pub fn install(exe: &Path) -> io::Result<()> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let icon = format!("\"{}\"", exe.display());
    for (base, label, flag) in FILE_MENUS.iter().chain(DIR_MENUS.iter()) {
        let (key, _) = hkcu.create_subkey(base)?;
        key.set_value("", &label.to_string())?;
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
pub fn refresh_if_installed() -> io::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
pub fn install(_exe: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
pub fn uninstall() -> io::Result<()> {
    Ok(())
}
