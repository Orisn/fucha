//! A running torrent download as the app's `model::Job` holds it: only what the download list
//! and its progress window need from it (its snapshot, stopping it, its speed limit), so
//! `model.rs` can move here while the librqbit session itself stays in the app.
//!
//! What only the app's own torrent run can answer (the progress window's torrent tabs, the
//! pieces' order, the upload limit) is reached through [`TorrentHandle::as_any`]: those keep
//! their app types (mdm_torrent's), and moving them in here would drag the torrent session
//! along.
//!
//! Also the torrent settings the app's settings hold without a session: the seeding limits, the
//! blocklist file's name, and how a session is bound to the chosen network adapter.

/// A running torrent download (`model::Job::Torrent`).
pub trait TorrentHandle: Send {
    /// The torrent as the list and the progress window see it.
    fn snapshot(&self) -> mdm_engine::Snapshot;

    /// Stop it: a completed run leaves the session; an incomplete one pauses to resume later.
    fn cancel(&self);

    /// Its own download limit, bytes/s (None: no limit of its own).
    fn set_speed_limit(&self, rate: Option<u64>);

    /// The limit [`Self::set_speed_limit`] last set.
    fn speed_limit(&self) -> Option<u64>;

    /// The app's torrent run behind this handle (`torrents::Run`), for what only it knows:
    /// the progress window's torrent tabs, the pieces' order, the upload limit.
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Torrent settings' seeding limits (`Settings::sharing_limits`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SharingLimits {
    /// Stop at this share ratio (sent ÷ what is there).
    pub ratio: Option<f64>,
    /// Stop after this many seconds of sharing once finished.
    pub secs: Option<u64>,
}

/// The blocklist as MDM converted it (mdm_torrent::blocklist), in the torrent data folder.
pub const BLOCKLIST_FILE: &str = "blocklist.p2p";

/// How the torrent session is held to the network adapter as it is (#35, #143): None without
/// one chosen; else why torrents can't start (it isn't connected).
pub fn route_bind(route: &mdm_engine::Route) -> Result<Option<mdm_torrent::Bind>, String> {
    let down = |label: &str| {
        crate::i18n::fmt(
            // i18n: an error: the network adapter chosen in Options (a VPN's, usually) is off or gone; {adapter} its name
            crate::i18n::t("the network adapter {adapter} is not connected"),
            &[("adapter", label)],
        )
    };
    match route {
        mdm_engine::Route::Any => Ok(None),
        mdm_engine::Route::Down { label } => Err(down(label)),
        // Windows: its address (strong host model, see mdm_engine::net); IPv4 when it has one.
        #[cfg(windows)]
        mdm_engine::Route::Bound(b) => {
            b.v4.map(std::net::IpAddr::V4)
                .or(b.v6.map(std::net::IpAddr::V6))
                .map(|ip| Some(mdm_torrent::Bind::Address(ip)))
                .ok_or_else(|| down(&b.label))
        }
        // Linux and macOS: its device.
        #[cfg(not(windows))]
        mdm_engine::Route::Bound(b) => Ok(Some(mdm_torrent::Bind::Device {
            name: b.name.clone(),
            v4_only: b.v6.is_none(),
        })),
    }
}
