//! Fucha's Tauri backend: the download core runs in this process
//! ([`pump`]), the webview talks to it through commands, and the browser
//! extension and the command line through MDM's own command channel
//! ([`mdm_ipc`], sharing `%APPDATA%\misterdownloadmanager`).

mod pump;

use std::path::{Path, PathBuf};

use pump::{Call, Host, Msg, Pump};
use serde_json::Value;
use tauri::Manager;

/// Tauri's managed state: the core thread's handle, and the instance lock
/// that keeps a second Fucha (or Mister Download Manager) from writing the
/// same list.
struct AppState {
    host: Host,
    _instance: std::fs::File,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            mdm_call,
            settings_read,
            settings_write,
            pick_directory,
            open_external,
            reveal,
            browser_integration,
            download_dir,
            system_download_dir,
            ipc_path,
        ])
        .setup(|app| {
            // One instance, as in MDM: a second launch forwards its command
            // line to the running one and leaves.
            let Some(instance) = mdm_ipc::instance_lock() else {
                let args: Vec<String> = std::env::args().skip(1).collect();
                if let Some(cmd) = mdm_ipc::parse_args(&args) {
                    let _ = mdm_ipc::send(&cmd);
                }
                app.handle().exit(0);
                return Ok(());
            };

            let handle = tauri::async_runtime::handle().inner().clone();
            let (tx, rx) = std::sync::mpsc::channel::<Msg>();
            let (ipc_tx, ipc_rx) = std::sync::mpsc::channel::<mdm_ipc::Incoming>();
            // The command channel's thread answers "ok" at once; commands that
            // want an answer come here and are answered by the core thread.
            let forward = tx.clone();
            std::thread::Builder::new()
                .name("fucha-ipc".into())
                .spawn(move || {
                    while let Ok(incoming) = ipc_rx.recv() {
                        if forward.send(Msg::Incoming(incoming)).is_err() {
                            break;
                        }
                    }
                })?;
            let core = Pump::new(app.handle().clone(), handle);
            std::thread::Builder::new()
                .name("fucha-core".into())
                .spawn(move || core.run(rx))?;
            // The browser's native host and the CLI still reach Fucha: the
            // same `ipc.json`, the same one-line protocol. TODO: bundle
            // MDM's native messaging host (`mdm-nmhost`, crates/nmhost) with
            // the installer so the extension finds the channel without a
            // separate MDM install; until then the channel is there but
            // browsers have no host to reach it through.
            if let Err(error) = mdm_ipc::serve(ipc_tx, || {}) {
                eprintln!("浮槎命令通道启动失败：{error}");
            }

            app.manage(AppState {
                host: Host::new(tx),
                _instance: instance,
            });
            Ok(())
        });

    let app = builder
        .build(tauri::generate_context!())
        .expect("error while building tauri application");
    app.run(|handle, event| {
        if matches!(
            event,
            tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
        ) && let Some(state) = handle.try_state::<AppState>()
        {
            // Saves the list, waits for the write, stops the downloads.
            state.host.shutdown();
        }
    });
}

/// An automation-API request from the webview: the same payload the command
/// channel takes (`{query: "list"}`, `{action, ids}`, `{url, …}`), answered
/// with the same JSON. The webview is not bound by the automation API's
/// switch, which guards other programs.
#[tauri::command]
async fn mdm_call(state: tauri::State<'_, AppState>, payload: Value) -> Result<Value, String> {
    let cmd: mdm_ipc::Command =
        serde_json::from_value(payload).map_err(|error| format!("无法解析请求：{error}"))?;
    state.host.ask(Call::Mdm(cmd)).await
}

#[tauri::command]
async fn settings_read(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    state.host.ask(Call::SettingsRead).await
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsInput {
    mode: String,
    download_dir: String,
}

/// The settings dialog: save the General category's folder. Fucha is the
/// running program now, so no second program is restarted.
#[tauri::command]
async fn settings_write(
    state: tauri::State<'_, AppState>,
    input: SettingsInput,
) -> Result<Value, String> {
    state
        .host
        .ask(Call::SettingsWrite {
            mode: input.mode,
            download_dir: input.download_dir,
        })
        .await
}

#[tauri::command]
async fn pick_directory(default_path: Option<String>) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut dialog = rfd::FileDialog::new();
        if let Some(dir) = default_path.filter(|dir| !dir.trim().is_empty()) {
            dialog = dialog.set_directory(dir);
        }
        dialog.pick_folder().map(|path| path.display().to_string())
    })
    .await
    .map_err(|error| format!("打开文件夹选择器失败：{error}"))
}

#[tauri::command]
async fn open_external(url: String) -> Result<(), String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("只允许打开 http/https 链接".into());
    }
    opener::open(&url).map_err(|error| format!("无法打开链接：{error}"))
}

#[tauri::command]
async fn reveal(path: String) -> Result<(), String> {
    if !Path::new(&path).exists() {
        return Err("文件不存在".into());
    }
    opener::reveal(&path).map_err(|error| format!("无法打开文件夹：{error}"))
}

/// Which browsers have started the extension's native host: MDM records one
/// `browser\tversion` line in `extension-seen` each time one reaches it.
#[tauri::command]
fn browser_integration() -> Value {
    let file = mdm_ipc::data_dir().map(|dir| dir.join("extension-seen"));
    let text = file
        .and_then(|file| std::fs::read_to_string(file).ok())
        .unwrap_or_default();
    let browsers: Vec<Value> = text
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let (name, version) = line.split_once('\t').unwrap_or((line, ""));
            Some(serde_json::json!({ "name": name, "version": version }))
        })
        .collect();
    serde_json::json!({ "seen": !browsers.is_empty(), "browsers": browsers })
}

/// The General category's folder: where downloads go unless a category has
/// one of its own.
#[tauri::command]
async fn download_dir(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    state.host.ask(Call::DownloadDir).await
}

#[tauri::command]
fn system_download_dir() -> Option<String> {
    dirs::download_dir().map(|path| path.display().to_string())
}

/// Where the browser extension and the CLI find the command channel.
#[tauri::command]
fn ipc_path() -> String {
    mdm_ipc::endpoint_path()
        .map(PathBuf::into_os_string)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default()
}
