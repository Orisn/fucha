//! A running torrent download as the app's `model::Job` holds it: only what the download list
//! and its progress window need from it (its snapshot, stopping it, its speed limit), so
//! `model.rs` can move here while the librqbit session itself stays in the app.
//!
//! What only the app's own torrent run can answer (the progress window's torrent tabs, the
//! pieces' order, the upload limit) is reached through [`TorrentHandle::as_any`]: those keep
//! their app types (mdm_torrent's), and moving them in here would drag the torrent session
//! along.

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
