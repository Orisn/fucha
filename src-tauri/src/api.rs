use std::path::{Path, PathBuf};

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tower_http::cors::CorsLayer;

use crate::mdm;

#[derive(Clone)]
struct AppState {
    ipc: PathBuf,
}

pub(crate) fn router(ipc: PathBuf) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/mdm", post(mdm_proxy))
        .route("/api/ipc-path", get(ipc_path))
        .route("/api/download-dir", get(download_dir))
        .route("/api/system-download-dir", get(system_download_dir))
        .route("/api/settings", post(settings))
        .route("/api/pick-directory", post(pick_directory))
        .route("/api/open", post(open))
        .route("/api/reveal", post(reveal))
        .layer(CorsLayer::permissive())
        .with_state(AppState { ipc })
}

async fn health() -> Json<Value> {
    Json(json!({ "ok": true }))
}

/// Passes the payload to MDM as-is: the one-line JSON protocol is MDM's, not ours.
async fn mdm_proxy(
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    mdm::request_at(&state.ipc, &payload)
        .await
        .map(Json)
        .map_err(ApiError::from)
}

async fn ipc_path(State(state): State<AppState>) -> Json<Value> {
    Json(json!(state.ipc.display().to_string()))
}

async fn download_dir(State(state): State<AppState>) -> Json<Value> {
    Json(json!(mdm::download_dir(&state.ipc)))
}

async fn system_download_dir() -> Json<Value> {
    Json(json!(
        dirs::download_dir().map(|path| path.display().to_string())
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsBody {
    mode: String,
    download_dir: String,
}

async fn settings(
    State(state): State<AppState>,
    Json(body): Json<SettingsBody>,
) -> Result<Json<Value>, ApiError> {
    mdm::apply_settings(&state.ipc, &body.mode, &body.download_dir)
        .await
        .map(Json)
        .map_err(ApiError::from)
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct PickBody {
    default_path: Option<String>,
}

async fn pick_directory(Json(body): Json<PickBody>) -> Result<Json<Value>, ApiError> {
    let path = tokio::task::spawn_blocking(move || {
        let mut dialog = rfd::FileDialog::new();
        if let Some(dir) = body.default_path.filter(|dir| !dir.trim().is_empty()) {
            dialog = dialog.set_directory(dir);
        }
        dialog.pick_folder().map(|path| path.display().to_string())
    })
    .await
    .map_err(|error| ApiError::new(format!("打开文件夹选择器失败：{error}")))?;
    Ok(Json(json!({ "path": path })))
}

#[derive(Deserialize)]
struct OpenBody {
    url: String,
}

async fn open(Json(body): Json<OpenBody>) -> Result<Json<Value>, ApiError> {
    if !(body.url.starts_with("http://") || body.url.starts_with("https://")) {
        return Err(ApiError::bad_request("只允许打开 http/https 链接"));
    }
    opener::open(&body.url).map_err(|error| ApiError::new(format!("无法打开链接：{error}")))?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct RevealBody {
    path: String,
}

async fn reveal(Json(body): Json<RevealBody>) -> Result<Json<Value>, ApiError> {
    if !Path::new(&body.path).exists() {
        return Err(ApiError::bad_request("文件不存在"));
    }
    opener::reveal(&body.path)
        .map_err(|error| ApiError::new(format!("无法打开文件夹：{error}")))?;
    Ok(Json(json!({ "ok": true })))
}

pub(crate) struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: message.into(),
        }
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }
}

impl From<String> for ApiError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    use tower::ServiceExt;

    fn endpoint_file(port: u16) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fucha-api-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ipc.json");
        std::fs::write(&path, format!(r#"{{"port":{port},"token":"secret"}}"#)).unwrap();
        path
    }

    #[tokio::test]
    async fn health_reports_ok() {
        let response = router(PathBuf::from("unused"))
            .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn mdm_route_forwards_the_token_and_returns_the_answer() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let ipc = endpoint_file(port);

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut line = String::new();
            {
                let mut reader = BufReader::new(&mut socket);
                reader.read_line(&mut line).await.unwrap();
            }
            let request: Value = serde_json::from_str(line.trim()).unwrap();
            assert_eq!(request["token"], "secret");
            assert_eq!(request["query"], "list");
            socket.write_all(b"{\"downloads\":[]}\n").await.unwrap();
        });

        let response = router(ipc.clone())
            .oneshot(
                Request::post("/api/mdm")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"query":"list"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let answer: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(answer["downloads"], json!([]));

        let _ = std::fs::remove_dir_all(ipc.parent().unwrap());
    }
}
