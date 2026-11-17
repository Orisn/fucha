//! Paths as the user types them in a dialog: a relative folder is under their home folder.

use std::path::PathBuf;

/// `s` as a path: a relative one is under the user's home folder.
pub fn resolve_path(s: &str) -> PathBuf {
    let p = PathBuf::from(s.trim());
    if p.is_absolute() {
        p
    } else {
        dirs::home_dir().map_or(p.clone(), |h| h.join(p))
    }
}
