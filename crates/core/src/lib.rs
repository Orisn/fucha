//! Mister Download Manager's UI-independent core.
//!
//! The egui app (`mdm-app`) and Fucha's Tauri backend both depend on this crate; nothing here
//! may depend on egui or on a window.

pub mod countries;
pub mod i18n;
pub mod paths;
pub mod queue;
pub mod quota;
pub mod regional;
pub mod rss;
