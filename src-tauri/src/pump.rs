//! Fucha's download core, driven in process: one thread owns
//! [`mdm_core::session::Core`] and does its periodic work (polling the engine,
//! the scheduler, the download limits, saving), while Tauri commands and the
//! command channel ([`mdm_ipc`], used by the browser extension and the CLI)
//! hand it work over a channel and wait for the answer.
//!
//! The core is not thread safe and the egui app owns one on its UI thread; the
//! same model works here with a thread of its own, so every access is
//! serialized and no lock is held while the engine runs.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, SyncSender};
use std::time::{Duration, Instant};

use mdm_core::layout::Layout;
use mdm_core::model::{Category, Item, Status};
use mdm_core::queue::Queue;
use mdm_core::session::{ActEffect, Core, LimitEffect, RequestParts, TickEffect};
use mdm_ipc as ipc;
use serde_json::{Value, json};
use tauri::Emitter;

/// One list snapshot, at most four times a second. The payload is the same
/// JSON object the automation API's `{"query":"list"}` answers (a string: the
/// webview parses it once).
pub const LIST_EVENT: &str = "mdm://list";
/// Something worth telling the user about that is not in the list: the
/// download limit stopped the downloads, a download finished or failed, the
/// list could not be saved.
pub const NOTICE_EVENT: &str = "mdm://notice";

const TICK: Duration = Duration::from_millis(250);
const IDLE: Duration = Duration::from_millis(50);
const EMIT_EVERY: Duration = Duration::from_millis(250);
const SAVE_EVERY: Duration = Duration::from_secs(1);
const FREE_SPACE_EVERY: Duration = Duration::from_secs(10);

type Answer = Result<Value, ipc::api::ErrorAnswer>;

/// Work the webview asks the core thread for.
pub enum Call {
    /// An automation-API request, in the same shape as the command channel's.
    Mdm(ipc::Command),
    SettingsRead,
    SettingsWrite {
        mode: String,
        download_dir: String,
    },
    /// The General category's folder (the effective one).
    DownloadDir,
}

pub enum Msg {
    Call {
        call: Call,
        reply: tokio::sync::oneshot::Sender<Result<Value, String>>,
    },
    /// A command from the browser extension or the CLI: one that asks for
    /// an answer waits for one; a plain add or `/exit` was answered "ok" by
    /// the IPC server when it arrived.
    Incoming(ipc::Incoming),
    Shutdown {
        done: SyncSender<()>,
    },
}

/// The webview's handle on the core thread. Cheap to clone; held in Tauri's
/// managed state.
#[derive(Clone)]
pub struct Host {
    tx: Sender<Msg>,
}

impl Host {
    pub fn new(tx: Sender<Msg>) -> Host {
        Host { tx }
    }

    /// Hand `call` to the core and wait for its answer (async, for Tauri
    /// commands).
    pub async fn ask(&self, call: Call) -> Result<Value, String> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(Msg::Call { call, reply })
            .map_err(|_| "后台下载核心已停止".to_string())?;
        rx.await.map_err(|_| "后台下载核心没有回复".to_string())?
    }

    /// Save the list and wait for the write to reach the disk (app exit).
    pub fn shutdown(&self) {
        let (done, wait) = std::sync::mpsc::sync_channel(1);
        if self.tx.send(Msg::Shutdown { done }).is_ok() {
            let _ = wait.recv_timeout(Duration::from_secs(10));
        }
    }
}

/// The core thread.
pub struct Pump {
    core: Core,
    columns: Layout,
    app: tauri::AppHandle,
    /// "background" or "window": what the settings dialog last chose. It no
    /// longer starts a second program, but the dialog remembers it.
    mode: String,
    free: Option<u64>,
    free_checked: Instant,
    last_tick: Instant,
    last_emit: Instant,
    last_save: Instant,
    stopped: bool,
}

impl Pump {
    pub fn new(app: tauri::AppHandle, handle: tokio::runtime::Handle) -> Pump {
        let loaded = mdm_core::store::load();
        let settings = loaded.settings;
        // The category registries the model's `Category::of` reads.
        settings.publish_categories();
        let downloads = dirs::download_dir()
            .or_else(|| dirs::home_dir().map(|home| home.join("Downloads")))
            .unwrap_or_else(|| PathBuf::from("."));
        let mut queues = loaded.queues;
        if queues.len() < 2 {
            // The built-in queues, as MDM names them (saved in English, shown
            // translated there; Fucha shows the names as they are).
            queues = vec![
                Queue::new("Main download queue", false),
                Queue::new("Synchronization queue", true),
            ];
        }
        for q in &mut queues {
            q.running = q.schedule.start_on_startup;
        }
        let mut items = loaded.items;
        for item in &mut items {
            // Torrents are a category of their own (D-021).
            if item.torrent.is_some() {
                item.category = Category::Torrents;
            }
            // A user's category that is gone: General.
            if let Category::Custom(id) = item.category
                && mdm_core::model::custom(id).is_none()
            {
                item.category = Category::General;
            }
        }
        let mode = load_mode();
        let core = Core {
            rt: handle,
            items,
            next_id: loaded.next_id.max(1),
            queues,
            settings,
            downloads,
            dirty: false,
            secrets_confirmed: 0,
            last_save: Instant::now(),
            saver: mdm_core::store::Saver::new(),
            clock: seconds_now(),
            wake: Default::default(),
            global: Arc::new(mdm_engine::limiter::Limiter::new(None)),
            host_limits: Default::default(),
            quota: Default::default(),
            failures: HashMap::new(),
            stopped_by_hand: Default::default(),
            debrid: Default::default(),
            debrid_from: 0,
            route: mdm_engine::Route::Any,
            adapter_paused: Vec::new(),
        };
        Pump {
            core,
            columns: loaded.columns.unwrap_or_default(),
            app,
            mode,
            free: None,
            free_checked: Instant::now() - FREE_SPACE_EVERY,
            last_tick: Instant::now() - TICK,
            last_emit: Instant::now(),
            last_save: Instant::now(),
            stopped: false,
        }
    }

    /// The core thread's loop: commands as they come, the clock four times a
    /// second, and a list snapshot for the webview at the same rate.
    pub fn run(mut self, rx: Receiver<Msg>) {
        loop {
            while let Ok(msg) = rx.try_recv() {
                if !self.handle(msg) {
                    self.stop();
                    return;
                }
            }
            if self.last_tick.elapsed() >= TICK {
                self.tick();
                self.last_tick = Instant::now();
            }
            match rx.recv_timeout(IDLE) {
                Ok(msg) => {
                    if !self.handle(msg) {
                        self.stop();
                        return;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    self.stop();
                    return;
                }
            }
        }
    }

    /// One message; false when the loop must end (shutdown).
    fn handle(&mut self, msg: Msg) -> bool {
        match msg {
            Msg::Incoming(incoming) => {
                self.incoming(incoming);
                true
            }
            Msg::Call { call, reply } => {
                let answer = match call {
                    Call::Mdm(cmd) => self.answer(&cmd, true).map_err(|error| error.error),
                    Call::SettingsRead => Ok(self.settings_read()),
                    Call::SettingsWrite { mode, download_dir } => {
                        self.settings_write(&mode, &download_dir)
                    }
                    Call::DownloadDir => {
                        Ok(json!(self.general_dir().to_string_lossy().into_owned()))
                    }
                };
                let _ = reply.send(answer);
                true
            }
            Msg::Shutdown { done } => {
                self.stop();
                let _ = done.send(());
                false
            }
        }
    }

    fn incoming(&mut self, incoming: ipc::Incoming) {
        match incoming.reply {
            Some(reply) => {
                let answer = match self.answer(&incoming.cmd, false) {
                    Ok(value) => value.to_string(),
                    Err(error) => ipc::api::line(&error),
                };
                let _ = reply.send(answer);
            }
            // A command that asks nothing, answered "ok" by the IPC server
            // before it reached us (an address to add, /exit, a queue start).
            None => self.command(&incoming.cmd),
        }
    }

    /// The automation API's answer (a JSON value): `api` turns it on or off,
    /// `query: "list"` gives every download, `action` acts on downloads, a
    /// `url` adds one. `internal`: from Fucha's own webview, which is not
    /// bound by Options → Automation API's switch (that guards other
    /// programs) and always gets its answer.
    fn answer(&mut self, cmd: &ipc::Command, internal: bool) -> Answer {
        let fail = |message: &str| ipc::api::ErrorAnswer {
            error: message.into(),
            code: None,
        };
        if cmd.pair || cmd.remote {
            return Err(fail(
                "the remote control isn't available in Fucha (use Mister Download Manager)",
            ));
        }
        if let Some(on) = cmd.api {
            self.core.settings.automation_api = on;
            self.core.dirty = true;
            return Ok(json!({ "api": on }));
        }
        if !internal
            && !self.core.settings.automation_api
            && (cmd.query.is_some() || cmd.action.is_some())
        {
            return Err(ipc::api::ErrorAnswer {
                error: "the automation API is off (misterdownloadmanager-cli api on)".into(),
                code: Some(ipc::api::API_OFF.into()),
            });
        }
        if let Some(action @ ("start_queue" | "stop_queue")) = cmd.action.as_deref() {
            let at = cmd
                .queue
                .as_deref()
                .and_then(|name| self.core.queues.iter().position(|q| q.name == name));
            let Some(queue) = at else {
                return Err(ipc::api::ErrorAnswer {
                    error: "no such queue".into(),
                    code: Some("unknown-queue".into()),
                });
            };
            if action == "start_queue" {
                self.start_queue(queue);
            } else {
                self.core.stop_queue(queue);
                self.core.dirty = true;
            }
            return Ok(serde_json::to_value(ipc::api::ActAnswer::default()).unwrap_or(Value::Null));
        }
        if let Some(action) = cmd.action.as_deref() {
            // A running download is paused first by the caller, as the API
            // promises (`Core::act` says "running: pause it first").
            let (answer, effects) = self.core.act(action, &cmd.ids)?;
            for effect in effects {
                match effect {
                    ActEffect::Start(id) => self.start(id),
                    ActEffect::Remove { id, files } => self.remove(id, files),
                }
            }
            return Ok(serde_json::to_value(answer).unwrap_or(Value::Null));
        }
        match cmd.query.as_deref() {
            Some("list") => {
                let free = self.free();
                let line = self.core.list_answer(false, free);
                return Ok(serde_json::from_str(&line).unwrap_or(Value::Null));
            }
            Some(other) => return Err(fail(&format!("unknown query {other:?}"))),
            None => {}
        }
        if cmd.start_queue {
            self.start_queue(0);
        }
        if cmd.url.is_some() {
            let id = self.add(cmd)?;
            return Ok(serde_json::to_value(ipc::api::AddAnswer {
                id: Some(id),
                pending: false,
            })
            .unwrap_or(Value::Null));
        }
        Ok(json!({ "ok": true }))
    }

    /// A command that expects no answer (the IPC server already said "ok").
    fn command(&mut self, cmd: &ipc::Command) {
        if cmd.exit {
            self.stop();
            self.app.exit(0);
            return;
        }
        // Fucha has no MDM windows to open or hide.
        if cmd.startup || cmd.show.is_some() {
            return;
        }
        if cmd.start_queue {
            self.start_queue(0);
        }
        if cmd.url.is_some() {
            if let Err(error) = self.add(cmd) {
                self.notice(json!({ "kind": "add-failed", "error": error }));
            }
        }
    }

    /// Add an address as the command line's silent add does: by the file's
    /// type into its category's folder, started now unless it waits in a queue.
    fn add(&mut self, cmd: &ipc::Command) -> Result<u64, ipc::api::ErrorAnswer> {
        let fail = |message: String| ipc::api::ErrorAnswer {
            error: message,
            code: None,
        };
        let raw = cmd
            .url
            .as_deref()
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .ok_or_else(|| fail("地址为空".into()))?;
        let url = url::Url::parse(raw).map_err(|error| fail(format!("地址无法解析：{error}")))?;
        let magnet = url.scheme() == "magnet";
        if !magnet && !mdm_engine::supported(&url) {
            return Err(fail("只支持 http、https、ftp 和 magnet 地址".into()));
        }
        let guessed = cmd.name.is_none();
        let name = cmd
            .name
            .clone()
            .unwrap_or_else(|| mdm_engine::naming::file_name(None, &url));
        let name = mdm_engine::naming::sanitize(&name);
        let category = cmd
            .category
            .as_deref()
            .and_then(|key| Category::all().into_iter().find(|c| c.key() == key))
            .unwrap_or_else(|| {
                if magnet {
                    Category::Torrents
                } else {
                    Category::of(&name)
                }
            });
        let queue = cmd
            .queue
            .as_deref()
            .and_then(|name| self.core.queues.iter().position(|q| q.name == name))
            .unwrap_or(0);
        let dir = cmd.dir.as_deref().map(PathBuf::from).unwrap_or_else(|| {
            self.core
                .settings
                .category_dir(category, &self.core.downloads)
        });
        let dest = mdm_engine::naming::destination(&dir, &name);
        let id = self.core.next_id;
        self.core.next_id += 1;
        let mut item = Item::new(id, url, dest, category);
        item.size = cmd.size;
        item.context = cmd.clone();
        // The browser had started it: the host is already sending the file.
        item.direct = mdm_core::debrid::browser_started(cmd);
        item.note(if cmd.referer.is_some() || cmd.cookies.is_some() {
            // i18n: event log entry, past tense: a download added from the browser
            "Added from the browser"
        } else {
            // i18n: event log entry, past tense: a download added from the command line
            "Added from the command line"
        });
        if guessed {
            item.name_from_url = true;
        }
        self.core.items.push(item);
        self.core.dirty = true;
        // With no Download File Info dialog, "only add to queue" applies as it
        // does when MDM skips its start dialog.
        let later = cmd.later || self.core.settings.only_add_to_queue;
        if later {
            self.core.queues[queue].items.push(id);
        } else {
            self.start(id);
        }
        Ok(id)
    }

    /// Start or resume one download, as the app's `start` does without the
    /// windows: the request parts from the item's context, the file limit,
    /// the checksum. Videos, galleries and direct torrents need MDM's helpers
    /// or its torrent session and are not wired here (TODO nmhost/torrents).
    fn start(&mut self, id: u64) {
        self.core.stopped_by_hand.remove(&id);
        // Read the facts first: the mutable work below (queue heads, statuses)
        // must not overlap a borrow of the list.
        let Some((live, status, video, gallery, remote, torrent, magnet)) =
            self.core.items.iter().find(|i| i.id == id).map(|i| {
                (
                    i.live.is_some(),
                    i.status,
                    i.video.is_some(),
                    i.gallery.is_some(),
                    i.remote.is_some(),
                    i.torrent.is_some(),
                    i.url.scheme() == "magnet",
                )
            })
        else {
            return;
        };
        if live || (status == Status::Complete && !gallery) {
            return;
        }
        if video || gallery {
            self.fail(id, "视频和图集下载需要 MDM 的工具，浮槎尚未接入。");
            return;
        }
        // The download limit is reached: it starts when the window has room.
        if self.core.settings.limits_on && self.core.quota.blocked {
            if !self.core.quota.held.contains(&id) {
                self.core.quota.held.push(id);
            }
            if let Some(item) = self.core.item(id) {
                item.status = Status::Queued;
                item.note("Waiting: the download limit is reached (Scheduler)");
            }
            self.core.dirty = true;
            return;
        }
        if remote {
            self.core.start_remote(id);
            return;
        }
        if torrent || magnet {
            self.fail(
                id,
                "种子下载需要 MDM 的 torrent 会话，浮槎尚未接入（nmhost/种子为 TODO）。",
            );
            return;
        }
        let Some((url, login, context, own_limit, checksum, direct)) =
            self.core.items.iter().find(|i| i.id == id).map(|i| {
                (
                    i.url.clone(),
                    i.login.clone(),
                    i.context.clone(),
                    i.own_limit,
                    i.checksum.clone(),
                    i.direct,
                )
            })
        else {
            return;
        };
        // The browser's proxy needs its VPN extension's own login: said, never
        // bypassed.
        if let Some(why) = self.core.settings.browser_proxy_refused(&context) {
            self.fail(id, why);
            return;
        }
        let RequestParts { headers, mut opts } =
            self.core
                .request_parts(&url, login.as_ref(), &context, direct);
        // The file's own remembered limit, else the Speed Limiter's.
        opts.speed_limit = own_limit.or(self.core.settings.speed_limit());
        opts.checksum = checksum;
        self.core
            .start_file(id, RequestParts { headers, opts }, None);
    }

    fn fail(&mut self, id: u64, why: impl Into<String>) {
        if let Some(item) = self.core.item(id) {
            item.status = Status::Error;
            item.error = Some(why.into());
            item.last_try = Some(mdm_core::model::now_stamp());
        }
        self.core.dirty = true;
    }

    /// Take a download off the list; `files` deletes what it made on the disk.
    fn remove(&mut self, id: u64, files: bool) {
        let Some(item) = self.core.items.iter().find(|i| i.id == id) else {
            return;
        };
        if item.live.is_some() {
            return;
        }
        let dest = item.dest.clone();
        let complete = item.status == Status::Complete;
        if files && complete {
            let _ = std::fs::remove_file(&dest);
        }
        // The data so far, and what a resume would read.
        let _ = std::fs::remove_file(mdm_engine::naming::part_path(&dest));
        let _ = std::fs::remove_file(mdm_engine::state::state_path(&dest));
        self.core.items.retain(|i| i.id != id);
        for q in &mut self.core.queues {
            q.items.retain(|x| *x != id);
        }
        self.core.failures.remove(&id);
        self.core.stopped_by_hand.remove(&id);
        self.core.dirty = true;
    }

    fn start_queue(&mut self, queue: usize) {
        if self.core.start_queue(queue) {
            self.notice(json!({
                "kind": "queue-started",
                "queue": self.core.queues[queue].name,
            }));
        }
        self.core.dirty = true;
        self.apply_ticks();
    }

    /// The core's clock: engine snapshots, the scheduler's queues and starts,
    /// the download limits, the debrid accounts.
    fn tick(&mut self) {
        let polled = self.core.poll_items();
        self.core.note_running(polled.any_live);
        if !polled.finished.is_empty() {
            self.notice(json!({ "kind": "finished", "ids": polled.finished }));
        }
        if !polled.failed.is_empty() {
            self.notice(json!({ "kind": "failed", "ids": polled.failed }));
        }
        self.apply_ticks();
        for effect in self.core.check_limits(Instant::now()) {
            match effect {
                LimitEffect::Start(id) => self.start(id),
                LimitEffect::Notice { mb, hours, wait } => self.notice(json!({
                    "kind": "limit",
                    "mb": mb,
                    "hours": hours,
                    "waitSecs": wait.as_secs(),
                })),
            }
        }
        self.core.debrid_tick();
        if self.core.dirty && self.last_save.elapsed() >= SAVE_EVERY {
            if let Err(error) = self.core.save(&self.columns) {
                self.notice(json!({ "kind": "save-error", "error": error }));
            }
            self.last_save = Instant::now();
        }
        if let Some(Err(error)) = self.core.take_save_outcome() {
            self.notice(json!({ "kind": "save-error", "error": error }));
        }
        // Keep the snapshot's free space fresh (looked up at most every 10 s).
        self.free();
        if self.last_emit.elapsed() >= EMIT_EVERY {
            self.emit_list();
            self.last_emit = Instant::now();
        }
    }

    fn apply_ticks(&mut self) {
        for effect in self.core.tick_queues() {
            match effect {
                TickEffect::Start(id) => self.start(id),
                // A finished queue's "when done" actions (sound, exit, turn
                // off) are the egui app's; Fucha only says so.
                TickEffect::QueueDone(queue) => self.notice(json!({
                    "kind": "queue-done",
                    "queue": self.core.queues.get(queue).map(|q| q.name.clone()),
                })),
            }
        }
    }

    fn emit_list(&self) {
        let line = self.core.list_answer(false, self.free);
        let _ = self.app.emit(LIST_EVENT, line);
    }

    fn notice(&self, value: Value) {
        let _ = self.app.emit(NOTICE_EVENT, value.to_string());
    }

    fn general_dir(&self) -> PathBuf {
        self.core
            .settings
            .category_dir(Category::General, &self.core.downloads)
    }

    /// Free space where downloads go, looked up at most every 10 s (a network
    /// share can be slow to answer).
    fn free(&mut self) -> Option<u64> {
        if self.free_checked.elapsed() >= FREE_SPACE_EVERY {
            self.free = mdm_engine::disk::volume_of(&self.general_dir()).map(|v| v.free);
            self.free_checked = Instant::now();
        }
        self.free
    }

    fn settings_read(&self) -> Value {
        json!({
            "downloadDir": self.general_dir().to_string_lossy(),
            "mode": self.mode,
        })
    }

    /// The settings dialog: the General category's folder, and the run mode
    /// (kept, though Fucha itself is the running program now). Saved at once:
    /// the dialog's answer promises it.
    fn settings_write(&mut self, mode: &str, download_dir: &str) -> Result<Value, String> {
        let dir = download_dir.trim();
        if dir.is_empty() {
            return Err("请选择下载目录".into());
        }
        std::fs::create_dir_all(dir).map_err(|error| format!("无法创建下载目录：{error}"))?;
        self.core
            .settings
            .set_category_dir(Category::General, Some(dir.to_owned()));
        self.core.settings.publish_categories();
        self.mode = if mode == "window" {
            "window".into()
        } else {
            "background".into()
        };
        save_mode(&self.mode);
        self.core.dirty = true;
        self.core
            .save(&self.columns)
            .map_err(|error| format!("保存设置失败：{error}"))?;
        self.core.saver.flush();
        if let Some(Err(error)) = self.core.take_save_outcome() {
            return Err(format!("保存设置失败：{error}"));
        }
        Ok(json!({ "mode": self.mode, "downloadDir": dir }))
    }

    /// Save everything and wait for the write (Fucha closing).
    fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        self.core.stop_all();
        let _ = self.core.save(&self.columns);
        self.core.saver.flush();
        if let Some(Err(error)) = self.core.take_save_outcome() {
            self.notice(json!({ "kind": "save-error", "error": error }));
        }
    }
}

fn seconds_now() -> u32 {
    use chrono::Timelike;
    chrono::Local::now().num_seconds_from_midnight()
}

/// Fucha's own settings (the run mode the dialog remembers), beside MDM's.
fn mode_path() -> Option<PathBuf> {
    mdm_ipc::data_dir().map(|dir| dir.join("fucha.json"))
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ModeFile {
    #[serde(default = "default_mode")]
    mode: String,
}

fn default_mode() -> String {
    "background".into()
}

fn load_mode() -> String {
    let Some(path) = mode_path() else {
        return default_mode();
    };
    let Ok(raw) = std::fs::read(&path) else {
        return default_mode();
    };
    match serde_json::from_slice::<ModeFile>(&raw) {
        Ok(file) if file.mode == "window" => "window".into(),
        _ => default_mode(),
    }
}

fn save_mode(mode: &str) {
    let Some(path) = mode_path() else {
        return;
    };
    if let Ok(bytes) = serde_json::to_vec(&ModeFile {
        mode: mode.to_owned(),
    }) {
        let _ = mdm_ipc::private_write(&path, &bytes);
    }
}
