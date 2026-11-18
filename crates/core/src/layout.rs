//! The download list's column layout (order, widths, visibility and sort), the part saved with
//! the list. The app paints the columns; the core keeps the data.

use serde::{Deserialize, Serialize};

/// Columns, by their fixed data slot in a row: IDM's eight, then the user's additions:
/// "Added", "Source", and a torrent's "Peers" and "Share ratio".
pub const COLS: usize = 12;
/// The "Added" column's data slot.
pub const ADDED: usize = 8;
/// The "Source" column's data slot: its cell is "Web" or "Torrent", painted as a chip.
pub const SOURCE: usize = 9;
/// The "Peers" column's data slot (a torrent's connected peers; blank for anything else).
pub const PEERS: usize = 10;
/// The "Share ratio" column's data slot (a torrent's sent over taken; blank otherwise).
pub const RATIO: usize = 11;

/// IDM's default column widths (and ours for Added, room for a 12-hour clock's
/// `Oct 28 9:05:01 AM 2026`, Source, Peers and Share ratio).
pub const DEFAULT_WIDTHS: [f32; COLS] = [
    150.0, 20.0, 75.0, 65.0, 75.0, 95.0, 65.0, 200.0, 145.0, 62.0, 64.0, 92.0,
];
/// Default display order: Source first, then IDM's, a torrent's two columns after its rate, and
/// Added after Last Try Date.
pub const DEFAULT_ORDER: [usize; COLS] = [SOURCE, 0, 1, 2, 3, 4, 5, PEERS, RATIO, 6, ADDED, 7];

/// The part of the column state that is saved with the download list.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub order: Vec<usize>,
    /// Lists (not fixed arrays), so layouts saved before a column was added still load.
    pub widths: Vec<f32>,
    pub visible: Vec<bool>,
    pub sort: Option<(usize, bool)>,
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            order: DEFAULT_ORDER.to_vec(),
            widths: DEFAULT_WIDTHS.to_vec(),
            visible: vec![true; COLS],
            sort: None,
        }
    }
}
