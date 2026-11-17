//! Mister Download Manager's operating-system layer.
//!
//! Only this crate calls the system's own APIs for the UI-independent core; `mdm-app`'s
//! `win32.rs` and `unix.rs` keep the window, tray and theme calls.
#![allow(unsafe_code)]

use serde::{Deserialize, Serialize};

/// The order of a date's day, month and year.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DateOrder {
    Dmy,
    #[default]
    Mdy,
    Ymd,
}

/// The user's region (Control Panel → Region): the short date's order, whether the clock is
/// 24-hour, and the first day of the week (0 = Sunday). None if the system can't say.
#[cfg(windows)]
pub fn region_formats() -> Option<(DateOrder, bool, u8)> {
    use windows_sys::Win32::Globalization::{
        GetLocaleInfoEx, LOCALE_IFIRSTDAYOFWEEK, LOCALE_SSHORTDATE, LOCALE_STIMEFORMAT,
    };
    let info = |what: u32| -> Option<String> {
        let mut buf = [0u16; 80];
        // SAFETY: null locale = the user's; the buffer's length is passed in characters.
        let n =
            unsafe { GetLocaleInfoEx(std::ptr::null(), what, buf.as_mut_ptr(), buf.len() as i32) };
        (n > 1).then(|| String::from_utf16_lossy(&buf[..n as usize - 1]))
    };
    let date = info(LOCALE_SSHORTDATE)?;
    let at = |c: char| date.find(c).unwrap_or(usize::MAX);
    let (d, m, y) = (at('d'), at('M'), at('y'));
    let order = if y < m && y < d {
        DateOrder::Ymd
    } else if m < d {
        DateOrder::Mdy
    } else {
        DateOrder::Dmy
    };
    // "HH:mm:ss" (24-hour) or "h:mm:ss tt".
    let clock24 = info(LOCALE_STIMEFORMAT).is_some_and(|t| t.contains('H'));
    // Windows counts from Monday (0) to Sunday (6).
    let first = info(LOCALE_IFIRSTDAYOFWEEK)
        .and_then(|s| s.trim().parse::<u8>().ok())
        .map_or(0, |n| (n + 1) % 7);
    Some((order, clock24, first))
}

/// Elsewhere the language's convention (`LC_ALL`, `LC_TIME`, `LANG`) is read by
/// `mdm_core::regional`, which asks this only on Windows.
#[cfg(not(windows))]
pub fn region_formats() -> Option<(DateOrder, bool, u8)> {
    None
}
