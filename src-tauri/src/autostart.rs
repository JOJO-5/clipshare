use std::path::Path;

pub const AUTOSTART_VALUE_NAME: &str = "ClipShare";

pub fn autostart_command(executable: &Path) -> String {
    format!("\"{}\" --minimized", executable.display())
}

#[cfg(windows)]
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_WRITE};
    use winreg::RegKey;

    let current_user = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = current_user
        .open_subkey_with_flags(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
            KEY_WRITE,
        )
        .map_err(|error| error.to_string())?;

    if enabled {
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        run_key
            .set_value(AUTOSTART_VALUE_NAME, &autostart_command(&executable))
            .map_err(|error| error.to_string())
    } else {
        match run_key.delete_value(AUTOSTART_VALUE_NAME) {
            Ok(()) | Err(_) => Ok(()),
        }
    }
}

#[cfg(not(windows))]
pub fn set_autostart(_enabled: bool) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autostart_command_quotes_executable_and_starts_minimized() {
        let executable = Path::new(r"C:\Program Files\ClipShare\clipshare.exe");

        assert_eq!(
            autostart_command(executable),
            r#""C:\Program Files\ClipShare\clipshare.exe" --minimized"#
        );
    }
}
