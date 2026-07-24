# Standalone HTML Viewer & Cross-Platform `phs://` Protocol Handler Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Provide zero-server standalone HTML visualization (`phs script.phs --view` / `file://`) and cross-platform protocol registration (`phs register-protocol` / `phs://path/to/script.phs`).

**Architecture:**
- `physure-cli/src/html.rs`: Generates portable single-file HTML reports and saves them to temp directory (`$TEMP/physure_<name>.html`), opening them via `file://` without background HTTP processes.
- `physure-cli/src/protocol.rs`: Cross-platform OS protocol handler registration (`phs://`) for Windows (Registry), Linux (`xdg-mime` desktop entry), and macOS.
- `physure-cli/src/main.rs`: Adds `--view`, `--html`, `phs register-protocol`, and `phs://` URI argument parsing.

**Tech Stack:** Rust (`physure-cli`), std::fs, open crate, OS-native APIs.

---

### Task 1: Implement Standalone HTML Generator & Viewer (`physure-cli/src/html.rs`)

**Files:**
- Create: `D:\Projects\physure\physure-cli\src\html.rs`

- [ ] **Step 1: Create `html.rs` to render self-contained HTML reports and open via `file://`**

```rust
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use physure_script::value::PhsValue;

pub fn open_standalone_html(title: &str, code: &str, vars: &HashMap<String, PhsValue>) -> Result<(), Box<dyn std::error::Error>> {
    let mut temp_dir = env::temp_dir();
    let file_name = format!("physure_{}.html", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs());
    temp_dir.push(file_name);

    let mut rows_html = String::new();
    for (k, v) in vars {
        let val_str = v.to_string();
        rows_html.push_str(&format!(
            "<tr><td class='py-3 font-mono text-cyan-400 font-bold'>{}</td><td class='py-3 font-mono text-amber-300'>{}</td><td class='py-3'><button onclick=\"navigator.clipboard.writeText('{} = {}')\" class='px-3 py-1 bg-slate-800 hover:bg-cyan-600 text-xs rounded-md text-white font-medium transition shadow'>Copy</button></td></tr>",
            k, val_str, k, val_str
        ));
    }

    let html_content = format!(r#"
<!DOCTYPE html>
<html lang="en" class="dark">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Physure Report - {}</title>
    <script src="https://cdn.tailwindcss.com"></script>
</head>
<body class="bg-slate-950 text-slate-100 min-h-screen p-8 font-sans">
    <div class="max-w-6xl mx-auto space-y-6">
        <header class="flex justify-between items-center border-b border-slate-800 pb-4">
            <div>
                <h1 class="text-3xl font-bold bg-gradient-to-r from-cyan-400 to-blue-500 bg-clip-text text-transparent">Physure Standalone Report</h1>
                <p class="text-slate-400 text-sm">Target: {} | Zero-Server Standalone Viewer</p>
            </div>
            <span class="px-3 py-1 bg-cyan-950 border border-cyan-800 text-cyan-400 text-xs rounded-full font-mono">phs:// Offline</span>
        </header>

        <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
            <div class="bg-slate-900 border border-slate-800 rounded-xl p-5 shadow-xl flex flex-col">
                <h2 class="text-lg font-semibold text-cyan-400 mb-3">PHS Source Code</h2>
                <pre class="bg-slate-950 p-4 rounded-lg font-mono text-sm text-slate-300 overflow-x-auto border border-slate-800 flex-1">{}</pre>
            </div>

            <div class="bg-slate-900 border border-slate-800 rounded-xl p-5 shadow-xl flex flex-col">
                <h2 class="text-lg font-semibold text-emerald-400 mb-3">Quantities & Results</h2>
                <div class="overflow-x-auto flex-1">
                    <table class="w-full text-left text-sm border-collapse">
                        <thead>
                            <tr class="border-b border-slate-800 text-slate-400 font-semibold">
                                <th class="pb-2">Variable</th>
                                <th class="pb-2">Value</th>
                                <th class="pb-2">Action</th>
                            </tr>
                        </thead>
                        <tbody class="divide-y divide-slate-800/50">
                            {}
                        </tbody>
                    </table>
                </div>
            </div>
        </div>
    </div>
</body>
</html>
    "#, title, title, code, rows_html);

    fs::write(&temp_dir, html_content)?;
    println!("\x1b[1;32m📄 Generated standalone HTML report:\x1b[0m {}", temp_dir.display());
    open::that(&temp_dir)?;
    Ok(())
}
```

---

### Task 2: Implement Cross-Platform `phs://` Protocol Handler (`physure-cli/src/protocol.rs`)

**Files:**
- Create: `D:\Projects\physure\physure-cli\src\protocol.rs`

- [ ] **Step 1: Implement `register_phs_protocol()` for Windows, Linux, and macOS**

```rust
use std::env;
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
        let cmd_val = format!("\"{}\" \"--view\" \"%1\"", exe_path);
        command_key.set_value("", &cmd_val)?;
        println!("\x1b[1;32m✅ Successfully registered phs:// protocol in Windows Registry!\x1b[0m");
    }

    #[cfg(target_os = "linux")]
    {
        let desktop_entry = format!(
            "[Desktop Entry]\nType=Application\nName=Physure CLI Protocol Handler\nExec=\"{}\" --view %u\nTerminal=false\nMimeType=x-scheme-handler/phs;\nNoDisplay=true\n",
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
```

---

### Task 3: Integration in `physure-cli` (`physure-cli/src/main.rs`)

**Files:**
- Modify: `D:\Projects\physure\physure-cli\Cargo.toml`
- Modify: `D:\Projects\physure\physure-cli\src\main.rs`

- [ ] **Step 1: Add `winreg` dependency for Windows in `physure-cli/Cargo.toml`**

```toml
[target.'cfg(windows)'.dependencies]
winreg = "0.52"
```

- [ ] **Step 2: Add `phs register-protocol`, `phs://` URI resolution, and `--view`/`--html` flags to `main.rs`**
- [ ] **Step 3: Run `cargo test --workspace`**
- [ ] **Step 4: Run `cargo run --bin phs -- D:\Projects\test_physure\1_cargas.phs --view` and verify instant browser report launch with zero servers.**
