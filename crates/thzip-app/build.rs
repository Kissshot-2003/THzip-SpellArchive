use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=icon.ico");
    println!("cargo:rerun-if-changed=app.manifest");

    #[cfg(windows)]
    {
        // winresource 通过注册表定位 Windows SDK（内部调用 reg.exe）。受限环境
        // —— 沙箱、域策略禁用 reg.exe、精简 CI 镜像 —— 下这一步会失败，错误信息
        // 只剩「系统找不到指定的路径」，很难定位。先自己探测 rc.exe 并设好
        // RC_PATH，winresource 的 SDK 查询就会直接走这个环境变量，绕开注册表。
        if let Some(rc) = find_rc_exe() {
            // SAFETY: build 脚本单线程执行，设置进程内环境变量没有并发风险。
            unsafe { std::env::set_var("RC_PATH", &rc) };
            println!("cargo:warning=使用探测到的资源编译器 {}", rc.display());
        }

        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("icon.ico");
        resource.set_manifest_file("app.manifest");
        resource.set("FileDescription", "THzip-SpellArchive");
        resource.set("ProductName", "THzip-SpellArchive");
        resource.set("LegalCopyright", "MIT / assets CC BY-NC-SA 4.0");
        resource.set("FileVersion", env!("CARGO_PKG_VERSION"));
        resource.set("ProductVersion", env!("CARGO_PKG_VERSION"));
        if let Err(err) = resource.compile() {
            panic!(
                "嵌入 Windows 资源失败: {err}\n\
                 请确认已安装 Windows SDK；也可手动指定：\n\
                 set RC_PATH=C:\\Program Files (x86)\\Windows Kits\\10\\bin\\<版本>\\x64\\rc.exe"
            );
        }
    }
}

/// 定位资源编译器 rc.exe。
///
/// 优先用 RC_PATH 环境变量，其次按 Windows SDK 的标准安装位置探测版本目录，
/// 取版本号最大的一个。都找不到就返回 None，让 winresource 走注册表。
#[cfg(windows)]
fn find_rc_exe() -> Option<PathBuf> {
    if let Ok(rc) = std::env::var("RC_PATH") {
        let path = PathBuf::from(rc);
        if path.is_file() {
            return Some(path);
        }
    }

    for root in [
        r"C:\Program Files (x86)\Windows Kits\10\bin",
        r"C:\Program Files\Windows Kits\10\bin",
    ] {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        let mut versions: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        // 版本号位数相同时字典序与大小序一致，取最大的一个。
        versions.sort();
        for dir in versions.into_iter().rev() {
            let candidate = dir.join("x64").join("rc.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(not(windows))]
fn find_rc_exe() -> Option<PathBuf> {
    None
}
