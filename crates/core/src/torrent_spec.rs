//! A torrent download as the list keeps it (downloads.json; M15, D-014): which torrent, which
//! files, where, what it uploaded, and the order its pieces are downloaded in. The app's torrent
//! session (crates/app/src/torrents.rs) reads it from here.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// What a torrent download keeps in the list (downloads.json).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TorrentSpec {
    /// The info hash, 40 hex digits; the .torrent itself is kept in the app's torrent data
    /// folder (`torrent_file`).
    pub hash: String,
    /// The chosen files (indexes into the torrent's list); None: all of them.
    pub files: Option<Vec<usize>>,
    /// Where its files go (a multi-file torrent's own folder included).
    pub folder: PathBuf,
    /// Bytes sent to peers, in all its runs (librqbit counts only the current one: from 0
    /// again at each resume and each start of the app). Added up by the app's accountant.
    #[serde(default)]
    pub uploaded: u64,
    /// Seconds it has spent sharing once finished, in all its runs.
    #[serde(default)]
    pub seeded_secs: u64,
    /// Peers connected to it now (#357), for the list's "Peers" column; every connected peer,
    /// seeders and leechers alike. Live only: never saved, and zero whenever there is no torrent
    /// session or the session has no numbers for it.
    #[serde(skip)]
    pub peers: usize,
    /// Its own upload limit, bytes/s (None: only Torrent settings' limit for all torrents).
    #[serde(default)]
    pub upload_limit: Option<u64>,
    /// Downloaded in order (#39): its files' start first, to watch a video while it
    /// downloads. Off: the rarest pieces first, as BitTorrent clients usually do (better for
    /// the swarm, and no slower). Torrents kept from before have it off.
    #[serde(default)]
    pub in_order: bool,
    /// In order: each file's first and last pieces first (a video's index is often at its
    /// end, and a player reads it before anything plays).
    #[serde(default = "first_and_last_first")]
    pub ends_first: bool,
}

fn first_and_last_first() -> bool {
    true
}

impl TorrentSpec {
    /// The order its pieces are downloaded in (#39).
    pub fn order(&self) -> mdm_torrent::PieceOrder {
        if self.in_order {
            mdm_torrent::PieceOrder::InOrder {
                first_and_last_first: self.ends_first,
            }
        } else {
            mdm_torrent::PieceOrder::RarestFirst
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_spec() -> TorrentSpec {
        TorrentSpec {
            hash: "h".into(),
            files: None,
            folder: "f".into(),
            uploaded: 0,
            seeded_secs: 0,
            peers: 0,
            upload_limit: None,
            in_order: false,
            ends_first: true,
        }
    }

    /// #357: a torrent's live peer count isn't kept across a restart: it isn't saved and an old
    /// value in the file is ignored, so a stopped torrent can't show a stale count.
    #[test]
    fn a_torrents_peers_are_not_saved() {
        let mut t = a_spec();
        t.peers = 7;
        let saved = serde_json::to_value(&t).unwrap();
        assert!(saved.get("peers").is_none(), "the count isn't written");
        let mut with_old = saved.clone();
        with_old["peers"] = 9.into();
        with_old["seeders"] = 9.into();
        let loaded: TorrentSpec = serde_json::from_value(with_old).unwrap();
        assert_eq!(
            loaded.peers, 0,
            "an old value in the file doesn't come back"
        );
    }

    /// #39: a torrent kept from before downloads the rarest pieces first; asked, in order, with
    /// each file's first and last pieces first unless that is turned off.
    #[test]
    fn the_piece_order_kept_and_chosen() {
        let old: TorrentSpec =
            serde_json::from_str(r#"{"hash":"h","files":null,"folder":"f"}"#).unwrap();
        assert_eq!((old.in_order, old.ends_first), (false, true));
        assert_eq!(old.order(), mdm_torrent::PieceOrder::RarestFirst);
        let mut spec = old.clone();
        spec.in_order = true;
        assert_eq!(
            spec.order(),
            mdm_torrent::PieceOrder::InOrder {
                first_and_last_first: true
            }
        );
        spec.ends_first = false;
        assert_eq!(
            spec.order(),
            mdm_torrent::PieceOrder::InOrder {
                first_and_last_first: false
            }
        );
        let back: TorrentSpec =
            serde_json::from_str(&serde_json::to_string(&spec).unwrap()).unwrap();
        assert_eq!(back, spec);
    }
}
