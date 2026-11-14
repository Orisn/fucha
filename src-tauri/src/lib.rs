mod mdm;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            mdm::mdm_request,
            mdm::mdm_ipc_path,
            mdm::mdm_download_dir,
            mdm::mdm_apply_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
