//! EXMT Modbox — Tauri backend.
//!
//! The shell is thin: window chrome is drawn by the frontend. Asset work runs
//! in-process through `exmt-core` where the Rust pipeline is complete
//! (`skinpack` today), and everything else still goes through the Python core
//! (`core/exmt_core`) until it is ported.

pub mod analyze;
pub mod exmtmesh;
pub mod interchange;
pub mod modpack;
pub mod preview;
pub mod skinpack;
pub mod store;
pub mod workspace;

use std::path::{Path, PathBuf};
use std::process::Command;

use exmt_core::asset;
use exmt_core::texconv::Texconv;
use serde::Serialize;
use tauri::{LogicalSize, Manager, PhysicalPosition, PhysicalSize};

/// Default window footprint, as ratios of the monitor's work area (taskbar
/// excluded). The reference layout is 1920×1080: the window keeps that 16:9
/// shape at roughly two thirds of the screen, so a 4K panel gets a bigger
/// window than a laptop without either being hard-coded in pixels.
const WINDOW_ASPECT: f64 = 16.0 / 9.0;
const WINDOW_WIDTH_SHARE: f64 = 2.0 / 3.0;
const WINDOW_HEIGHT_LIMIT: f64 = 0.9;
/// Must stay in sync with `minWidth` / `minHeight` in `tauri.conf.json`.
const WINDOW_MIN_WIDTH: f64 = 920.0;
const WINDOW_MIN_HEIGHT: f64 = 600.0;
const WINDOW_FALLBACK: (f64, f64) = (1120.0, 630.0);

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
fn run_core_blocking(
    python: String,
    core_dir: String,
    args: Vec<String>,
) -> Result<CoreOutput, String> {
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

/// Locate `texconv.exe`: env override, bundled resource, then the dev tree.
///
/// The resource directory exists only in a packaged app; `tauri dev` runs the
/// binary from `target/debug`, so the in-repo copy is the dev fallback.
fn texconv_for(app: &tauri::AppHandle) -> Option<Texconv> {
    if let Ok(path) = std::env::var("EXMT_TEXCONV") {
        if Path::new(&path).is_file() {
            return Some(Texconv::new(path));
        }
    }
    if let Ok(dir) = app.path().resource_dir() {
        let bundled = dir.join("bin").join("texconv.exe");
        if bundled.is_file() {
            return Some(Texconv::new(bundled));
        }
    }
    let dev: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("bin").join("texconv.exe");
    if dev.is_file() {
        Some(Texconv::new(dev))
    } else {
        None
    }
}

/// Commands the Rust core implements end to end.
///
/// Returning `None` falls back to the Python process, which keeps the app
/// working on a checkout where `texconv.exe` is missing.
fn run_rust_core(app: &tauri::AppHandle, args: &[String]) -> Option<CoreOutput> {
    let spec = args.get(1)?.clone();
    match args.first().map(String::as_str) {
        Some("skinpack") => {
            let tool = texconv_for(app)?;
            Some(match skinpack::run(Path::new(&spec), &tool) {
                Ok(stdout) => CoreOutput { code: 0, stdout, stderr: String::new() },
                Err(e) => CoreOutput { code: 1, stdout: String::new(), stderr: e },
            })
        }
        Some("modpack") => match modpack::run(Path::new(&spec), true) {
            Ok(stdout) => Some(CoreOutput { code: 0, stdout, stderr: String::new() }),
            Err(modpack::RunError::Failed(e)) => {
                Some(CoreOutput { code: 1, stdout: String::new(), stderr: e })
            }
        },
        Some("skinfo") => {
            let bundle = asset::open(&args[1]).ok()?;
            Some(match preview::skinfo(&bundle) {
                Ok(stdout) => CoreOutput { code: 0, stdout, stderr: String::new() },
                Err(e) => CoreOutput { code: 1, stdout: String::new(), stderr: e },
            })
        }
        Some("thumbs") => {
            let tool = texconv_for(app)?;
            let size = args.get(2)?.parse::<usize>().ok()?;
            let bundle = asset::open(&args[1]).ok()?;
            Some(match preview::thumbs(&bundle, size, &args[3..], &tool) {
                Ok(stdout) => CoreOutput { code: 0, stdout, stderr: String::new() },
                Err(e) => CoreOutput { code: 1, stdout: String::new(), stderr: e },
            })
        }
        Some("backup") | Some("restore") | Some("backup-size") => {
            Some(match store::run(args) {
                Ok(stdout) => CoreOutput { code: 0, stdout, stderr: String::new() },
                Err(e) => CoreOutput { code: 1, stdout: String::new(), stderr: e },
            })
        }
        Some("workspace") => {
            let outdir = args.get(1)?;
            let spec_path = args.get(2)?;
            let tool = texconv_for(app)?;
            Some(match workspace::run(Path::new(outdir), Path::new(spec_path), &tool) {
                Ok(stdout) => CoreOutput { code: 0, stdout, stderr: String::new() },
                Err(e) => CoreOutput { code: 1, stdout: String::new(), stderr: e },
            })
        }
        _ => None,
    }
}

/// Async wrapper so a long run never blocks the window's event loop.
///
/// A synchronous `#[tauri::command]` runs on the main thread, and the whole app
/// looks frozen while the core parses bundles; `spawn_blocking` moves the work
/// to a worker thread instead.
#[tauri::command]
async fn run_core(
    app: tauri::AppHandle,
    python: String,
    core_dir: String,
    args: Vec<String>,
) -> Result<CoreOutput, String> {
    // Commands with a Rust implementation run here; everything else (and any
    // spec Rust deliberately declines) falls through to the Python core.
    let rust = {
        let app = app.clone();
        let args = args.clone();
        tauri::async_runtime::spawn_blocking(move || run_rust_core(&app, &args))
            .await
            .map_err(|e| format!("core task failed: {e}"))?
    };
    if let Some(out) = rust {
        return Ok(out);
    }
    tauri::async_runtime::spawn_blocking(move || run_core_blocking(python, core_dir, args))
        .await
        .map_err(|e| format!("core task failed: {e}"))?
}

/// Reveal the main window once web content is ready.
#[tauri::command]
fn show_main_window(app: tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.set_focus();
    }
}

/// Pick a widescreen default size for a monitor whose work area is
/// `work_w × work_h` logical pixels. Pure, so the curve is unit-testable.
fn default_window_size(work_w: f64, work_h: f64) -> (f64, f64) {
    if !(work_w > 0.0 && work_h > 0.0) {
        return WINDOW_FALLBACK;
    }

    let mut width = work_w * WINDOW_WIDTH_SHARE;
    let mut height = width / WINDOW_ASPECT;

    // Ultrawide or short screens: trade width for a window that clears the
    // taskbar instead of growing taller than the work area.
    if height > work_h * WINDOW_HEIGHT_LIMIT {
        height = work_h * WINDOW_HEIGHT_LIMIT;
        width = height * WINDOW_ASPECT;
    }

    // Keep the window usable on small panels, but never larger than the work area.
    if width < WINDOW_MIN_WIDTH {
        width = WINDOW_MIN_WIDTH;
        height = width / WINDOW_ASPECT;
    }
    if height < WINDOW_MIN_HEIGHT {
        height = WINDOW_MIN_HEIGHT;
        width = height * WINDOW_ASPECT;
    }

    (width.min(work_w), height.min(work_h))
}

/// Size and centre the main window inside its monitor's work area.
fn fit_main_window(window: &tauri::WebviewWindow) {
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };

    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let (width, height) = default_window_size(
        area.size.width as f64 / scale,
        area.size.height as f64 / scale,
    );

    let _ = window.set_size(LogicalSize::new(width.round(), height.round()));

    // Centre the visible frame, not the client area: Windows keeps a few
    // invisible resize pixels around a decoration-less window.
    let outer = window.outer_size().unwrap_or(PhysicalSize::new(
        (width * scale).round() as u32,
        (height * scale).round() as u32,
    ));
    // Centre against the work area, not the raw screen: that keeps the window
    // clear of the taskbar and on the right monitor in multi-display setups.
    let x = area.position.x as f64 + (area.size.width as f64 - outer.width as f64) / 2.0;
    let y = area.position.y as f64 + (area.size.height as f64 - outer.height as f64) / 2.0;
    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        // NOTE: tauri-plugin-http is deliberately absent. Its cookie store calls
        // `app_cache_dir()` + opens a cookie file during plugin setup, and on this
        // machine that setup fails with `os error 5 (access denied)` and aborts the
        // whole app before any window appears. The frontend never used the plugin
        // (no fetch / plugin-http import anywhere), so removing it is the fix.
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                fit_main_window(&window);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![show_main_window, run_core])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_aspect(width: f64, height: f64) {
        assert!(
            (width / height - WINDOW_ASPECT).abs() < 0.01,
            "not 16:9: {width}×{height}"
        );
    }

    #[test]
    fn follows_the_screen_but_stays_inside_it() {
        // (work area in logical px, expected window): 16:9 at two thirds wide
        let screens = [
            ((1920.0, 1040.0), (1280.0, 720.0)),
            ((1707.0, 1019.0), (1138.0, 640.0)),
            ((2560.0, 1400.0), (1707.0, 960.0)),
            ((1366.0, 728.0), (1067.0, 600.0)),
        ];
        for ((work_w, work_h), (want_w, want_h)) in screens {
            let (width, height) = default_window_size(work_w, work_h);
            assert!(
                (width - want_w).abs() <= 2.0 && (height - want_h).abs() <= 2.0,
                "{work_w}×{work_h} gave {width}×{height}, wanted {want_w}×{want_h}"
            );
            assert_aspect(width, height);
            assert!(width <= work_w && height <= work_h);
        }
    }

    #[test]
    fn ultrawide_is_height_limited() {
        let (width, height) = default_window_size(3440.0, 1400.0);
        assert!(height <= 1400.0 * WINDOW_HEIGHT_LIMIT + 0.5);
        assert!(width < 3440.0 * WINDOW_WIDTH_SHARE);
        assert_aspect(width, height);
    }

    #[test]
    fn small_panels_keep_the_minimum() {
        let (width, height) = default_window_size(1024.0, 600.0);
        assert!(width >= 960.0 && height >= 600.0);
        assert!(width <= 1024.0 && height <= 600.0);
    }

    #[test]
    fn unknown_monitor_falls_back() {
        assert_eq!(default_window_size(0.0, 0.0), WINDOW_FALLBACK);
        assert_eq!(default_window_size(f64::NAN, 1080.0), WINDOW_FALLBACK);
    }
}
