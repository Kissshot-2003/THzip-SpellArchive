use std::io;
use std::path::Path;

use crate::i18n::{Locale, TextKey, builtin};

/// 一个右键菜单动词：注册表键后缀、显示名文案、命令行开关（空表示直接打开）。
struct Verb {
    /// 键名后缀，菜单里的排序靠它。
    suffix: &'static str,
    /// 显示名文案。
    text: TextKey,
    /// 命令行开关；空表示「用本程序打开」。
    flag: &'static str,
}

/// 文件上的动词：打开 / 解压到当前文件夹 / 解压到单独文件夹 / 添加到 zip。
///
/// 只在「确实是归档」的目标上注册——`Open` 会出现在任何文件上，
/// 另外三个动词必须先确认目标是个包，否则用户右键一个 .txt 会看到
/// 「解压到当前文件夹」，点下去只会报错。
const FILE_VERBS: [Verb; 4] = [
    Verb {
        suffix: "Open",
        text: TextKey::ShellOpen,
        flag: "",
    },
    Verb {
        suffix: "ExtractHere",
        text: TextKey::ShellExtractHere,
        flag: "--extract-here",
    },
    Verb {
        suffix: "ExtractTo",
        text: TextKey::ShellExtractTo,
        flag: "--extract-to",
    },
    Verb {
        suffix: "AddToZip",
        text: TextKey::ShellAddToZip,
        flag: "--add",
    },
];

/// 目录上的动词：只有「添加到 zip」。
const DIR_VERBS: [Verb; 1] = [Verb {
    suffix: "AddToZip",
    text: TextKey::ShellAddToZip,
    flag: "--add",
}];

/// 本程序能处理的归档扩展名。右键菜单只在这些扩展名上出现，
/// 否则右键任何一个 .txt 都会挂上四个无关动词。
///
/// 与 `OPEN_WITH_EXTENSIONS` 的区别：这里只列「真的是包」的格式。
/// `.gz` / `.bz2` / `.xz` / `.zst` / `.lz4` 单独看是压缩流，
/// 但它们也可能是 `.tar.gz` 这种包的一部分，所以仍然列进来——
/// 程序在运行时会用魔数二次确认，识别成 tar 家族照样能解压。
const ARCHIVE_EXTENSIONS: [&str; 12] = [
    ".zip", ".7z", ".rar", ".tar", ".gz", ".tgz", ".bz2", ".tbz2", ".xz", ".txz", ".zst", ".lz4",
];

/// 「打开方式」用的 ProgID。只登记 OpenWithProgids，不改各扩展名的默认值，
/// 因此不会抢走用户当前的默认程序，只是在列表里多一个候选。
const PROGID: &str = "THzip-SpellArchive.Archive";
const OPEN_WITH_EXTENSIONS: [&str; 10] = [
    ".zip", ".7z", ".rar", ".tar", ".gz", ".tgz", ".bz2", ".xz", ".zst", ".lz4",
];
const APP_NAME: &str = "THzip-SpellArchive";

/// 菜单动词在注册表里的公共前缀。用 `THzip-SpellArchive.` 而不是裸
/// `THzip-SpellArchive`，避免和同名 ProgID 撞在同一层。
const VERB_PREFIX: &str = "THzip-SpellArchive";

/// Win11 恢复 Win10 经典右键菜单用的 CLSID 掩码。
///
/// Win11 的紧凑菜单只显示通过 `IExplorerCommand` + 应用标识注册的命令，
/// 纯注册表动词一律被塞进「显示更多选项」。这个 CLSID 的
/// `InprocServer32` 留空即可让资源管理器退回 Win10 风格的菜单，
/// 所有第三方动词重新回到一级菜单。
///
/// 写在 HKCU 下，只影响当前用户，不需要管理员权限。
const CLASSIC_MENU_CLSID: &str = r"Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}";

#[cfg(windows)]
pub fn is_installed() -> bool {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    archive_extensions().any(|ext| {
        let base = format!(r"Software\Classes\{ext}\shell\{VERB_PREFIX}.Open");
        hkcu.open_subkey(base).is_ok()
    })
}

/// Win11 经典菜单掩码当前是否处于「已恢复」状态。
#[cfg(windows)]
pub fn classic_menu_enabled() -> bool {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey(CLASSIC_MENU_CLSID)
        .and_then(|key| key.open_subkey("InprocServer32"))
        .is_ok()
}

/// 打开或关闭 Win11 经典右键菜单。
///
/// 打开：写一个空的 `InprocServer32` 值（内容必须严格为空，不能有空格）。
/// 关闭：删掉整个 CLSID 键。
///
/// 资源管理器只在启动时读一次这个掩码，所以改完需要重启 explorer.exe
/// 才生效——这里由调用方决定要不要重启。
#[cfg(windows)]
pub fn set_classic_menu(enabled: bool) -> io::Result<bool> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if enabled {
        let (clsid, _) = hkcu.create_subkey(CLASSIC_MENU_CLSID)?;
        let (inproc, _) = clsid.create_subkey("InprocServer32")?;
        // 必须是空字符串。写成 " " 或 "none" 都不起作用。
        inproc.set_value("", &"")?;
        Ok(true)
    } else {
        match hkcu.delete_subkey_all(CLASSIC_MENU_CLSID) {
            Ok(()) => Ok(true),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(err) => Err(err),
        }
    }
}

/// 通知资源管理器关联关系变了，让右键菜单立刻重读注册表。
///
/// 写完注册表不通知的话，菜单要等到下次 explorer.exe 启动才更新，
/// 用户会觉得「开关点了没反应」。
#[cfg(windows)]
pub fn notify_shell_change() {
    use windows_sys::Win32::UI::Shell::SHCNE_ASSOCCHANGED;
    use windows_sys::Win32::UI::Shell::SHCNF_IDLIST;
    use windows_sys::Win32::UI::Shell::SHChangeNotify;

    unsafe {
        SHChangeNotify(
            SHCNE_ASSOCCHANGED as i32,
            SHCNF_IDLIST,
            std::ptr::null(),
            std::ptr::null(),
        );
    }
}

/// 所有会被注册菜单的扩展名。
fn archive_extensions() -> impl Iterator<Item = &'static str> {
    ARCHIVE_EXTENSIONS.into_iter()
}

/// 生成某个扩展名下的动词注册表键。
fn verb_key(ext: &str, verb: &Verb) -> String {
    format!(
        r"Software\Classes\{ext}\shell\{VERB_PREFIX}.{suffix}",
        suffix = verb.suffix
    )
}

/// 生成某个扩展名下的命令键。
fn verb_command_key(ext: &str, verb: &Verb) -> String {
    format!(r"{base}\command", base = verb_key(ext, verb))
}

/// 目录菜单的注册表键。目录没有扩展名，单独一套。
fn dir_verb_key(verb: &Verb) -> String {
    format!(
        r"Software\Classes\Directory\shell\{VERB_PREFIX}.{suffix}",
        suffix = verb.suffix
    )
}

fn dir_verb_command_key(verb: &Verb) -> String {
    format!(r"{base}\command", base = dir_verb_key(verb))
}

/// 文件夹空白处的菜单键。右键文件夹里的空白处和右键文件夹本身是两套键，
/// 只注册 `Directory\shell` 的话，空白处右键是看不到菜单的。
fn background_verb_key(verb: &Verb) -> String {
    format!(
        r"Software\Classes\Directory\Background\shell\{VERB_PREFIX}.{suffix}",
        suffix = verb.suffix
    )
}

fn background_verb_command_key(verb: &Verb) -> String {
    format!(r"{base}\command", base = background_verb_key(verb))
}

/// 旧名 SpellArchive 的菜单键，只用于一次性迁移。
const LEGACY_FILE_MENUS: [&str; 4] = [
    r"Software\Classes\*\shell\SpellArchive",
    r"Software\Classes\*\shell\SpellArchive.ExtractHere",
    r"Software\Classes\*\shell\SpellArchive.ExtractTo",
    r"Software\Classes\*\shell\SpellArchive.AddToZip",
];
const LEGACY_DIR_MENUS: [&str; 1] = [r"Software\Classes\Directory\shell\SpellArchive.AddToZip"];

/// 上一版把菜单挂在 `*\shell` 下（所有文件都能看到）。升级时清掉。
const LEGACY_STAR_MENUS: [&str; 4] = [
    r"Software\Classes\*\shell\THzip-SpellArchive",
    r"Software\Classes\*\shell\THzip-SpellArchive.ExtractHere",
    r"Software\Classes\*\shell\THzip-SpellArchive.ExtractTo",
    r"Software\Classes\*\shell\THzip-SpellArchive.AddToZip",
];

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

    // 每个归档扩展名一套菜单。放在扩展名下而不是 `*` 下，
    // 右键一个 .txt 不该看到「解压到当前文件夹」。
    for ext in archive_extensions() {
        for verb in FILE_VERBS.iter() {
            let (key, _) = hkcu.create_subkey(verb_key(ext, verb))?;
            key.set_value("", &builtin(locale, verb.text).to_string())?;
            key.set_value("Icon", &icon)?;
            // 多选时把全部选中项一起传过去。用 `%*` 而不是 `%1`，
            // 否则一次选中 20 个文件只能加进第一个。
            let entry_command = if verb.flag.is_empty() {
                format!("\"{}\" \"%*\"", exe.display())
            } else {
                format!("\"{}\" {} \"%*\"", exe.display(), verb.flag)
            };
            let (cmd, _) = hkcu.create_subkey(verb_command_key(ext, verb))?;
            cmd.set_value("", &entry_command)?;
            // 告诉资源管理器这个动词支持多选。
            key.set_value("MultiSelectModel", &"Player")?;
        }
    }

    // 目录菜单：右键文件夹时「添加到 zip」。
    for verb in DIR_VERBS.iter() {
        let (key, _) = hkcu.create_subkey(dir_verb_key(verb))?;
        key.set_value("", &builtin(locale, verb.text).to_string())?;
        key.set_value("Icon", &icon)?;
        let entry_command = format!("\"{}\" {} \"%*\"", exe.display(), verb.flag);
        let (cmd, _) = hkcu.create_subkey(dir_verb_command_key(verb))?;
        cmd.set_value("", &entry_command)?;
        key.set_value("MultiSelectModel", &"Player")?;

        // 文件夹空白处右键：把当前文件夹打成一个 zip。
        // 这时 %1 是文件夹本身，%V 是文件夹的路径，两种都得传。
        let (bg, _) = hkcu.create_subkey(background_verb_key(verb))?;
        bg.set_value("", &builtin(locale, verb.text).to_string())?;
        bg.set_value("Icon", &icon)?;
        let (bg_cmd, _) = hkcu.create_subkey(background_verb_command_key(verb))?;
        bg_cmd.set_value("", &format!("\"{}\" {} \"%V\"", exe.display(), verb.flag))?;
    }

    // 清理上一版挂在 `*\shell` 下的菜单：那些键现在会让所有文件
    // 都显示解压动词，留着纯属噪音。
    for base in LEGACY_STAR_MENUS.iter() {
        match hkcu.delete_subkey_all(base) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
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

    // 逐个删掉本程序在每个扩展名下建的动词。
    for ext in archive_extensions() {
        for verb in FILE_VERBS.iter() {
            for base in [verb_key(ext, verb), verb_command_key(ext, verb)] {
                match hkcu.delete_subkey_all(&base) {
                    Ok(()) => {}
                    Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                    Err(err) => return Err(err),
                }
            }
        }
    }
    for verb in DIR_VERBS.iter() {
        for base in [
            dir_verb_key(verb),
            dir_verb_command_key(verb),
            background_verb_key(verb),
            background_verb_command_key(verb),
        ] {
            match hkcu.delete_subkey_all(&base) {
                Ok(()) => {}
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => return Err(err),
            }
        }
    }

    // 旧版挂在 `*\shell` 下的菜单也要清。
    for base in LEGACY_STAR_MENUS.iter() {
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
pub fn classic_menu_enabled() -> bool {
    false
}

#[cfg(not(windows))]
pub fn set_classic_menu(_enabled: bool) -> io::Result<bool> {
    Ok(false)
}

#[cfg(not(windows))]
pub fn notify_shell_change() {}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_list_has_no_duplicates() {
        let mut seen = ARCHIVE_EXTENSIONS.to_vec();
        seen.sort_unstable();
        let count = seen.len();
        seen.dedup();
        assert_eq!(count, seen.len(), "扩展名列表里有重复项");
    }

    #[test]
    fn verb_keys_are_scoped_to_extension() {
        let verb = &FILE_VERBS[1];
        let key = verb_key(".zip", verb);
        assert!(key.contains(r"Software\Classes\.zip\shell"));
        assert!(key.contains("THzip-SpellArchive.ExtractHere"));
        // 关键：不能是 `*\shell`，否则所有文件都会看到解压菜单。
        assert!(!key.contains('*'));
    }

    #[test]
    fn every_extension_gets_every_verb() {
        for ext in archive_extensions() {
            for verb in FILE_VERBS.iter() {
                let key = verb_key(ext, verb);
                assert!(key.contains(ext), "{key} 少了扩展名");
            }
        }
    }

    #[test]
    fn dir_verbs_do_not_use_extension_keys() {
        let key = dir_verb_key(&DIR_VERBS[0]);
        assert!(key.contains(r"Software\Classes\Directory\shell"));
        assert!(!key.contains(r"\.zip"));
    }

    #[test]
    fn legacy_star_menus_are_scheduled_for_removal() {
        assert_eq!(LEGACY_STAR_MENUS.len(), 4);
        for base in LEGACY_STAR_MENUS {
            assert!(base.contains(r"\*\shell"), "{base}");
        }
    }

    /// 端到端验证：真的写一次注册表，检查布局，然后回滚。
    ///
    /// 只在 Windows 上跑。跑完必须恢复到验证前的状态——
    /// 测试会动用户真实的 HKCU，所以先记下 `.zip` 的默认 ProgID，
    /// 结束后比对，确认没有把用户原有的文件关联改坏。
    #[cfg(windows)]
    #[test]
    fn install_produces_expected_layout_and_preserves_associations() {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let exe = std::env::current_exe().unwrap();

        // 记住验证前的文件关联，安装后要能原样恢复。
        let zip_key = r"Software\Classes\.zip";
        let default_before: String = hkcu
            .open_subkey(zip_key)
            .and_then(|key| key.get_value(""))
            .unwrap_or_default();

        install(&exe, Locale::ZhCn).expect("install should succeed");

        // 1) 每个归档扩展名下四个动词齐全，且都支持多选。
        for ext in archive_extensions() {
            for verb in FILE_VERBS.iter() {
                let key = verb_key(ext, verb);
                let verb_key = hkcu
                    .open_subkey(&key)
                    .unwrap_or_else(|_| panic!("缺 {key}"));
                assert!(
                    !verb_key
                        .get_value::<String, _>("")
                        .unwrap_or_default()
                        .is_empty(),
                    "{key} 没有显示名"
                );
                assert_eq!(
                    verb_key
                        .get_value::<String, _>("MultiSelectModel")
                        .unwrap_or_default(),
                    "Player",
                    "{key} 缺 MultiSelectModel"
                );
                let command: String = hkcu
                    .open_subkey(verb_command_key(ext, verb))
                    .expect("缺命令键")
                    .get_value("")
                    .unwrap();
                assert!(command.contains("%*"), "{key} 应支持多选: {command}");
            }
        }

        // 2) 旧版挂在 `*` 下的动词必须已被清理，否则右键任何文件都看到解压菜单。
        for base in LEGACY_STAR_MENUS {
            assert!(
                hkcu.open_subkey(base).is_err(),
                "{base} 应已被清理，但仍然存在"
            );
        }

        // 3) 目录菜单在位：右键文件夹、以及右键文件夹里的空白处是两套键。
        assert!(
            hkcu.open_subkey(dir_verb_key(&DIR_VERBS[0])).is_ok(),
            "缺目录菜单"
        );
        assert!(
            hkcu.open_subkey(background_verb_key(&DIR_VERBS[0])).is_ok(),
            "缺文件夹空白处菜单"
        );
        // 空白处右键传的是 %V（文件夹路径），不是 %1。
        let background_command: String = hkcu
            .open_subkey(background_verb_command_key(&DIR_VERBS[0]))
            .expect("缺空白处命令键")
            .get_value("")
            .unwrap();
        assert!(
            background_command.contains("%V"),
            "空白处命令应传 %V: {background_command}"
        );

        // 4) 没有破坏用户原有的文件关联。
        let default_after: String = hkcu
            .open_subkey(zip_key)
            .and_then(|key| key.get_value(""))
            .unwrap_or_default();
        assert_eq!(
            default_before, default_after,
            "安装动到了 .zip 的默认 ProgID"
        );

        // 5) 卸载后彻底清干净，且同样不破坏关联。
        uninstall().expect("uninstall should succeed");
        for ext in archive_extensions() {
            for verb in FILE_VERBS.iter() {
                assert!(
                    hkcu.open_subkey(verb_key(ext, verb)).is_err(),
                    "卸载后 {} 仍在",
                    verb_key(ext, verb)
                );
            }
        }
        let default_final: String = hkcu
            .open_subkey(zip_key)
            .and_then(|key| key.get_value(""))
            .unwrap_or_default();
        assert_eq!(
            default_before, default_final,
            "卸载动到了 .zip 的默认 ProgID"
        );
    }

    /// 经典菜单掩码的开关必须真的能开能关，且只影响 HKCU。
    #[cfg(windows)]
    #[test]
    fn classic_menu_toggle_roundtrips() {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let was_on = classic_menu_enabled();

        set_classic_menu(true).expect("开启经典菜单失败");
        assert!(classic_menu_enabled(), "开启后状态仍为关");
        // InprocServer32 的值必须严格为空字符串。
        let value: String = hkcu
            .open_subkey(CLASSIC_MENU_CLSID)
            .and_then(|key| key.open_subkey("InprocServer32"))
            .and_then(|key| key.get_value(""))
            .unwrap_or_default();
        assert_eq!(value, "", "掩码值必须为空字符串");

        set_classic_menu(false).expect("关闭经典菜单失败");
        assert!(!classic_menu_enabled(), "关闭后状态仍为开");

        // 恢复测试前的状态。
        if was_on {
            set_classic_menu(true).unwrap();
        }
    }
}
