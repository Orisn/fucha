mod api;
mod mdm;

use std::net::{Ipv4Addr, SocketAddr};

pub const API_PORT: u16 = 8484;

pub async fn serve() -> Result<(), String> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, API_PORT));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|error| format!("无法监听 {addr}：{error}"))?;
    println!("浮槎 API：http://{addr}");
    axum::serve(listener, api::router(mdm::ipc_path()))
        .await
        .map_err(|error| format!("API 运行失败：{error}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|_app| {
            tauri::async_runtime::spawn(async {
                if let Err(error) = serve().await {
                    eprintln!("浮槎 API 启动失败：{error}");
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
