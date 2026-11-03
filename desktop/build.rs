// Embeds the app icon in the Windows program, so it shows in File Explorer
// and Task Manager. Cross-compiling from macOS uses MinGW's windres.
fn main() {
    println!("cargo:rerun-if-changed=../assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("../assets/icon.ico");
        resource.set("ProductName", "Carryover");
        resource.set("FileDescription", "Carryover clipboard sync");
        resource.compile().expect("couldn't embed the Windows icon");
    }
}
