//! The download session's state that needs no window: the list, its queues, the settings it is
//! saved with, and the persistence around them. The egui app keeps the widgets and dialogs and
//! reaches this state through `Deref`; Fucha's Tauri backend will drive it directly.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mdm_engine::hosts::HostLimits;
use mdm_engine::limiter::Limiter;

use crate::layout::Layout;
use crate::model::Item;
use crate::queue::Queue;
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

impl Core {
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
