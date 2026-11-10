//! Per-user installation used by the setup wizard and launcher uninstall action.
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn data_dir() -> Result<PathBuf> {
    Ok(std::env::var_os("XDG_DATA_HOME")
        .filter(|path| Path::new(path).is_absolute())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .context("can't find the user's application folder")?)
}
pub fn program() -> Result<PathBuf> {
    Ok(data_dir()?.join("Carryover/copyblip"))
}
pub fn needed() -> bool {
    match (std::env::current_exe(), program()) {
        (Ok(current), Ok(target)) => current.canonicalize().ok() != target.canonicalize().ok(),
        _ => true,
    }
}
/// Quotes an Exec argument, including the desktop-file string escaping layer.
pub fn exec_argument(path: &Path) -> Result<String> {
    let text = path.to_str().context("executable path isn't UTF-8")?;
    anyhow::ensure!(
        !text.contains(['\n', '\r', '\t']),
        "executable path contains a control character"
    );
    let mut result = String::new();
    for ch in text.chars() {
        if matches!(ch, '\\' | '"' | '`' | '$') {
            result.push('\\');
        }
        result.push(ch);
        if ch == '%' {
            result.push('%');
        }
    }
    Ok(format!("\"{}\"", result.replace('\\', "\\\\")))
}
pub fn install_from(source: &Path) -> Result<PathBuf> {
    install_from_in(source, &crate::config::default_dir()?)
}

pub fn install_from_in(source: &Path, dir: &Path) -> Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let target = program()?;
    let folder = target.parent().unwrap();
    std::fs::create_dir_all(folder)?;
    if source.canonicalize()? != target.canonicalize().unwrap_or_default() {
        let staged = folder.join(format!(".copyblip-{}", rand::random::<u64>()));
        let result = (|| -> Result<()> {
            std::fs::copy(source, &staged)?;
            std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))?;
            std::fs::rename(&staged, &target)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&staged);
        }
        result.context("couldn't install Carryover")?;
    }
    let data = data_dir()?;
    let applications = data.join("applications");
    let icons = data.join("icons/hicolor/256x256/apps");
    std::fs::create_dir_all(&applications)?;
    std::fs::create_dir_all(&icons)?;
    std::fs::write(
        icons.join("carryover.png"),
        include_bytes!("../../assets/icon.png"),
    )?;
    let command = exec_argument(&target)?;
    let profile = exec_argument(dir)?;
    std::fs::write(
        applications.join("com.coledeterman.carryover.desktop"),
        format!(
            "[Desktop Entry]\nType=Application\nName=Carryover\nComment=Shared clipboard across your devices\nExec={command} --config-dir {profile}\nIcon=carryover\nTerminal=false\nCategories=Utility;\nActions=Uninstall;\n\n[Desktop Action Uninstall]\nName=Uninstall Carryover\nExec={command} uninstall --config-dir {profile}\n"
        ),
    )?;
    // Backward compatibility with the first Linux archive.
    std::fs::write(
        folder.join("uninstall.sh"),
        format!("#!/bin/sh\nexec \"$(dirname -- \"$0\")/copyblip\" uninstall\n"),
    )?;
    refresh_launchers(&applications);
    Ok(target)
}
pub fn uninstall() -> Result<()> {
    crate::install::stop_running();
    remove_files()
}

fn remove_files() -> Result<()> {
    crate::autostart::set_enabled(false)?;
    let data = data_dir()?;
    for path in [
        program()?,
        data.join("Carryover/uninstall.sh"),
        data.join("applications/com.coledeterman.carryover.desktop"),
        data.join("icons/hicolor/256x256/apps/carryover.png"),
    ] {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let _ = std::fs::remove_dir(data.join("Carryover"));
    refresh_launchers(&data.join("applications"));
    Ok(())
}
fn refresh_launchers(applications: &Path) {
    let _ = std::process::Command::new("update-desktop-database")
        .arg(applications)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn install_login_upgrade_and_uninstall_preserve_group() {
        if std::env::var_os("CARRYOVER_LIFECYCLE_CHILD").is_some() {
            let source = std::env::current_exe().unwrap();
            let target = install_from(&source).unwrap();
            assert_eq!(
                std::fs::read(&source).unwrap(),
                std::fs::read(&target).unwrap()
            );
            let data = data_dir().unwrap();
            let desktop = std::fs::read_to_string(
                data.join("applications/com.coledeterman.carryover.desktop"),
            )
            .unwrap();
            assert!(desktop.contains("Actions=Uninstall;") && desktop.contains("%%"));
            let dir = crate::config::default_dir().unwrap();
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("config.json"), "retained group").unwrap();
            std::fs::create_dir_all(dir.join("history")).unwrap();
            std::fs::write(dir.join("history/clip"), "retained history").unwrap();
            crate::autostart::set_enabled_for_in(&target, true, &dir).unwrap();
            assert!(crate::autostart::is_enabled());
            let login = std::fs::read_to_string(
                dir.parent()
                    .unwrap()
                    .join("autostart/com.coledeterman.carryover.desktop"),
            )
            .unwrap();
            assert!(login.contains("--config-dir") && login.contains("%%"));
            install_from(&source).unwrap();
            remove_files().unwrap();
            assert!(!target.exists() && !crate::autostart::is_enabled());
            assert!(
                !data
                    .join("applications/com.coledeterman.carryover.desktop")
                    .exists()
            );
            assert_eq!(
                std::fs::read_to_string(dir.join("config.json")).unwrap(),
                "retained group"
            );
            assert_eq!(
                std::fs::read_to_string(dir.join("history/clip")).unwrap(),
                "retained history"
            );
            remove_files().unwrap();
            return;
        }
        let root = std::env::temp_dir().join(format!(
            "carryover lifecycle 100% \"$`-{}",
            rand::random::<u64>()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "linux_install::tests::install_login_upgrade_and_uninstall_preserve_group",
                "--nocapture",
            ])
            .env("CARRYOVER_LIFECYCLE_CHILD", "1")
            .env("HOME", &root)
            .env("XDG_DATA_HOME", root.join("data"))
            .env("XDG_CONFIG_HOME", root.join("config"))
            .status()
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
        assert!(result.success());
    }
}
