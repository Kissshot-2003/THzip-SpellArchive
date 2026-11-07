fn main() {
    println!("cargo:rerun-if-changed=icon.ico");
    println!("cargo:rerun-if-changed=app.manifest");

    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("icon.ico");
        resource.set_manifest_file("app.manifest");
        resource.set("FileDescription", "SpellArchive");
        resource.set("ProductName", "SpellArchive");
        resource.set("LegalCopyright", "MIT / assets CC BY-NC-SA 4.0");
        resource.set("FileVersion", env!("CARGO_PKG_VERSION"));
        resource.set("ProductVersion", env!("CARGO_PKG_VERSION"));
        if let Err(err) = resource.compile() {
            panic!("嵌入 Windows 资源失败: {err}");
        }
    }
}
