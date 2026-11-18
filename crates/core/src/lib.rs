//! Mister Download Manager's UI-independent core.
//!
//! The egui app (`mdm-app`) and Fucha's Tauri backend both depend on this crate; nothing here
//! may depend on egui or on a window.

pub mod countries;
pub mod errors;
pub mod i18n;
pub mod model;
pub mod paths;
pub mod queue;
pub mod quota;
pub mod regional;
pub mod rss;
pub mod torrent;
pub mod torrent_spec;
pub mod ui;
pub mod wake;
pub mod whereis;
