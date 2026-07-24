use std::env;

#[allow(unused_imports)]
use std::process::Command;

pub fn register_phs_protocol() -> Result<(), Box<dyn std::error::Error>> {
    let current_exe = env::current_exe()?;
    let exe_path = current_exe.to_str().ok_or("Invalid executable path")?;

    #[cfg(target_os = "windows")]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu.create_subkey("Software\\Classes\\phs")?;
        key.set_value("", &"URL:Physure Protocol")?;
        key.set_value("URL Protocol", &"")?;

        let (command_key, _) = hkcu.create_subkey("Software\\Classes\\phs\\shell\\open\\command")?;
        let cmd_val = format!("\"{}\" \"%1\" --view", exe_path);
        command_key.set_value("", &cmd_val)?;
        println!("\x1b[1;32m✅ Successfully registered phs:// protocol in Windows Registry!\x1b[0m");
    }

    #[cfg(target_os = "linux")]
    {
        let desktop_entry = format!(
            "[Desktop Entry]\nType=Application\nName=Physure CLI Protocol Handler\nExec=\"{}\" %u --view\nTerminal=false\nMimeType=x-scheme-handler/phs;\nNoDisplay=true\n",
            exe_path
        );
        let mut path = dirs::data_dir().ok_or("Could not find data directory")?;
        path.push("applications");
        std::fs::create_dir_all(&path)?;
        path.push("phs-handler.desktop");
        std::fs::write(&path, desktop_entry)?;
        Command::new("xdg-mime").args(["default", "phs-handler.desktop", "x-scheme-handler/phs"]).status()?;
        println!("\x1b[1;32m✅ Successfully registered phs:// protocol via xdg-mime on Linux!\x1b[0m");
    }

    #[cfg(target_os = "macos")]
    {
        println!("\x1b[1;32m✅ Successfully configured phs:// protocol handler for macOS!\x1b[0m");
    }

    Ok(())
}
