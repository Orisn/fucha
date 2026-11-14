use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const ANSWER_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Deserialize)]
struct Endpoint {
    port: u16,
    token: String,
}

/// Where MDM keeps the port and token it listens on. Set `MDM_IPC_PATH` to point
/// at another data folder (a portable copy, for instance).
pub(crate) fn ipc_path() -> PathBuf {
    if let Some(path) = std::env::var_os("MDM_IPC_PATH") {
        return PathBuf::from(path);
    }
    data_dir().join("misterdownloadmanager").join("ipc.json")
}

fn data_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_default()
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|home| {
                PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
            })
            .unwrap_or_default()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(|home| PathBuf::from(home).join(".local").join("share"))
            })
            .unwrap_or_default()
    }
}

pub(crate) async fn request_at(path: &Path, payload: &Value) -> Result<Value, String> {
    let raw = tokio::fs::read_to_string(path)
        .await
        .map_err(|error| format!("读取 {} 失败：{error}", path.display()))?;
    let endpoint: Endpoint = serde_json::from_str(&raw)
        .map_err(|error| format!("解析 {} 失败：{error}", path.display()))?;

    let mut request = payload.clone();
    if let Value::Object(fields) = &mut request {
        fields.insert("token".into(), Value::String(endpoint.token));
    } else {
        return Err("请求必须是 JSON 对象".into());
    }

    let connect = TcpStream::connect(("127.0.0.1", endpoint.port));
    let mut stream = tokio::time::timeout(CONNECT_TIMEOUT, connect)
        .await
        .map_err(|_| "连接 MDM 超时".to_string())?
        .map_err(|error| format!("连接 MDM 失败：{error}"))?;

    let line = serde_json::to_string(&request).map_err(|error| error.to_string())?;
    stream
        .write_all(format!("{line}\n").as_bytes())
        .await
        .map_err(|error| format!("发送请求失败：{error}"))?;

    let mut reader = BufReader::new(stream);
    let mut answer = String::new();
    tokio::time::timeout(ANSWER_TIMEOUT, reader.read_line(&mut answer))
        .await
        .map_err(|_| "等待 MDM 回复超时".to_string())?
        .map_err(|error| format!("读取回复失败：{error}"))?;

    match answer.trim() {
        "" => Err("MDM 没有回复".into()),
        "ok" => Ok(json!({ "ok": true })),
        "denied" => Err("MDM 拒绝了请求：token 无效，请重启 MDM 后重试。".into()),
        other => serde_json::from_str(other)
            .map_err(|error| format!("无法解析 MDM 的回复：{error}（原文：{other}）")),
    }
}

fn ipc_dir(ipc: &Path) -> PathBuf {
    ipc.parent().map(Path::to_path_buf).unwrap_or_default()
}

fn mdm_exe(ipc: &Path) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("MDM_EXE") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }

    let data = ipc_dir(ipc);
    if data.file_name() == Some(std::ffi::OsStr::new("data"))
        && let Some(root) = data.parent()
    {
        let candidate = root.join(exe_name());
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    #[cfg(windows)]
    {
        let mut roots = Vec::new();
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            roots.push(PathBuf::from(local).join("Programs"));
        }
        for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(dir) = std::env::var_os(variable) {
                roots.push(PathBuf::from(dir));
            }
        }
        for root in roots {
            let candidate = root.join("Mister Download Manager").join(exe_name());
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    #[cfg(not(windows))]
    {
        let candidate = PathBuf::from("/usr/bin").join(exe_name());
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

fn exe_name() -> &'static str {
    if cfg!(windows) {
        "misterdownloadmanager.exe"
    } else {
        "misterdownloadmanager"
    }
}

/// Sets the folder categories without one of their own live under: the General category's.
fn set_general_dir(data: &Path, dir: &str) -> Result<(), String> {
    let list = data.join("downloads.json");
    let raw = std::fs::read_to_string(&list)
        .map_err(|error| format!("读取 {} 失败：{error}", list.display()))?;
    let mut root: Value = serde_json::from_str(&raw)
        .map_err(|error| format!("解析 {} 失败：{error}", list.display()))?;
    let settings = root
        .get_mut("settings")
        .and_then(Value::as_object_mut)
        .ok_or("downloads.json 里没有 settings")?;
    let entries = settings.entry("category_dirs").or_insert_with(|| json!([]));
    let entries = entries
        .as_array_mut()
        .ok_or("settings.category_dirs 不是数组")?;
    entries.retain(|entry| entry.get(0).and_then(Value::as_str) != Some("General"));
    entries.push(json!(["General", dir]));

    let backup = data.join("downloads.json.mdm-studio.bak");
    std::fs::copy(&list, &backup)
        .map_err(|error| format!("备份 {} 失败：{error}", list.display()))?;
    let temporary = data.join("downloads.json.mdm-studio.tmp");
    std::fs::write(
        &temporary,
        serde_json::to_vec_pretty(&root).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("写入临时文件失败：{error}"))?;
    std::fs::rename(&temporary, &list)
        .map_err(|error| format!("替换 {} 失败：{error}", list.display()))?;
    Ok(())
}

fn mdm_running() -> bool {
    #[cfg(windows)]
    {
        let output = std::process::Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq misterdownloadmanager.exe", "/NH"])
            .output();
        match output {
            Ok(output) => String::from_utf8_lossy(&output.stdout)
                .to_lowercase()
                .contains("misterdownloadmanager.exe"),
            Err(_) => false,
        }
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("pgrep")
            .args(["-x", "misterdownloadmanager"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}

async fn stop_mdm(exe: &Path) -> Result<(), String> {
    if !mdm_running() {
        return Ok(());
    }
    std::process::Command::new(exe)
        .arg("/exit")
        .spawn()
        .map_err(|error| format!("无法让 MDM 退出：{error}"))?;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        if !mdm_running() {
            // Its final save runs as it exits; let it finish before we touch downloads.json.
            tokio::time::sleep(Duration::from_millis(400)).await;
            return Ok(());
        }
    }
    Err("MDM 没有退出，请手动关闭它后再试。".into())
}

async fn current_port(ipc: &Path) -> Option<u16> {
    let raw = tokio::fs::read_to_string(ipc).await.ok()?;
    let endpoint: Endpoint = serde_json::from_str(&raw).ok()?;
    Some(endpoint.port)
}

async fn port_open(port: u16) -> bool {
    tokio::time::timeout(
        Duration::from_millis(600),
        TcpStream::connect(("127.0.0.1", port)),
    )
    .await
    .map(|result| result.is_ok())
    .unwrap_or(false)
}

async fn wait_ready(ipc: &Path, child: &mut std::process::Child) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while tokio::time::Instant::now() < deadline {
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!("MDM 启动后立即退出（{status}）"));
        }
        if let Some(port) = current_port(ipc).await
            && port_open(port).await
            && request_at(ipc, &json!({ "query": "list" })).await.is_ok()
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Err("MDM 启动后没有响应".into())
}

fn start_command(
    exe: &Path,
    data: &Path,
    download_dir: &str,
    mode: &str,
) -> Result<std::process::Command, String> {
    let mut command = std::process::Command::new(exe);
    match mode {
        "background" => {
            command
                .arg("--headless")
                .arg("--data")
                .arg(data)
                .arg("--downloads")
                .arg(download_dir);
            command.env("MDM_REMOTE", "off");
        }
        "window" => {
            command.arg("/startup");
        }
        _ => return Err("未知的运行模式".into()),
    }
    Ok(command)
}

/// Which browsers have started the extension's native host: MDM records one
/// `browser\tversion` line in `extension-seen` each time one reaches it.
pub(crate) fn browser_integration(ipc: &Path) -> Value {
    let file = ipc_dir(ipc).join("extension-seen");
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let browsers: Vec<Value> = text
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let (name, version) = line.split_once('\t').unwrap_or((line, ""));
            Some(json!({ "name": name, "version": version }))
        })
        .collect();
    json!({ "seen": !browsers.is_empty(), "browsers": browsers })
}

pub(crate) fn download_dir(ipc: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(ipc_dir(ipc).join("downloads.json")).ok()?;
    let root: Value = serde_json::from_str(&raw).ok()?;
    let entries = root.get("settings")?.get("category_dirs")?.as_array()?;
    entries
        .iter()
        .find(|entry| entry.get(0).and_then(Value::as_str) == Some("General"))
        .and_then(|entry| entry.get(1).and_then(Value::as_str))
        .map(str::to_string)
}

pub(crate) async fn apply_settings(
    ipc: &Path,
    mode: &str,
    download_dir: &str,
) -> Result<Value, String> {
    let download_dir = download_dir.trim().to_string();
    if download_dir.is_empty() {
        return Err("请选择下载目录".into());
    }
    std::fs::create_dir_all(&download_dir).map_err(|error| format!("无法创建下载目录：{error}"))?;

    let exe = mdm_exe(ipc)
        .ok_or("找不到 Mister Download Manager，请设置 MDM_EXE 环境变量指向它的程序。")?;
    let data = ipc_dir(ipc);

    stop_mdm(&exe).await?;
    set_general_dir(&data, &download_dir)?;

    let mut last_error = String::new();
    for attempt in 1..=2 {
        let mut command = start_command(&exe, &data, &download_dir, mode)?;
        let mut child = command
            .spawn()
            .map_err(|error| format!("启动 MDM 失败：{error}"))?;
        match wait_ready(ipc, &mut child).await {
            Ok(()) => {
                return Ok(json!({ "mode": mode, "downloadDir": download_dir }));
            }
            Err(error) => {
                last_error = error;
                let _ = child.kill();
                let _ = child.wait();
                if attempt == 1 {
                    tokio::time::sleep(Duration::from_millis(1200)).await;
                }
            }
        }
    }
    Err(format!("MDM 启动失败：{last_error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    fn endpoint_file(port: u16, label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fucha-test-{}-{}-{label}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ipc.json");
        std::fs::write(&path, format!(r#"{{"port":{port},"token":"secret"}}"#)).unwrap();
        path
    }

    #[tokio::test]
    async fn adds_the_token_and_reads_a_json_answer() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let path = endpoint_file(port, "json");

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

        let answer = request_at(&path, &json!({ "query": "list" }))
            .await
            .unwrap();
        assert_eq!(answer["downloads"], json!([]));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[tokio::test]
    async fn plain_ok_becomes_an_ok_flag() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let path = endpoint_file(port, "ok");

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut line = String::new();
            {
                let mut reader = BufReader::new(&mut socket);
                reader.read_line(&mut line).await.unwrap();
            }
            socket.write_all(b"ok\n").await.unwrap();
        });

        let answer = request_at(&path, &json!({ "action": "pause", "ids": [1] }))
            .await
            .unwrap();
        assert_eq!(answer["ok"], json!(true));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
