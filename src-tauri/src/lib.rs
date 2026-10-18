//! EXMT Modbox — Tauri backend.
//!
//! The shell is thin: window chrome is drawn by the frontend, and the asset
//! pipeline lives in the Python core (`core/exmt_core`). This module only
//! exposes a guarded process bridge so the UI can drive that core.

use std::process::Command;

use serde::Serialize;
use tauri::Manager;

#[derive(Serialize)]
struct CoreOutput {
    code: i32,
    stdout: String,
    stderr: String,
}

/// Run `python -m exmt_core <args…>` inside the core directory.
///
/// The GUI never shells out to arbitrary commands: the caller selects a Python
/// interpreter and a core directory (both explicit), and the argument vector is
/// passed through without a shell, so nothing is re-interpreted.
#[tauri::command]
fn run_core(python: String, core_dir: String, args: Vec<String>) -> Result<CoreOutput, String> {
    if !std::path::Path::new(&core_dir).is_dir() {
        return Err(format!("core directory not found: {core_dir}"));
    }
    let mut cmd = Command::new(&python);
    cmd.arg("-m").arg("exmt_core").args(&args).current_dir(&core_dir);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().map_err(|e| format!("failed to start python: {e}"))?;
    Ok(CoreOutput {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// Reveal the main window once web content is ready.
#[tauri::command]
fn show_main_window(app: tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![show_main_window, run_core])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
