//! The download session's state that needs no window: the list, its queues, the settings it is
//! saved with, and the persistence around them. The egui app keeps the widgets and dialogs and
//! reaches this state through `Deref`; Fucha's Tauri backend will drive it directly.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mdm_engine::hosts::HostLimits;
use mdm_engine::limiter::Limiter;
use mdm_ipc as ipc;

use crate::layout::Layout;
use crate::model::{Category, Item, Status};
use crate::queue::{self, Member, Queue};
use crate::quota::Quota;
use crate::settings::Settings;
use crate::store;
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

impl Core {
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
