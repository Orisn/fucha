//! The download session's state that needs no window: the list, its queues, the settings it is
//! saved with, and the persistence around them. The egui app keeps the widgets and dialogs and
//! reaches this state through `Deref`; Fucha's Tauri backend will drive it directly.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mdm_engine::hosts::HostLimits;
use mdm_engine::limiter::Limiter;
use mdm_engine::state::state_path;
use mdm_engine::{Destination, Download, KeptAnswer, Options, Request};
use mdm_ipc as ipc;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};

use crate::layout::Layout;
use crate::model::{Category, Item, Job, Live, Status};
use crate::queue::{self, Member, Queue};
use crate::quota::Quota;
use crate::settings::Settings;
use crate::store;
use crate::torrent::SharingLimits;
use crate::torrent_spec::TorrentSpec;
use crate::wake::Waker;

pub struct Core {
    pub rt: tokio::runtime::Handle,
    pub items: Vec<Item>,
    pub next_id: u64,
    pub queues: Vec<Queue>,
    pub settings: Settings,
    pub downloads: PathBuf,
    pub dirty: bool,
    pub secrets_confirmed: u64,
    pub last_save: Instant,
    pub saver: store::Saver,
    /// Seconds since midnight, for the queues' schedules (`tick_queues`).
    pub clock: u32,
    pub wake: Waker,
    /// The global speed cap shared by every download (Speed Limiter settings).
    pub global: Arc<Limiter>,
    /// Connection caps learned per host, shared by every download.
    pub host_limits: Arc<HostLimits>,
    /// Scheduler → Download limits: traffic in the window, and what the limit holds back.
    pub quota: Quota,
    /// Failure time and count per download, for queue retries.
    pub failures: HashMap<u64, (Instant, u32)>,
    /// Stopped by hand while they could have run: a running queue leaves them alone until
    /// they are started again or the queue is started anew (#341).
    pub stopped_by_hand: HashSet<u64>,
    /// Options → Accounts (#44): the debrid accounts downloads go through, and what they were
    /// set up from (so they are set up again only when that changes).
    pub debrid: mdm_debrid::Debrid,
    pub debrid_from: u64,
    pub route: mdm_engine::Route,
    pub adapter_paused: Vec<u64>,
}

/// What an automation action asks the window to do once the core has decided it: starting or
/// taking a download off the list is the app's (it owns the engine's progress windows).
pub enum ActEffect {
    Start(u64),
    Remove { id: u64, files: bool },
}

/// What a scheduler tick asks the app to do: start a download, or run a finished queue's
/// "when done" actions (sound, open, turn off, exit).
pub enum TickEffect {
    Start(u64),
    QueueDone(usize),
}

/// What the download-limit check asks the app to do: start a download again once the window
/// has room, or show the limit's notice.
pub enum LimitEffect {
    Start(u64),
    Notice { mb: u32, hours: u32, wait: Duration },
}

/// What starting a torrent asks the app's librqbit session to run, once the core has decided it:
/// the torrent, the hashes the session keeps (and must not add twice), its download limit and
/// whether it keeps sharing once finished.
pub struct StartTorrent {
    pub id: u64,
    pub spec: TorrentSpec,
    pub url: url::Url,
    pub known: HashSet<String>,
    pub limit: Option<u64>,
    pub share: bool,
}

/// What a network route change asks the app to do: stop the torrent session (its sockets were
/// bound as things were), then start again the downloads the change stopped, once their jobs
/// have ended.
#[derive(Default)]
pub struct RouteEffect {
    pub torrent_restart: bool,
    pub starts: Vec<u64>,
}

/// Which seeding limit a finished torrent reached.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reached {
    /// The share ratio it reached.
    Ratio(f64),
    /// The seconds it shared.
    Time(u64),
}

/// Whether `spec`, with `done` bytes there, has reached a seeding limit (the ratio first).
pub fn sharing_limit_reached(
    spec: &TorrentSpec,
    done: u64,
    limits: SharingLimits,
) -> Option<Reached> {
    let ratio = share_ratio(spec.uploaded, done);
    if let Some(r) = limits.ratio
        && done > 0
        && ratio >= r
    {
        return Some(Reached::Ratio(ratio));
    }
    if let Some(s) = limits.secs
        && spec.seeded_secs >= s
    {
        return Some(Reached::Time(spec.seeded_secs));
    }
    None
}

/// A torrent's share ratio: what it sent to peers against what it has. Zero while it has nothing.
pub fn share_ratio(sent: u64, done: u64) -> f64 {
    if done == 0 {
        0.0
    } else {
        sent as f64 / done as f64
    }
}

/// What polling the downloads found: whether any is still running, and the ids that just
/// finished or failed (the app plays sounds and opens windows for them).
pub struct PollResult {
    pub any_live: bool,
    pub finished: Vec<u64>,
    pub failed: Vec<u64>,
}

fn seconds_now() -> u32 {
    use chrono::Timelike;
    chrono::Local::now().num_seconds_from_midnight()
}

fn member_in(members: &HashMap<u64, Member>, id: u64) -> Member {
    members.get(&id).copied().unwrap_or(Member::Complete)
}

/// What a download's requests carry: its headers (a login, the browser's Referer and Cookie, a
/// form's Content-Type) and the options its requests go with (User-Agent, proxy, a form's body).
/// Download File Info's look-up asks with the same as the download, so both get one answer.
#[derive(Clone)]
pub struct RequestParts {
    pub headers: HeaderMap,
    pub opts: Options,
}

/// The login written in an address (`ftp://user:pass@host/…`), decoded.
pub fn url_login(url: &url::Url) -> Option<(String, String)> {
    let password = url.password()?;
    let decode = |s: &str| {
        percent_encoding::percent_decode_str(s)
            .decode_utf8_lossy()
            .into_owned()
    };
    Some((decode(url.username()), decode(password)))
}

/// `Authorization: Basic …` for "Use authorization".
pub fn auth_headers(login: Option<&(String, String)>) -> HeaderMap {
    let mut h = HeaderMap::new();
    if let Some((user, pass)) = login {
        use base64::Engine as _;
        let token = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"));
        if let Ok(v) = HeaderValue::from_str(&format!("Basic {token}")) {
            h.insert(AUTHORIZATION, v);
        }
    }
    h
}

/// The Referer a browser would send for `url` from page `referrer` (its default policy,
/// strict-origin-when-cross-origin): the whole address on the same site, only the site's origin
/// to another, nothing from https to http.
pub fn sent_referrer(url: &url::Url, referrer: &str) -> Option<String> {
    let from = url::Url::parse(referrer).ok()?;
    if !matches!(from.scheme(), "http" | "https") {
        return None;
    }
    if from.scheme() == "https" && url.scheme() == "http" {
        return None;
    }
    if from.origin() == url.origin() {
        let mut full = from.clone();
        full.set_fragment(None);
        let _ = full.set_username("");
        let _ = full.set_password(None);
        return Some(full.to_string());
    }
    Some(format!("{}/", from.origin().ascii_serialization()))
}

/// Marks each file the engine starts writing as downloaded from the web (Mark-of-the-Web, macOS'
/// quarantine), as browsers mark theirs. Only http(s) addresses (an FTP address may carry its
/// login).
pub fn mark_hook(url: &url::Url, referrer: Option<&str>) -> mdm_engine::FileHook {
    let web = matches!(url.scheme(), "http" | "https");
    let (url, referrer) = (url.to_string(), referrer.map(str::to_owned));
    mdm_engine::FileHook::new(move |path: &Path| {
        if !web {
            return;
        }
        mdm_platform::mark_downloaded(path, &url, referrer.as_deref());
    })
}

/// yt-dlp's and gallery-dl's view of the browser's request context.
fn browser_request(c: &ipc::Command) -> mdm_tools::proc::FromBrowser {
    mdm_tools::proc::FromBrowser {
        referer: c.referer.clone(),
        user_agent: c.user_agent.clone(),
        cookies: c.cookies.clone(),
        jar: c.cookie_jar.clone(),
    }
}

pub fn place_in(dir: &Path, name: &str, taken: &mut HashSet<PathBuf>) -> PathBuf {
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    let mut at = mdm_engine::naming::destination(dir, name);
    let mut n = 1u32;
    while !taken.insert(at.clone()) {
        at = mdm_engine::naming::destination(dir, &format!("{stem} ({n}){ext}"));
        n += 1;
    }
    at
}

/// A download's new place (its name from it). A name the user gave is no longer the address's
/// guess (`Item::name_from_url`); another folder alone keeps the server's say.
pub fn take_place(item: &mut Item, target: PathBuf) {
    if let Some(name) = target.file_name() {
        let name = name.to_string_lossy().into_owned();
        if name != item.file_name {
            item.name_from_url = false;
        }
        item.file_name = name;
    }
    item.dest = target;
}

/// The place a download about to start takes instead of its own, when its own belongs to
/// another row (#442): two can wait under one name (nothing was on the disk when each was added,
/// and they can start in the same moment, before the engine has reserved anything). None: it
/// stays. A row with data of its own never moves, so a resume finds its pieces.
pub fn place_to_take(item: &Item, items: &[Item]) -> Option<PathBuf> {
    if item.downloaded > 0 || item.owns_resume_state() {
        return None;
    }
    let dir = item.dest.parent()?;
    let others = || {
        items
            .iter()
            .filter(|o| o.id != item.id && o.dest.parent() == Some(dir))
    };
    let lower = |p: &Path| p.to_string_lossy().to_lowercase();
    let stem = |o: &Item| {
        o.dest
            .file_stem()
            .map(|s| s.to_string_lossy().to_lowercase())
    };
    if item.video.is_some() {
        // yt-dlp takes any `<stem>.*` there as its own (#32): the stem has to be free.
        others()
            .any(|o| stem(o) == stem(item) && o.holds_place())
            .then(|| {
                mdm_engine::naming::stem_destination(dir, &item.file_name, |s| {
                    let s = s.to_lowercase();
                    others().any(|o| stem(o).as_ref() == Some(&s) && o.holds_place())
                })
            })
    } else {
        others()
            .any(|o| lower(&o.dest) == lower(&item.dest) && o.holds_place())
            .then(|| {
                let mut taken: HashSet<PathBuf> = others()
                    .filter(|o| o.holds_place())
                    .map(|o| o.dest.clone())
                    .collect();
                taken.insert(item.dest.clone());
                place_in(dir, &item.file_name, &mut taken)
            })
    }
}

impl Core {
    /// Which account a download goes through (#44).
    pub fn debrid_route(&self) -> crate::debrid::Route {
        crate::debrid::Route::new(&self.debrid, &self.settings)
    }

    /// What a download's requests carry: its headers (a login, the browser's Referer and
    /// Cookie, a form's Content-Type) and the options its requests go with (User-Agent, proxy, a
    /// form's body). Download File Info's look-up asks with the same as the download.
    pub fn request_parts(
        &self,
        url: &url::Url,
        login: Option<&(String, String)>,
        context: &ipc::Command,
        direct: bool,
    ) -> RequestParts {
        let site_login = self.settings.site_login(url);
        let url_login = mdm_engine::ftp::is_ftp(url)
            .then(|| url_login(url))
            .flatten();
        let mut headers = auth_headers(login.or(site_login.as_ref()).or(url_login.as_ref()));
        let post_type = context.post.as_ref().and(context.post_type.as_ref());
        let referer = context
            .referer
            .as_deref()
            .and_then(|r| sent_referrer(url, r));
        for (name, v) in [
            (reqwest::header::REFERER, &referer),
            (reqwest::header::COOKIE, &context.cookies),
            (reqwest::header::CONTENT_TYPE, &post_type.cloned()),
        ] {
            if let Some(v) = v.as_deref().and_then(|v| HeaderValue::from_str(v).ok()) {
                headers.insert(name, v);
            }
        }
        let ua = self.settings.user_agent.trim();
        let opts = Options {
            max_connections: self.settings.connections_for(url),
            proxy: self.settings.proxy_for(context),
            dns: self.settings.dns(),
            route: self.route.clone(),
            // Through a debrid account when one takes this host (#44).
            unlocker: self.debrid_route().unlocker(url, direct),
            global: Some(self.global.clone()),
            host_limits: Some(self.host_limits.clone()),
            ignore_mtime: self.settings.ignore_mtime,
            server_time: self.settings.set_creation_date,
            ftp_passive: self.settings.ftp_passive,
            user_agent: match context.user_agent.as_deref() {
                Some(browser) => browser.to_owned(),
                None if ua.is_empty() => Options::default().user_agent,
                None => ua.to_owned(),
            },
            // The browser's User-Agent may be refused on this connection (#372): MDM's own
            // (Options' or the default) is tried once then, as Add URL would.
            agent_fallback: context.user_agent.is_some().then(|| {
                if ua.is_empty() {
                    Options::default().user_agent
                } else {
                    ua.to_owned()
                }
            }),
            post: context.post.clone().map(|body| body.into_bytes().into()),
            on_file_created: Some(mark_hook(url, context.referer.as_deref())),
            ..Options::default()
        };
        RequestParts { headers, opts }
    }

    /// Options → Accounts (#44): set the debrid accounts up again when the settings or the
    /// network adapter changed.
    pub fn debrid_tick(&mut self) {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for a in self.settings.debrid_accounts.iter().filter(|a| a.enabled) {
            (a.id, a.service, &a.secret).hash(&mut h);
        }
        format!("{:?}", crate::settings::own_net().proxy).hash(&mut h);
        self.settings.dns().hash(&mut h);
        self.route.hash(&mut h);
        let from = h.finish();
        if from == self.debrid_from {
            return;
        }
        self.debrid_from = from;
        let accounts: Vec<mdm_debrid::Account> = self
            .settings
            .debrid_accounts
            .iter()
            .filter(|a| a.enabled)
            .filter_map(crate::settings::DebridAccount::account)
            .collect();
        let Ok(http) = crate::debrid::http() else {
            return;
        };
        let any = !accounts.is_empty();
        self.debrid.set_accounts(accounts, http);
        if any {
            let debrid = self.debrid.clone();
            self.rt.spawn(async move {
                if let Err(e) = debrid.refresh_hosts().await {
                    tracing::warn!("debrid host lists: {e:?}");
                }
            });
        }
    }

    /// The automation API's list: every download as JSON, with its queues and categories.
    pub fn list_answer(&self, remote: bool, free: Option<u64>) -> String {
        use ipc::api::{DownloadInfo, ListAnswer, line};
        let downloads = self
            .items
            .iter()
            .map(|i| DownloadInfo {
                id: i.id,
                file: i.file_name.clone(),
                // A paired device sees no folder of the computer's, nor a password in an
                // address.
                url: if remote {
                    let mut u = i.url.clone();
                    let _ = u.set_password(None);
                    u.to_string()
                } else {
                    i.url.to_string()
                },
                path: if remote {
                    String::new()
                } else {
                    i.dest.to_string_lossy().into_owned()
                },
                status: format!("{:?}", i.status),
                size: i.size,
                downloaded: i.downloaded,
                speed: if i.live.is_some() { i.rate() as u64 } else { 0 },
                added: i.added.clone(),
                category: i.category.key().into_owned(),
                error: i.error.clone().filter(|_| i.status == Status::Error),
            })
            .collect();
        let speed: f64 = self
            .items
            .iter()
            .filter(|i| i.live.is_some())
            .map(|i| i.rate())
            .sum();
        let queues = self
            .queues
            .iter()
            .map(|q| ipc::api::QueueInfo {
                name: q.name.clone(),
                running: q.running,
                ids: q.items.clone(),
            })
            .collect();
        let categories = Category::all()
            .into_iter()
            .map(|c| ipc::api::CategoryInfo {
                key: c.key().into_owned(),
                name: c.label().into_owned(),
            })
            .collect();
        line(&ListAnswer {
            downloads,
            speed: speed as u64,
            queues,
            categories,
            free,
        })
    }

    /// Decide an automation action: what is done here (pausing), what the app must do (start,
    /// remove). The answer names each download done or skipped, as the API promises.
    pub fn act(
        &mut self,
        action: &str,
        ids: &[u64],
    ) -> Result<(ipc::api::ActAnswer, Vec<ActEffect>), ipc::api::ErrorAnswer> {
        if !matches!(action, "pause" | "start" | "remove" | "delete") {
            return Err(ipc::api::ErrorAnswer {
                error: format!("unknown action {action:?}"),
                code: None,
            });
        }
        let mut answer = ipc::api::ActAnswer::default();
        let mut effects = Vec::new();
        for &id in ids {
            let Some(item) = self.item(id) else {
                answer.skipped.push(ipc::api::Skipped {
                    id,
                    why: "no such download".into(),
                });
                continue;
            };
            let running = item.live.is_some();
            // As Resume: a finished gallery may start again (#58).
            let startable = running || item.can_start();
            let why = match action {
                "pause" if !running => Some("not running"),
                "pause" => {
                    self.stop_by_hand(id);
                    None
                }
                "start" if running => Some("already running"),
                "start" if !startable => Some("complete"),
                "start" => {
                    effects.push(ActEffect::Start(id));
                    None
                }
                _ if running => Some("running: pause it first"),
                _ => {
                    effects.push(ActEffect::Remove {
                        id,
                        files: action == "delete",
                    });
                    None
                }
            };
            match why {
                Some(why) => answer.skipped.push(ipc::api::Skipped {
                    id,
                    why: why.into(),
                }),
                None => answer.done.push(id),
            }
        }
        Ok((answer, effects))
    }

    /// Start a queue; true when it wasn't running (the app plays its sound then).
    pub fn start_queue(&mut self, q: usize) -> bool {
        let was = self.queues[q].running;
        self.queues[q].running = true;
        for id in &self.queues[q].items {
            self.stopped_by_hand.remove(id);
        }
        !was
    }

    /// Stop a queue and its running downloads.
    pub fn stop_queue(&mut self, q: usize) {
        self.queues[q].running = false;
        for id in self.queues[q].items.clone() {
            self.stop(id);
        }
    }

    /// Each download's standing in a running queue, looked up once.
    fn members(&self, now: Instant) -> HashMap<u64, Member> {
        self.items
            .iter()
            .map(|i| (i.id, self.member_of(i, now)))
            .collect()
    }

    fn member_of(&self, i: &Item, now: Instant) -> Member {
        let id = i.id;
        match i.status {
            Status::Stopped if self.stopped_by_hand.contains(&id) => Member::Held,
            Status::Queued | Status::Stopped => Member::Waiting,
            Status::Downloading => Member::Running,
            Status::Complete => Member::Complete,
            Status::Error => {
                let long_ago = now.checked_sub(queue::RETRY_DELAY).unwrap_or(now);
                let (at, tries) = self.failures.get(&id).copied().unwrap_or((long_ago, 0));
                Member::Failed { at, tries }
            }
        }
    }

    /// The scheduler's tick: start and stop queues by their schedules, then start what a
    /// running queue has room for and note queues that finished. The app starts the downloads
    /// and runs a finished queue's actions, in the returned order.
    pub fn tick_queues(&mut self) -> Vec<TickEffect> {
        use chrono::Datelike;
        let now_s = seconds_now();
        let prev = std::mem::replace(&mut self.clock, now_s);
        let today = chrono::Local::now().date_naive();
        let weekday = today.weekday().num_days_from_sunday() as usize;
        let date = (today.year(), today.month(), today.day());
        for q in 0..self.queues.len() {
            let sch = self.queues[q].schedule.clone();
            if let Some(t) = sch.start_at
                && sch.starts_on(date, weekday)
                && queue::crossed(t, prev, now_s)
            {
                self.queues[q].running = true;
            }
            if let Some(t) = sch.stop_at
                && queue::crossed(t, prev, now_s)
            {
                self.stop_queue(q);
            }
        }
        let mut effects = Vec::new();
        let now = Instant::now();
        for q in 0..self.queues.len() {
            if !self.queues[q].running {
                continue;
            }
            // Each download's standing, looked up once (a linear search per queued id before).
            let members = self.members(now);
            let starts = queue::to_start(&self.queues[q], |id| member_in(&members, id), now);
            for id in starts {
                effects.push(TickEffect::Start(id));
            }
            let members = self.members(now);
            if queue::finished(&self.queues[q], |id| member_in(&members, id)) {
                self.queues[q].running = false;
                effects.push(TickEffect::QueueDone(q));
            }
        }
        // The next wake follows the schedules (set again when they change), and a queue that
        // wakes the computer keeps it awake while it runs (#142).
        self.wake.sync(&self.queues);
        let waking = self
            .queues
            .iter()
            .find(|q| q.running && q.schedule.wake)
            .map(|q| q.name.clone());
        self.wake
            .keep_awake(waking.is_some(), waking.as_deref().unwrap_or_default());
        effects
    }

    /// Scheduler → Download limits: stop at the limit, go on when the window has room. The app
    /// starts the downloads the returned effects name and shows the notice.
    pub fn check_limits(&mut self, now: Instant) -> Vec<LimitEffect> {
        self.quota
            .record(now, self.items.iter().map(|i| (i.id, i.downloaded)));
        let s = &self.settings;
        let limit = u64::from(s.limits_mb.max(1)) << 20;
        let window = Duration::from_secs(u64::from(s.limits_hours.max(1)) * 3600);
        let change = if s.limits_on {
            self.quota.check(now, limit, window)
        } else if self.quota.blocked {
            self.quota.blocked = false;
            Some(false)
        } else {
            None
        };
        let mut effects = Vec::new();
        match change {
            Some(true) => {
                let running: Vec<u64> = self
                    .items
                    .iter()
                    .filter(|i| i.live.is_some())
                    .map(|i| i.id)
                    .collect();
                for id in running {
                    self.stop(id);
                    self.quota.held.push(id);
                }
                for id in self.quota.held.clone() {
                    if let Some(item) = self.item(id) {
                        item.status = Status::Queued;
                    }
                }
                if self.settings.limits_warn {
                    let wait = self.quota.room_in(now, limit, window);
                    effects.push(LimitEffect::Notice {
                        mb: self.settings.limits_mb,
                        hours: self.settings.limits_hours,
                        wait,
                    });
                }
                self.dirty = true;
            }
            Some(false) => {
                for id in std::mem::take(&mut self.quota.held) {
                    if let Some(item) = self.item(id) {
                        item.status = Status::Stopped;
                    }
                    effects.push(LimitEffect::Start(id));
                }
                self.dirty = true;
            }
            None => {}
        }
        effects
    }

    /// Take each download's engine snapshot into the list. A download that finished leaves the
    /// non-synchronization queues and its failure count; a failed one is counted and moved to
    /// the end. The app plays the sounds and opens the windows the result names.
    pub fn poll_items(&mut self) -> PollResult {
        let mut finished = Vec::new();
        let mut failed = Vec::new();
        let mut any_live = false;
        for i in &mut self.items {
            if i.poll() {
                self.dirty = true;
                match i.status {
                    Status::Complete => finished.push(i.id),
                    Status::Error => failed.push(i.id),
                    _ => {}
                }
            }
            any_live |= i.live.is_some();
        }
        let now = Instant::now();
        for id in &failed {
            let e = self.failures.entry(*id).or_insert((now, 0));
            *e = (now, e.1 + 1);
            for q in &mut self.queues {
                queue::requeue_failed(q, *id);
            }
        }
        for id in &finished {
            self.failures.remove(id);
            // Finished files leave download queues (synchronization queues keep them).
            for q in self.queues.iter_mut().filter(|q| !q.sync) {
                q.items.retain(|x| x != id);
            }
        }
        PollResult {
            any_live,
            finished,
            failed,
        }
    }

    /// Before a download starts: into a free place of its own if its own is another row's
    /// (`place_to_take`). The name stays the address's guess if it was one.
    pub fn claim_place(&mut self, id: u64) {
        let target = self
            .items
            .iter()
            .find(|i| i.id == id)
            .and_then(|i| place_to_take(i, &self.items));
        if let (Some(target), Some(item)) = (target, self.items.iter_mut().find(|i| i.id == id)) {
            let guessed = item.name_from_url;
            take_place(item, target);
            item.name_from_url = guessed;
            self.dirty = true;
        }
    }

    /// Start or resume a file download from the parts the app prepared. True once the engine
    /// job is live; false when it couldn't start (the row says why).
    pub fn start_file(&mut self, id: u64, parts: RequestParts, kept: Option<KeptAnswer>) -> bool {
        self.claim_place(id);
        let rt = self.rt.clone();
        let Some(item) = self.items.iter_mut().find(|i| i.id == id) else {
            return false;
        };
        let _guard = rt.enter();
        // Resume whenever there is something to resume. If that fails, say why and keep the
        // data: starting over would overwrite it, and that is only ever the user's choice.
        let state = state_path(&item.dest);
        let dl = if state.exists() {
            match Download::resume(state, parts.headers, parts.opts) {
                Ok(dl) => dl,
                Err(e) => {
                    item.status = Status::Error;
                    item.error = Some(crate::errors::describe(&e));
                    item.last_try = Some(crate::model::now_stamp());
                    self.dirty = true;
                    return false;
                }
            }
        } else {
            // Nothing to resume. A missing folder is created by the engine; a removed drive is
            // reported without touching the progress shown for the data on it.
            if item.dest.parent().is_none_or(Path::is_dir) {
                item.downloaded = 0;
            }
            // A name nobody gave (the address's guess) is the server's to give: into the folder,
            // under its Content-Disposition name (#28); the row follows the file made.
            let dest = match item.dest.parent() {
                Some(dir) if item.name_from_url => Destination::Dir(dir.to_owned()),
                _ => Destination::File(item.dest.clone()),
            };
            Download::start_with(
                Request {
                    url: item.url.clone(),
                    headers: parts.headers,
                    dest,
                },
                parts.opts,
                kept,
            )
        };
        item.note(if item.downloaded > 0 {
            // i18n: event log entry, past tense: the download resumed (just now)
            crate::i18n::t("Resumed")
        } else {
            // i18n: event log entry, past tense: the download started (just now)
            crate::i18n::t("Started")
        });
        item.live = Some(Live::new(Job::File(dl), item.downloaded));
        item.status = Status::Downloading;
        item.error = None;
        item.last_try = Some(crate::model::now_stamp());
        self.dirty = true;
        true
    }

    /// Start or resume a torrent through a debrid account (#44), from the settings' network
    /// options. True once the remote job is live; the app opens its progress window.
    pub fn start_remote(&mut self, id: u64) -> bool {
        let limit = self.settings.speed_limit();
        let Some((url, spec, own)) = self
            .items
            .iter()
            .find(|i| i.id == id)
            .and_then(|i| Some((i.url.clone(), i.remote.clone()?, i.own_limit)))
        else {
            return false;
        };
        // The files' requests: Options' connections, proxy, DNS and adapter; the service's own
        // unlocking is the torrent job's (not a host's link).
        let mut net = self
            .request_parts(&url, None, &Default::default(), true)
            .opts;
        net.speed_limit = own.or(limit);
        let rt = self.rt.clone();
        let _guard = rt.enter();
        let debrid = self.debrid.clone();
        let job = mdm_debrid::torrents::RemoteTorrent::start(
            &debrid,
            mdm_debrid::torrents::Order {
                account: spec.account,
                magnet: url,
                transfer: spec.transfer.clone(),
                folder: spec.folder.clone(),
                net,
                poll: Duration::from_secs(5),
            },
        );
        let Some(item) = self.items.iter_mut().find(|i| i.id == id) else {
            return false;
        };
        item.note(if spec.transfer.is_some() {
            crate::i18n::t("Resumed")
        } else {
            crate::i18n::t("Started")
        });
        item.live = Some(Live::new(Job::Remote(job), item.downloaded));
        item.status = Status::Downloading;
        item.error = None;
        item.last_try = Some(crate::model::now_stamp());
        self.dirty = true;
        true
    }

    /// The hashes librqbit should keep or restore in its session: every torrent in the list,
    /// except a finished one that "keep sharing" is off for or that reached a seeding limit.
    pub fn session_hashes(&self) -> HashSet<String> {
        let limits = self.settings.sharing_limits();
        self.items
            .iter()
            .filter(|i| {
                i.status != Status::Complete
                    || (self.settings.torrent_keep_sharing
                        && i.torrent.as_ref().is_some_and(|spec| {
                            sharing_limit_reached(spec, i.downloaded, limits).is_none()
                        }))
            })
            .filter_map(|i| i.torrent.as_ref().map(|t| t.hash.clone()))
            .collect()
    }

    /// Decide a torrent's start: whether it keeps sharing, its own or the Speed Limiter's limit,
    /// and the hashes the session keeps. The row is marked started here; the app runs it and
    /// hands the handle back with [`Core::attach_torrent`]. None when the row isn't a torrent.
    pub fn start_torrent(&mut self, id: u64) -> Option<StartTorrent> {
        let auto_share = self.settings.torrent_keep_sharing;
        let known = self.session_hashes();
        let limit = self.settings.speed_limit();
        let item = self.items.iter().find(|i| i.id == id)?;
        // An explicit Start on a completed download means seed it, even when automatic
        // keep sharing is off. It remains in the session until Stop or a sharing limit.
        let share = auto_share
            || item
                .size
                .is_some_and(|size| size > 0 && item.downloaded >= size);
        // Its own remembered limit first, as a file download's (#39).
        let limit = item.own_limit.or(limit);
        let spec = item.torrent.clone()?;
        let url = item.url.clone();
        let downloaded = item.downloaded;
        let item = self.items.iter_mut().find(|i| i.id == id)?;
        item.note(if downloaded > 0 {
            // i18n: event log entry, past tense: the download resumed (just now)
            crate::i18n::t("Resumed")
        } else {
            // i18n: event log entry, past tense: the download started (just now)
            crate::i18n::t("Started")
        });
        item.status = Status::Downloading;
        item.error = None;
        item.last_try = Some(crate::model::now_stamp());
        self.dirty = true;
        Some(StartTorrent {
            id,
            spec,
            url,
            known,
            limit,
            share,
        })
    }

    /// The torrent [`Core::start_torrent`] decided is running: put it behind its row.
    pub fn attach_torrent(&mut self, id: u64, handle: Box<dyn crate::torrent::TorrentHandle>) {
        if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
            item.live = Some(Live::new(Job::Torrent(handle), item.downloaded));
            self.dirty = true;
        }
    }

    /// Start a video item: yt-dlp (installed and verified) downloads it. True when the start was
    /// tried (the app opens its progress window); false when the row can't run or yt-dlp isn't
    /// installed.
    pub fn start_video(&mut self, id: u64) -> bool {
        self.claim_place(id);
        let tools = mdm_tools::tools_dir();
        let tool = tools.as_deref().and_then(mdm_tools::installed);
        let ffmpeg = tools.as_deref().and_then(mdm_tools::ffmpeg::installed);
        let rate = self.settings.speed_limit();
        let Some(item) = self.item(id) else {
            return false;
        };
        let Some(spec) = item.video.clone() else {
            return false;
        };
        let Some(tool) = tool else {
            item.status = Status::Error;
            item.error = Some(
                crate::i18n::t(
                    "yt-dlp is not installed (Tasks → Download video from a page installs it)",
                )
                .into(),
            );
            self.dirty = true;
            return false;
        };
        let folder = item.dest.parent().map(PathBuf::from).unwrap_or_default();
        let stem = item
            .dest
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "video".into());
        let _ = std::fs::create_dir_all(&folder);
        let started = mdm_tools::video::VideoJob::start(
            &tool.exe,
            item.url.as_str(),
            &spec.selector,
            &folder,
            &stem,
            rate,
            ffmpeg.as_ref().map(|f| f.exe.as_path()),
            item.size,
            &browser_request(&item.context),
            &spec.subs,
        );
        match started {
            Ok(job) => {
                item.note(crate::i18n::fmt(
                    crate::i18n::t("Started with yt-dlp {version} ({format})"),
                    &[("version", &tool.version), ("format", &spec.selector)],
                ));
                let url = item.url.clone();
                item.live = Some(Live::new(Job::Video { job, url }, item.downloaded));
                item.status = Status::Downloading;
                item.error = None;
                item.last_try = Some(crate::model::now_stamp());
            }
            Err(e) => {
                item.status = Status::Error;
                item.error = Some(e.to_string());
            }
        }
        self.dirty = true;
        true
    }

    /// Start (or start again) a gallery: gallery-dl, installed and verified. True when the start
    /// was tried (the app opens its progress window); false when the row can't run or gallery-dl
    /// isn't installed.
    pub fn start_gallery(&mut self, id: u64) -> bool {
        let tool = mdm_tools::tools_dir()
            .as_deref()
            .and_then(mdm_tools::gallery::installed);
        let rate = self.settings.speed_limit();
        let Some(item) = self.item(id) else {
            return false;
        };
        let Some(spec) = item.gallery.clone() else {
            return false;
        };
        let Some(tool) = tool else {
            item.status = Status::Error;
            item.error = Some(
                crate::i18n::t(
                    // i18n: a gallery download's error: the helper program it needs is missing
                    "gallery-dl is not installed (Add URL with a gallery's address installs it)",
                )
                .into(),
            );
            self.dirty = true;
            return false;
        };
        let ask = mdm_tools::gallery::Ask {
            limit: spec.limit,
            rate,
        };
        let started = mdm_tools::gallery::GalleryJob::start(
            &tool,
            item.url.as_str(),
            &item.dest,
            &ask,
            &browser_request(&item.context),
        );
        match started {
            Ok(job) => {
                item.note(crate::i18n::fmt(
                    // i18n: a gallery download's log: {version} = gallery-dl's version
                    crate::i18n::t("Started with gallery-dl {version}"),
                    &[("version", &tool.version)],
                ));
                let url = item.url.clone();
                // A gallery counts what it saves this run.
                item.downloaded = 0;
                item.size = None;
                item.live = Some(Live::new(Job::Gallery { job, url }, 0));
                item.status = Status::Downloading;
                item.error = None;
                item.last_try = Some(crate::model::now_stamp());
            }
            Err(e) => {
                item.status = Status::Error;
                item.error = Some(e.to_string());
            }
        }
        self.dirty = true;
        true
    }

    /// Each frame: the route connections take. When it changes, everything running stops (its
    /// connections were held to the route before, or to none; sockets that are bound can't leak,
    /// this is so they don't wait for a timeout) and the torrent session with it; what was
    /// stopped so starts again once the route lets it. Returns what the app must do: restart the
    /// torrent session, and start the downloads whose jobs have ended.
    pub fn route_tick(&mut self, route: mdm_engine::Route) -> RouteEffect {
        let mut effect = RouteEffect::default();
        if route != self.route {
            let before = std::mem::replace(&mut self.route, route);
            // The torrent session's sockets were bound as things were.
            effect.torrent_restart = !before.is_any() || !self.route.is_any();
            self.route_changed();
        }
        if matches!(self.route, mdm_engine::Route::Down { .. }) || self.adapter_paused.is_empty() {
            return effect;
        }
        // Started again once their jobs have ended.
        let ready: Vec<u64> = self
            .adapter_paused
            .iter()
            .copied()
            .filter(|id| self.items.iter().any(|i| i.id == *id && i.live.is_none()))
            .collect();
        self.adapter_paused
            .retain(|id| self.items.iter().any(|i| i.id == *id) && !ready.contains(id));
        effect.starts = ready;
        effect
    }

    /// The route changed: stop what runs, saying why, and remember it to start again.
    fn route_changed(&mut self) {
        let note = match &self.route {
            mdm_engine::Route::Down { label } => crate::i18n::fmt(
                // i18n: event log entry: the download stopped because the chosen network adapter (a VPN's, usually) went away; {adapter} its name
                crate::i18n::t("Stopped: {adapter} is not connected; it goes on when it is back"),
                &[("adapter", label)],
            ),
            // i18n: event log entry: the download stopped to start again through the network adapter as it is now (a VPN reconnected, another adapter chosen)
            _ => crate::i18n::t("Stopped: the network adapter changed; starting again").to_owned(),
        };
        let mut stopped = Vec::new();
        for i in &mut self.items {
            if let Some(l) = &i.live {
                l.dl.cancel();
                i.note(note.clone());
                stopped.push(i.id);
            }
        }
        for id in stopped {
            if !self.adapter_paused.contains(&id) {
                self.adapter_paused.push(id);
            }
        }
        self.dirty = true;
    }

    /// The browser's download panel is never shown on `host` (its menu's "never on this site").
    pub fn hide_video_panel_site(&mut self, host: &str) {
        self.settings.video_panel_sites =
            ipc::capture::add_site(&self.settings.video_panel_sites, host);
        self.dirty = true;
    }

    /// This site's downloads stay with the browser (the toolbar button's "never on this site").
    pub fn exclude_site(&mut self, host: &str) {
        self.settings.excluded_sites = ipc::capture::add_site(&self.settings.excluded_sites, host);
        self.dirty = true;
    }

    /// The download with this id, to change it in place.
    pub fn item(&mut self, id: u64) -> Option<&mut Item> {
        self.items.iter_mut().find(|i| i.id == id)
    }

    /// Stop a running download; it can resume from its state.
    pub fn stop(&mut self, id: u64) {
        if let Some(l) = self.item(id).and_then(|i| i.live.as_ref()) {
            l.dl.cancel();
        }
    }

    /// The user's Stop (or Pause): it stays stopped, even in a queue that is running (#341).
    pub fn stop_by_hand(&mut self, id: u64) {
        self.stop(id);
        self.stopped_by_hand.insert(id);
    }

    /// Stop every running download by hand.
    pub fn stop_all(&mut self) {
        let running: Vec<u64> = self
            .items
            .iter()
            .filter(|i| i.live.is_some())
            .map(|i| i.id)
            .collect();
        for id in running {
            self.stop_by_hand(id);
        }
    }

    /// Apply the Speed Limiter's "for one file" setting to every running download, file or
    /// torrent; one with a remembered limit of its own keeps it, as when it starts.
    pub fn apply_speed_limit(&self) {
        let limit = self.settings.speed_limit();
        for i in &self.items {
            if let Some(l) = &i.live {
                l.dl.set_speed_limit(i.own_limit.or(limit));
            }
        }
    }

    /// Save the list now: the saver writes it off the caller's thread.
    pub fn save(&mut self, columns: &Layout) -> Result<(), String> {
        let result = self
            .saver
            .save(
                self.next_id,
                &self.items,
                columns,
                &self.settings,
                &self.queues,
            )
            .map_err(|e| e.to_string());
        self.dirty = false;
        self.last_save = Instant::now();
        result
    }

    /// A running download keeps the list's progress fresh at least every 30 s.
    pub fn note_running(&mut self, running: bool) {
        if running && self.last_save.elapsed() > Duration::from_secs(30) {
            self.dirty = true;
        }
    }

    /// The keychain confirmed a secret: the list is saved again with its reference.
    pub fn note_secrets(&mut self, confirmed: u64) -> bool {
        if confirmed == self.secrets_confirmed {
            return false;
        }
        self.secrets_confirmed = confirmed;
        self.dirty = true;
        true
    }

    /// The saver's last write, once.
    pub fn take_save_outcome(&mut self) -> Option<Result<(), String>> {
        self.saver.outcome()
    }
}
