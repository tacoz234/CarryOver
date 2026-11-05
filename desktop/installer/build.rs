// Embeds the app icon, and checks that the programs to carry were given.
fn main() {
    println!("cargo:rerun-if-changed=../../assets/icon.ico");
    for var in ["CARRYOVER_EXE", "CARRYOVER_LOADER"] {
        println!("cargo:rerun-if-env-changed={var}");
        let path = std::env::var(var).unwrap_or_else(|_| {
            panic!("set {var}; build this with scripts/package-windows.ps1 or scripts/package-windows.sh")
        });
        println!("cargo:rerun-if-changed={path}");
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("../../assets/icon.ico");
        resource.set("ProductName", "Carryover");
        resource.set("FileDescription", "Carryover Setup");
        // Carryover's version, which this crate's own doesn't track.
        println!("cargo:rerun-if-changed=../Cargo.toml");
        let manifest = std::fs::read_to_string("../Cargo.toml").expect("no ../Cargo.toml");
        let version = manifest
            .lines()
            .find_map(|line| line.strip_prefix("version = \""))
            .and_then(|rest| rest.strip_suffix('"'))
            .expect("no version in ../Cargo.toml");
        resource.set("ProductVersion", version);
        resource.set("FileVersion", version);
        resource.compile().expect("couldn't embed the Windows icon");
    }
}
