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

/// MDM_OFFSCREEN=1, for screenshots and checks: nothing may touch the person's system.
pub fn offscreen() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("MDM_OFFSCREEN").is_some_and(|v| v == "1"))
}

/// Options → Sites Logins' passwords, encrypted for this Windows account (DPAPI) before the list
/// is written: another account, or a copy of the file, can't read them.
#[cfg(windows)]
pub fn protect(plain: &str) -> Option<Vec<u8>> {
    use windows_sys::Win32::Security::Cryptography::{CRYPT_INTEGER_BLOB, CryptProtectData};
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(plain.len()).ok()?,
        pbData: plain.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: `input` points at `plain` for the call (only read); `output` is filled by the
    // system with a buffer we copy and then free with LocalFree, as CryptProtectData requires.
    let ok = unsafe {
        CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
            &mut output,
        )
    };
    blob_bytes(ok, output)
}

/// The password `protect` encrypted, for this account.
#[cfg(windows)]
pub fn unprotect(sealed: &[u8]) -> Option<String> {
    use windows_sys::Win32::Security::Cryptography::{CRYPT_INTEGER_BLOB, CryptUnprotectData};
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(sealed.len()).ok()?,
        pbData: sealed.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: as in `protect`.
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
            &mut output,
        )
    };
    String::from_utf8(blob_bytes(ok, output)?).ok()
}

/// Elsewhere the account's keychain keeps the sign-in secrets (mdm-app's secrets.rs), so nothing
/// is sealed here.
#[cfg(not(windows))]
pub fn protect(_plain: &str) -> Option<Vec<u8>> {
    None
}

/// Nothing was sealed here (`protect`).
#[cfg(not(windows))]
pub fn unprotect(_sealed: &[u8]) -> Option<String> {
    None
}

/// A DPAPI result's bytes, its buffer freed.
#[cfg(windows)]
fn blob_bytes(
    ok: i32,
    blob: windows_sys::Win32::Security::Cryptography::CRYPT_INTEGER_BLOB,
) -> Option<Vec<u8>> {
    if ok == 0 || blob.pbData.is_null() {
        return None;
    }
    // SAFETY: the system gave `cbData` bytes at `pbData`, ours until LocalFree.
    let bytes = unsafe { std::slice::from_raw_parts(blob.pbData, blob.cbData as usize) }.to_vec();
    // SAFETY: the buffer came from DPAPI, which allocates with LocalAlloc.
    unsafe { windows_sys::Win32::Foundation::LocalFree(blob.pbData.cast()) };
    Some(bytes)
}

/// Mark-of-the-Web on a file being downloaded from `url` (`referrer`'s page): the zone stream
/// browsers write, so SmartScreen checks a program, Office opens a document in Protected View and
/// archive tools pass the mark on, as for the browser's own download. Written on the part file
/// as it is created (the engine renames it, the stream going along).
#[cfg(windows)]
pub fn mark_downloaded(path: &std::path::Path, url: &str, referrer: Option<&str>) {
    // Only addresses from the web: never a login's password (the engine has none in `url`).
    let clean = |u: &str| u.split(['\r', '\n']).next().unwrap_or("").to_owned();
    let mut text = String::from("[ZoneTransfer]\r\nZoneId=3\r\n");
    if let Some(r) = referrer.filter(|r| r.starts_with("http")) {
        text.push_str(&format!("ReferrerUrl={}\r\n", clean(r)));
    }
    text.push_str(&format!("HostUrl={}\r\n", clean(url)));
    let mut stream = path.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    let _ = std::fs::write(stream, text);
}

/// The counterpart of Windows' Mark-of-the-Web on a file being downloaded from `url`: macOS'
/// quarantine attribute (Gatekeeper checks a program before its first start, as for a browser's
/// download), and on Linux the freedesktop.org origin attributes file managers show.
#[cfg(not(windows))]
pub fn mark_downloaded(path: &std::path::Path, url: &str, referrer: Option<&str>) {
    #[cfg(target_os = "macos")]
    {
        let _ = referrer;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        set_xattr(
            path,
            "com.apple.quarantine",
            &format!("0081;{now:x};Mister Download Manager;"),
        );
        let _ = url;
    }
    #[cfg(not(target_os = "macos"))]
    {
        set_xattr(path, "user.xdg.origin.url", url);
        if let Some(r) = referrer.filter(|r| r.starts_with("http")) {
            set_xattr(path, "user.xdg.referrer.url", r);
        }
    }
}

/// One extended attribute; a file system without them is left as it is.
#[cfg(not(windows))]
fn set_xattr(path: &std::path::Path, name: &str, value: &str) {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let (Ok(p), Ok(n)) = (
        CString::new(path.as_os_str().as_bytes()),
        CString::new(name),
    ) else {
        return;
    };
    // SAFETY: both strings are NUL-terminated and live through the call; the value is a byte
    // slice with its length.
    #[cfg(target_os = "macos")]
    let _ = unsafe {
        libc::setxattr(
            p.as_ptr(),
            n.as_ptr(),
            value.as_ptr().cast(),
            value.len(),
            0,
            0,
        )
    };
    #[cfg(not(target_os = "macos"))]
    let _ = unsafe {
        libc::setxattr(
            p.as_ptr(),
            n.as_ptr(),
            value.as_ptr().cast(),
            value.len(),
            0,
        )
    };
}

/// `t` (local time) as the user's Windows region writes a short date and a time with seconds
/// (Control Panel → Region): `30/10/2026 00:38:09`, `2026/10/30 0:38:09`… None if Windows can't.
#[cfg(windows)]
pub fn local_date_time(t: chrono::NaiveDateTime) -> Option<String> {
    use chrono::{Datelike, Timelike};
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::Globalization::{DATE_SHORTDATE, GetDateFormatEx, GetTimeFormatEx};
    let st = SYSTEMTIME {
        wYear: t.year() as u16,
        wMonth: t.month() as u16,
        wDayOfWeek: t.weekday().num_days_from_sunday() as u16,
        wDay: t.day() as u16,
        wHour: t.hour() as u16,
        wMinute: t.minute() as u16,
        wSecond: t.second() as u16,
        wMilliseconds: 0,
    };
    let mut date = [0u16; 80];
    let mut time = [0u16; 80];
    // SAFETY: null locale = the user's; the buffers' lengths are passed in characters.
    let (d, h) = unsafe {
        (
            GetDateFormatEx(
                std::ptr::null(),
                DATE_SHORTDATE,
                &st,
                std::ptr::null(),
                date.as_mut_ptr(),
                date.len() as i32,
                std::ptr::null(),
            ),
            GetTimeFormatEx(
                std::ptr::null(),
                0,
                &st,
                std::ptr::null(),
                time.as_mut_ptr(),
                time.len() as i32,
            ),
        )
    };
    if d <= 1 || h <= 1 {
        return None;
    }
    let date = String::from_utf16_lossy(&date[..d as usize - 1]);
    let time = String::from_utf16_lossy(&time[..h as usize - 1]);
    Some(format!("{date} {time}"))
}

/// Elsewhere the language's convention (`LC_ALL`, `LC_TIME`, `LANG`) is used by
/// `mdm_core::regional`, so there is no system format to ask about here.
#[cfg(not(windows))]
pub fn local_date_time(_t: chrono::NaiveDateTime) -> Option<String> {
    None
}

/// Whether this computer can be woken by MDM, and if not, why.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Support {
    // macOS never can (see the macOS wake_support).
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    Ready,
    /// Windows' power options don't allow wake timers now (on battery: `battery`).
    TimersOff { battery: bool },
    /// Windows allows wake timers on mains power only, and this computer has a battery.
    NotOnBattery,
    /// Linux: the wake alarm is the administrator's (the guide shows how to allow it).
    NeedsAdmin,
    /// Linux: no clock chip with a wake alarm.
    NoAlarm,
    /// macOS: only an administrator can schedule a wake.
    MacAdmin,
}

impl Support {
    /// The checkbox can be ticked: waking is possible here, or becomes so in the system's own
    /// settings without an administrator (Windows' wake timers are a power option).
    pub fn can_tick(&self) -> bool {
        matches!(
            self,
            Support::Ready | Support::TimersOff { .. } | Support::NotOnBattery
        )
    }
}

/// Why setting a wake failed. The two phrases the system has no words for are named, so the
/// core can translate them with the interface's own catalogs.
#[derive(Debug)]
#[allow(dead_code)]
pub enum WakeError {
    /// Windows set the timer, but this computer can't be woken by it.
    NotSupported,
    /// Linux: another program holds the computer's one wake alarm.
    AlarmTaken,
    /// Another failure, already a plain message.
    Message(String),
}

// ---- waking the computer for a queue (#142) -----------------------------------------------

#[cfg(windows)]
mod windows {
    use super::{Support, WakeError};
    use std::mem::zeroed;

    /// Windows' "Allow wake timers" power option for the active plan (0 off, 1 on, 2 important
    /// wake timers only), on mains power (`ac`) and on battery (`dc`), and the power source now.
    struct WakeTimers {
        ac: u32,
        dc: u32,
        on_battery: bool,
        has_battery: bool,
    }

    /// Power Options → Sleep → Allow wake timers, as `powercfg /q SCHEME_CURRENT SUB_SLEEP
    /// RTCWAKE` shows it. Read only; a setting that can't be read counts as on (nothing to warn of).
    fn wake_timers() -> WakeTimers {
        use windows_sys::Win32::Foundation::LocalFree;
        use windows_sys::Win32::System::Power::{
            GetSystemPowerStatus, PowerGetActiveScheme, PowerReadACValueIndex,
            PowerReadDCValueIndex, SYSTEM_POWER_STATUS,
        };
        use windows_sys::core::GUID;
        const SUB_SLEEP: GUID = GUID::from_u128(0x238c9fa8_0aad_41ed_83f4_97be242c8f20);
        const RTCWAKE: GUID = GUID::from_u128(0xbd3b718a_0680_4d9d_8ab2_e1d2b4ac806d);
        let (mut ac, mut dc) = (1u32, 1u32);
        let mut scheme: *mut GUID = std::ptr::null_mut();
        // SAFETY: PowerGetActiveScheme allocates the GUID it returns (freed with LocalFree below);
        // the reads only write the u32 given.
        unsafe {
            if PowerGetActiveScheme(std::ptr::null_mut(), &mut scheme) == 0 && !scheme.is_null() {
                let mut v = 0u32;
                if PowerReadACValueIndex(std::ptr::null_mut(), scheme, &SUB_SLEEP, &RTCWAKE, &mut v)
                    == 0
                {
                    ac = v;
                }
                if PowerReadDCValueIndex(std::ptr::null_mut(), scheme, &SUB_SLEEP, &RTCWAKE, &mut v)
                    == 0
                {
                    dc = v;
                }
                LocalFree(scheme.cast());
            }
        }
        // SAFETY: plain data, zero is valid; a plain query into it.
        let mut st: SYSTEM_POWER_STATUS = unsafe { zeroed() };
        let ok = unsafe { GetSystemPowerStatus(&mut st) } != 0;
        WakeTimers {
            ac,
            dc,
            // ACLineStatus 0: on battery; BatteryFlag 128: no system battery, 255: unknown.
            on_battery: ok && st.ACLineStatus == 0,
            has_battery: ok && st.BatteryFlag != 128 && st.BatteryFlag != 255,
        }
    }

    /// Whether this computer can be woken now.
    pub fn wake_support() -> Support {
        // 0 off, 1 on, 2 "important wake timers only" (Windows' own, not an app's).
        let w = wake_timers();
        let now = if w.on_battery { w.dc } else { w.ac };
        if now != 1 {
            Support::TimersOff {
                battery: w.on_battery,
            }
        } else if w.has_battery && w.dc != 1 {
            Support::NotOnBattery
        } else {
            Support::Ready
        }
    }

    /// A change to the wake timer, and where its result goes.
    struct WakeCmd {
        /// When (seconds since 1970, UTC) and for which queue; None cancels.
        at: Option<(i64, String)>,
        reply: std::sync::mpsc::Sender<Result<(), WakeError>>,
    }

    /// Wake the computer at `at` (seconds since 1970, UTC; for the queue named), or cancel the
    /// wake (None). A waitable timer set to resume the computer (SetWaitableTimer, fResume): it
    /// lives in MDM's process (Windows cancels it if the thread that set it ends, so one thread
    /// of its own keeps it), lists in `powercfg /waketimers`, and goes with MDM: nothing is left
    /// behind when MDM quits or is uninstalled.
    pub fn wake_at(at: Option<i64>, queue: &str) -> Result<(), WakeError> {
        use std::sync::{Mutex, OnceLock, mpsc};
        static THREAD: OnceLock<Mutex<mpsc::Sender<WakeCmd>>> = OnceLock::new();
        let tx = THREAD.get_or_init(|| {
            let (tx, rx) = mpsc::channel();
            let _ = std::thread::Builder::new()
                .name("mdm-wake".into())
                .spawn(move || wake_thread(rx));
            Mutex::new(tx)
        });
        let (reply, answer) = mpsc::channel();
        let cmd = WakeCmd {
            at: at.map(|t| (t, queue.to_owned())),
            reply,
        };
        let sent = tx.lock().map(|tx| tx.send(cmd).is_ok()).unwrap_or(false);
        let lost = || WakeError::Message("the wake timer's thread is gone".to_owned());
        if !sent {
            return Err(lost());
        }
        answer
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap_or_else(|_| Err(lost()))
    }

    fn wake_thread(rx: std::sync::mpsc::Receiver<WakeCmd>) {
        use std::sync::mpsc::RecvTimeoutError;
        use windows_sys::Win32::Foundation::{ERROR_NOT_SUPPORTED, GetLastError, WAIT_OBJECT_0};
        use windows_sys::Win32::System::Threading::{
            CancelWaitableTimer, CreateWaitableTimerExW, SetWaitableTimer, TIMER_ALL_ACCESS,
            WaitForSingleObject,
        };
        // A synchronization timer (auto-reset): the check below sees each firing once.
        // SAFETY: no name, default security; the handle is this thread's for the process's life.
        let timer = unsafe {
            CreateWaitableTimerExW(std::ptr::null(), std::ptr::null(), 0, TIMER_ALL_ACCESS)
        };
        let mut queue = String::new();
        loop {
            match rx.recv_timeout(std::time::Duration::from_secs(1)) {
                Ok(cmd) => {
                    let result = if timer.is_null() {
                        // SAFETY: a plain query.
                        Err(WakeError::Message(format!(
                            "CreateWaitableTimerExW failed ({})",
                            unsafe { GetLastError() }
                        )))
                    } else if let Some((unix, q)) = cmd.at {
                        // FILETIME: 100 ns steps since 1601, UTC (positive: an absolute time).
                        let due: i64 = (unix + 11_644_473_600) * 10_000_000;
                        // SAFETY: our timer; no completion routine; fResume = TRUE. GetLastError
                        // right after, as the call's documentation asks.
                        let (ok, err) = unsafe {
                            let ok =
                                SetWaitableTimer(timer, &due, 0, None, std::ptr::null(), 1) != 0;
                            (ok, GetLastError())
                        };
                        queue = q;
                        if !ok {
                            Err(WakeError::Message(format!(
                                "SetWaitableTimer failed ({err})"
                            )))
                        } else if err == ERROR_NOT_SUPPORTED {
                            // Set, but this computer can't be woken by it.
                            Err(WakeError::NotSupported)
                        } else {
                            Ok(())
                        }
                    } else {
                        // SAFETY: our timer.
                        unsafe { CancelWaitableTimer(timer) };
                        Ok(())
                    };
                    let _ = cmd.reply.send(result);
                }
                Err(RecvTimeoutError::Timeout) => {
                    // SAFETY: our timer; a zero wait only looks.
                    if !timer.is_null() && unsafe { WaitForSingleObject(timer, 0) } == WAIT_OBJECT_0
                    {
                        tracing::info!("wake: the wake timer went off (queue {queue:?})");
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    /// Keep the computer from sleeping while `reason` is given (a power request, listed with its
    /// reason in `powercfg /requests`); None lets it sleep again as its power plan says. False
    /// when Windows refused.
    pub fn keep_awake(reason: Option<&str>) -> bool {
        use std::sync::Mutex;
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Power::{
            PowerClearRequest, PowerCreateRequest, PowerRequestSystemRequired, PowerSetRequest,
        };
        use windows_sys::Win32::System::Threading::{
            POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT,
        };
        static REQUEST: Mutex<Option<isize>> = Mutex::new(None);
        let mut held = REQUEST.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(h) = held.take() {
            // SAFETY: the request made below, cleared and closed once.
            unsafe {
                PowerClearRequest(h as _, PowerRequestSystemRequired);
                CloseHandle(h as _);
            }
        }
        let Some(reason) = reason else {
            return true;
        };
        let mut text: Vec<u16> = reason.encode_utf16().chain(Some(0)).collect();
        // SAFETY: plain data, zero is valid.
        let mut ctx: REASON_CONTEXT = unsafe { zeroed() };
        // POWER_REQUEST_CONTEXT_VERSION is 0.
        ctx.Version = 0;
        ctx.Flags = POWER_REQUEST_CONTEXT_SIMPLE_STRING;
        ctx.Reason.SimpleReasonString = text.as_mut_ptr();
        // SAFETY: `ctx` and the string it points to live through the call (Windows copies it).
        let h = unsafe { PowerCreateRequest(&ctx) };
        if h.is_null() || h as isize == -1 {
            tracing::warn!("wake: PowerCreateRequest failed");
            return false;
        }
        // SAFETY: the request just made; closed if it can't be set.
        if unsafe { PowerSetRequest(h, PowerRequestSystemRequired) } == 0 {
            tracing::warn!("wake: PowerSetRequest failed");
            unsafe { CloseHandle(h) };
            return false;
        }
        *held = Some(h as isize);
        true
    }
}

#[cfg(windows)]
pub use windows::{keep_awake, wake_at, wake_support};

#[cfg(target_os = "linux")]
mod linux {
    use super::{Support, WakeError};
    use std::path::Path;

    /// Linux: the clock chip's wake alarm, in seconds since 1970. Root's alone unless an
    /// administrator lets users write it (docs/queues-and-scheduler.md shows a tmpfiles.d line).
    /// Without it nothing else wakes a sleeping Linux for a program: a systemd user timer's
    /// `WakeSystem=` needs the system manager, rtcwake needs root, alarm timers CAP_WAKE_ALARM.
    const WAKEALARM: &str = "/sys/class/rtc/rtc0/wakealarm";

    /// Whether MDM may wake this computer.
    pub fn wake_support() -> Support {
        let path = Path::new(WAKEALARM);
        if !path.exists() {
            return Support::NoAlarm;
        }
        // Opening it for writing changes nothing: it only asks whether we may.
        if std::fs::OpenOptions::new().write(true).open(path).is_ok() {
            Support::Ready
        } else {
            Support::NeedsAdmin
        }
    }

    /// Seconds to add to a system time to have the clock chip's: the chip may keep local time
    /// (/etc/adjtime: LOCAL), or have drifted from the system's clock (a virtual machine's was a
    /// minute and a half behind). The alarm is set in the chip's time, as rtcwake does.
    fn rtc_offset() -> i64 {
        let chip = std::fs::read_to_string("/sys/class/rtc/rtc0/since_epoch")
            .ok()
            .and_then(|s| s.trim().parse::<i64>().ok());
        match chip {
            Some(chip) => chip - chrono::Utc::now().timestamp(),
            None => 0,
        }
    }

    /// Wake the computer at `at` (seconds since 1970, UTC), or cancel MDM's wake (None).
    pub fn wake_at(at: Option<i64>, _queue: &str) -> Result<(), WakeError> {
        // The alarm MDM set (in the chip's time), to tell it from another program's: kept in the
        // data folder, so an alarm left by an MDM that was killed (or signed out of) is still ours.
        let file = mdm_ipc::data_dir().map(|d| d.join("wake-alarm"));
        let mut ours = file
            .as_ref()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .and_then(|s| s.trim().parse::<i64>().ok());
        let keep = |v: Option<i64>| {
            if let Some(f) = &file {
                let _ = match v {
                    Some(v) => std::fs::write(f, v.to_string()),
                    None => std::fs::remove_file(f),
                };
            }
        };
        let set_now = std::fs::read_to_string(WAKEALARM)
            .ok()
            .and_then(|s| s.trim().parse::<i64>().ok());
        let want = at.map(|t| t + rtc_offset());
        // The same alarm within a couple of seconds (the two clocks are read a moment apart).
        let close = |a: i64, b: i64| (a - b).abs() <= 2;
        match (set_now, want) {
            (Some(now), Some(w)) if close(now, w) => {
                if ours != Some(now) {
                    keep(Some(now));
                }
                return Ok(());
            }
            (None, None) => {
                if ours.is_some() {
                    keep(None);
                }
                return Ok(());
            }
            _ => {}
        }
        // There is one alarm: one that another program (rtcwake, a script) set stays theirs.
        if let Some(theirs) = set_now
            && ours.is_none_or(|o| !close(o, theirs))
        {
            if want.is_none() {
                return Ok(());
            }
            return Err(WakeError::AlarmTaken);
        }
        let write =
            |v: &str| std::fs::write(WAKEALARM, v).map_err(|e| WakeError::Message(e.to_string()));
        // An alarm is replaced by clearing it first (writing over it is refused).
        if set_now.is_some() {
            write("0")?;
        }
        ours = None;
        if let Some(v) = want {
            write(&v.to_string())?;
            ours = Some(v);
        }
        keep(ours);
        Ok(())
    }

    /// Keep the computer from sleeping while `reason` is given; None lets it sleep again as the
    /// system's settings say. Linux: logind's sleep and idle inhibitor, held by its file
    /// descriptor (`systemd-inhibit --list` shows it with its reason); a user in the active
    /// session may take it without a password. False when logind refused, or didn't answer within
    /// 25 s. Called on wake::Holder's thread, never the window's: logind can be slow right after a
    /// resume.
    pub fn keep_awake(reason: Option<&str>) -> bool {
        use std::sync::Mutex;
        static LOCK: Mutex<Option<zbus::zvariant::OwnedFd>> = Mutex::new(None);
        let mut held = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Closing the descriptor ends the inhibitor.
        *held = None;
        let Some(reason) = reason else {
            return true;
        };
        let take = || -> zbus::Result<zbus::zvariant::OwnedFd> {
            let conn = zbus::blocking::connection::Builder::system()?
                .method_timeout(std::time::Duration::from_secs(25))
                .build()?;
            let logind = zbus::blocking::Proxy::new(
                &conn,
                "org.freedesktop.login1",
                "/org/freedesktop/login1",
                "org.freedesktop.login1.Manager",
            )?;
            logind.call(
                "Inhibit",
                &("sleep:idle", "Mister Download Manager", reason, "block"),
            )
        };
        match take() {
            Ok(fd) => {
                *held = Some(fd);
                true
            }
            Err(e) => {
                tracing::warn!("wake: logind's inhibitor refused ({e})");
                false
            }
        }
    }
}

#[cfg(target_os = "linux")]
pub use linux::{keep_awake, wake_at, wake_support};

#[cfg(target_os = "macos")]
mod macos {
    use super::{Support, WakeError};
    use std::process::{Command, Stdio};

    /// macOS: `pmset schedule wake` and IOPMSchedulePowerEvent need an administrator, and MDM
    /// never asks for one by itself.
    pub fn wake_support() -> Support {
        Support::MacAdmin
    }

    pub fn wake_at(at: Option<i64>, _queue: &str) -> Result<(), WakeError> {
        match at {
            None => Ok(()),
            Some(_) => Err(WakeError::Message(
                "macOS lets only an administrator schedule a wake".to_owned(),
            )),
        }
    }

    /// macOS: `caffeinate -i` (no idle sleep) for as long as MDM runs, ended when not needed.
    pub fn keep_awake(reason: Option<&str>) -> bool {
        use std::sync::Mutex;
        static CHILD: Mutex<Option<std::process::Child>> = Mutex::new(None);
        let mut held = CHILD.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(mut c) = held.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        if reason.is_some() {
            *held = Command::new("caffeinate")
                .args(["-i", "-w", &std::process::id().to_string()])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .ok();
            return held.is_some();
        }
        true
    }
}

#[cfg(target_os = "macos")]
pub use macos::{keep_awake, wake_at, wake_support};

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod unsupported {
    use super::{Support, WakeError};

    pub fn wake_support() -> Support {
        Support::NeedsAdmin
    }

    pub fn wake_at(_at: Option<i64>, _queue: &str) -> Result<(), WakeError> {
        Err(WakeError::Message(
            "waking the computer isn't supported on this system".to_owned(),
        ))
    }

    pub fn keep_awake(_reason: Option<&str>) -> bool {
        true
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub use unsupported::{keep_awake, wake_at, wake_support};
