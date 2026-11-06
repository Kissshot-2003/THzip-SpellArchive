use std::io;
use std::path::Path;

const BASES: [&str; 2] = [
    r"Software\Classes\*\shell\SpellArchive",
    r"Software\Classes\Directory\shell\SpellArchive",
];

#[cfg(windows)]
pub fn is_installed() -> bool {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(BASES[0])
        .is_ok()
}

#[cfg(windows)]
pub fn install(exe: &Path) -> io::Result<()> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let icon = format!("\"{}\"", exe.display());
    let command = format!("\"{}\" \"%1\"", exe.display());

    for base in BASES {
        let (key, _) = hkcu.create_subkey(base)?;
        key.set_value("", &"用 SpellArchive 打开")?;
        key.set_value("Icon", &icon)?;
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
    for base in BASES {
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
pub fn install(_exe: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
pub fn uninstall() -> io::Result<()> {
    Ok(())
}
