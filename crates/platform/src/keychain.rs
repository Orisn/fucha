//! The system's keychain (#44): Windows Credential Manager, the Secret Service on Linux (GNOME
//! Keyring, KeePassXC, KDE's wallet in its Secret Service mode) and the macOS login keychain.
//!
//! The settings keep a reference in a secret's place and the secret itself lives here; the
//! vault's logic is `mdm_core::secrets`, which asks these functions.
//!
//! - Windows: generic credentials named `<service>/<account>`, for this Windows user, roaming
//!   with the profile as MDM's data folder (%APPDATA%) does. Credential Manager (Control Panel →
//!   Credential Manager → Windows Credentials) shows them, the label as their user name, and can
//!   remove them.
//! - Linux: the Secret Service over D-Bus, its default collection only, and only while it is
//!   unlocked: MDM never unlocks it nor answers a prompt (desktops that log in on their own
//!   often have a locked keyring, #164), so a locked or missing keyring leaves the secrets in
//!   the owner-only settings.
//! - macOS: the login keychain's generic passwords (SecItemAdd / SecItemCopyMatching, through
//!   security-framework).
//!
//! Nothing here writes a secret to the log: only keys, references and error codes.

/// Why the keychain didn't do something. Never holds a secret: an error code at most.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeychainFault {
    /// No keychain here (no Secret Service running, no session).
    Missing,
    /// It is there but locked; opening it would prompt the user.
    Locked,
    /// Anything else, as the system said it (a code).
    Failed(String),
}

impl std::fmt::Display for KeychainFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeychainFault::Missing => f.write_str("no keychain"),
            KeychainFault::Locked => f.write_str("the keychain is locked"),
            KeychainFault::Failed(why) => f.write_str(why),
        }
    }
}

/// Whether the keychain can be used now, without asking the user anything.
pub fn probe(service: &str) -> Result<(), KeychainFault> {
    imp::probe(service)
}

/// The secret of `service`'s `account`; None: no such entry.
pub fn read(service: &str, account: &str) -> Result<Option<String>, KeychainFault> {
    imp::read(service, account)
}

/// Create or replace, with `label` naming the entry in the system's own UI.
pub fn write(service: &str, account: &str, label: &str, secret: &str) -> Result<(), KeychainFault> {
    imp::write(service, account, label, secret)
}

/// Delete an entry; deleting what isn't there is no fault.
pub fn delete(service: &str, account: &str) -> Result<(), KeychainFault> {
    imp::delete(service, account)
}

/// The accounts of `service`'s entries (only these: never another program's).
pub fn list(service: &str) -> Result<Vec<String>, KeychainFault> {
    imp::list(service)
}

#[cfg(windows)]
mod imp {
    use super::KeychainFault;

    fn target_name(service: &str, account: &str) -> Vec<u16> {
        format!("{service}/{account}")
            .encode_utf16()
            .chain(Some(0))
            .collect()
    }

    /// The last error as a fault: no logon session (a service account) means no keychain.
    fn fault(what: &str) -> KeychainFault {
        use windows_sys::Win32::Foundation::{ERROR_NO_SUCH_LOGON_SESSION, GetLastError};
        // SAFETY: no arguments; reads this thread's last error.
        let code = unsafe { GetLastError() };
        if code == ERROR_NO_SUCH_LOGON_SESSION {
            KeychainFault::Missing
        } else {
            KeychainFault::Failed(format!("{what}: Windows error {code}"))
        }
    }

    fn not_found() -> bool {
        use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, GetLastError};
        // SAFETY: no arguments.
        unsafe { GetLastError() == ERROR_NOT_FOUND }
    }

    /// Whether Credential Manager answers (listing MDM's own entries).
    pub fn probe(service: &str) -> Result<(), KeychainFault> {
        list(service).map(|_| ())
    }

    pub fn read(service: &str, account: &str) -> Result<Option<String>, KeychainFault> {
        use windows_sys::Win32::Security::Credentials::{
            CRED_TYPE_GENERIC, CREDENTIALW, CredFree, CredReadW,
        };
        let target = target_name(service, account);
        let mut cred: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: `target` is a NUL-terminated UTF-16 string alive for the call; `cred` receives
        // a buffer the system allocated, freed below with CredFree.
        let ok = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut cred) };
        if ok == 0 {
            return if not_found() {
                Ok(None)
            } else {
                Err(fault("CredReadW"))
            };
        }
        // SAFETY: on success `cred` points at a CREDENTIALW whose blob holds
        // `CredentialBlobSize` bytes; both are ours until CredFree.
        let bytes = unsafe {
            let c = &*cred;
            let bytes = if c.CredentialBlob.is_null() {
                Vec::new()
            } else {
                std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize).to_vec()
            };
            CredFree(cred.cast());
            bytes
        };
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| KeychainFault::Failed("not UTF-8".into()))
    }

    /// Stored as UTF-8. Credential Manager takes at most 2560 bytes: a longer secret stays in
    /// the settings, sealed with DPAPI.
    pub fn write(
        service: &str,
        account: &str,
        label: &str,
        secret: &str,
    ) -> Result<(), KeychainFault> {
        use windows_sys::Win32::Security::Credentials::{
            CRED_MAX_CREDENTIAL_BLOB_SIZE, CRED_PERSIST_ENTERPRISE, CRED_TYPE_GENERIC, CREDENTIALW,
            CredWriteW,
        };
        if secret.len() > CRED_MAX_CREDENTIAL_BLOB_SIZE as usize {
            return Err(KeychainFault::Failed(
                "longer than Credential Manager takes".into(),
            ));
        }
        let mut target = target_name(service, account);
        let mut user: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
        let mut blob = secret.as_bytes().to_vec();
        // SAFETY: all-zero is a valid CREDENTIALW (null pointers, zero sizes).
        let mut cred: CREDENTIALW = unsafe { std::mem::zeroed() };
        cred.Type = CRED_TYPE_GENERIC;
        cred.TargetName = target.as_mut_ptr();
        cred.UserName = user.as_mut_ptr();
        cred.CredentialBlobSize = blob.len() as u32;
        cred.CredentialBlob = blob.as_mut_ptr();
        cred.Persist = CRED_PERSIST_ENTERPRISE;
        // SAFETY: every pointer in `cred` points at a buffer alive for the call (only read).
        let ok = unsafe { CredWriteW(&cred, 0) };
        let failure = (ok == 0).then(|| fault("CredWriteW"));
        // The plain copy doesn't linger in this buffer.
        blob.iter_mut().for_each(|b| *b = 0);
        failure.map_or(Ok(()), Err)
    }

    pub fn delete(service: &str, account: &str) -> Result<(), KeychainFault> {
        use windows_sys::Win32::Security::Credentials::{CRED_TYPE_GENERIC, CredDeleteW};
        let target = target_name(service, account);
        // SAFETY: `target` is NUL-terminated and alive for the call.
        let ok = unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) };
        if ok == 0 && !not_found() {
            return Err(fault("CredDeleteW"));
        }
        Ok(())
    }

    /// The accounts of `service`'s generic credentials (the filter `<service>/*`: only MDM's;
    /// the others' blobs are never read).
    pub fn list(service: &str) -> Result<Vec<String>, KeychainFault> {
        use windows_sys::Win32::Security::Credentials::{
            CRED_TYPE_GENERIC, CREDENTIALW, CredEnumerateW, CredFree,
        };
        let filter: Vec<u16> = format!("{service}/*")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut count = 0u32;
        let mut creds: *mut *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: `filter` is NUL-terminated; `creds` receives an array of `count` pointers the
        // system allocated, freed below with CredFree.
        let ok = unsafe { CredEnumerateW(filter.as_ptr(), 0, &mut count, &mut creds) };
        if ok == 0 {
            return if not_found() {
                Ok(Vec::new())
            } else {
                Err(fault("CredEnumerateW"))
            };
        }
        let prefix = format!("{service}/");
        let mut accounts = Vec::new();
        // SAFETY: on success `creds` holds `count` valid CREDENTIALW pointers, ours until
        // CredFree; each TargetName is a NUL-terminated UTF-16 string.
        unsafe {
            for i in 0..count as usize {
                let c = &**creds.add(i);
                if c.Type != CRED_TYPE_GENERIC || c.TargetName.is_null() {
                    continue;
                }
                let mut n = 0;
                while *c.TargetName.add(n) != 0 {
                    n += 1;
                }
                let name = String::from_utf16_lossy(std::slice::from_raw_parts(c.TargetName, n));
                if let Some(account) = name.strip_prefix(&prefix) {
                    accounts.push(account.to_owned());
                }
            }
            CredFree(creds.cast());
        }
        Ok(accounts)
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use zbus::blocking::{Connection, Proxy};
    use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

    use super::KeychainFault;

    const BUS: &str = "org.freedesktop.secrets";
    const ROOT: &str = "/org/freedesktop/secrets";
    const SERVICE: &str = "org.freedesktop.Secret.Service";
    const COLLECTION: &str = "org.freedesktop.Secret.Collection";
    const ITEM: &str = "org.freedesktop.Secret.Item";
    const SESSION: &str = "org.freedesktop.Secret.Session";
    /// Groups MDM's entries in Seahorse and the like.
    const SCHEMA: &str = "io.github.anefzaoui.misterdownloadmanager.Secret";

    /// The D-Bus error's kind; its text (the service's own words) is never kept.
    fn fault(e: zbus::Error) -> KeychainFault {
        let name = match &e {
            zbus::Error::MethodError(name, _, _) => name.as_str().to_owned(),
            zbus::Error::FDO(f) => zbus::DBusError::name(&**f).as_str().to_owned(),
            zbus::Error::InputOutput(_)
            | zbus::Error::Address(_)
            | zbus::Error::Connection(..)
            | zbus::Error::Handshake(_) => return KeychainFault::Missing,
            _ => return KeychainFault::Failed("D-Bus call failed".into()),
        };
        if name == "org.freedesktop.Secret.Error.IsLocked" {
            KeychainFault::Locked
        } else if name.starts_with("org.freedesktop.DBus.Error.") {
            // No such service, none to start, no such object or method: no keyring.
            KeychainFault::Missing
        } else {
            KeychainFault::Failed(name)
        }
    }

    /// The session bus, once (a call answers within 5 s or is taken as no keyring).
    fn connection() -> Result<Connection, KeychainFault> {
        static CONN: Mutex<Option<Connection>> = Mutex::new(None);
        let mut held = CONN.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(c) = &*held {
            return Ok(c.clone());
        }
        let conn = zbus::blocking::connection::Builder::session()
            .and_then(|b| b.method_timeout(std::time::Duration::from_secs(5)).build())
            .map_err(|_| KeychainFault::Missing)?;
        *held = Some(conn.clone());
        Ok(conn)
    }

    fn proxy<'a>(
        conn: &Connection,
        path: &'a str,
        iface: &'a str,
    ) -> Result<Proxy<'a>, KeychainFault> {
        zbus::blocking::proxy::Builder::<Proxy<'a>>::new(conn)
            .destination(BUS)
            .and_then(|b| b.path(path))
            .and_then(|b| b.interface(iface))
            .map(|b| b.cache_properties(zbus::proxy::CacheProperties::No))
            .and_then(|b| b.build())
            .map_err(fault)
    }

    /// A session with the service ("plain": the secret crosses only the user's own session bus),
    /// closed when dropped.
    struct Session {
        conn: Connection,
        path: OwnedObjectPath,
    }

    impl Session {
        fn open() -> Result<Session, KeychainFault> {
            let conn = connection()?;
            let (_, path): (OwnedValue, OwnedObjectPath) = proxy(&conn, ROOT, SERVICE)?
                .call("OpenSession", &("plain", Value::from("")))
                .map_err(fault)?;
            Ok(Session { conn, path })
        }

        fn path(&self) -> ObjectPath<'_> {
            self.path.as_ref()
        }

        /// The default collection, only while it is unlocked (never unlocked from here).
        fn collection(&self) -> Result<OwnedObjectPath, KeychainFault> {
            let path: OwnedObjectPath = proxy(&self.conn, ROOT, SERVICE)?
                .call("ReadAlias", &("default",))
                .map_err(fault)?;
            if path.as_str() == "/" {
                return Err(KeychainFault::Missing);
            }
            let locked: bool = proxy(&self.conn, path.as_str(), COLLECTION)?
                .get_property("Locked")
                .map_err(fault)?;
            if locked {
                return Err(KeychainFault::Locked);
            }
            Ok(path)
        }

        /// MDM's items of `service` (one account's, or all): (unlocked, locked).
        fn find(
            &self,
            service: &str,
            account: Option<&str>,
        ) -> Result<(Vec<OwnedObjectPath>, Vec<OwnedObjectPath>), KeychainFault> {
            let mut attrs = HashMap::from([("service", service)]);
            if let Some(account) = account {
                attrs.insert("account", account);
            }
            proxy(&self.conn, ROOT, SERVICE)?
                .call("SearchItems", &(attrs,))
                .map_err(fault)
        }
    }

    impl Drop for Session {
        fn drop(&mut self) {
            if let Ok(p) = proxy(&self.conn, self.path.as_str(), SESSION) {
                let _ = p.call::<_, _, ()>("Close", &());
            }
        }
    }

    pub fn probe(_service: &str) -> Result<(), KeychainFault> {
        Session::open()?.collection().map(|_| ())
    }

    pub fn read(service: &str, account: &str) -> Result<Option<String>, KeychainFault> {
        let s = Session::open()?;
        let (unlocked, locked) = s.find(service, Some(account))?;
        let Some(item) = unlocked.first() else {
            return if locked.is_empty() {
                Ok(None)
            } else {
                Err(KeychainFault::Locked)
            };
        };
        let (_, _, value, _): (OwnedObjectPath, Vec<u8>, Vec<u8>, String) =
            proxy(&s.conn, item.as_str(), ITEM)?
                .call("GetSecret", &(s.path(),))
                .map_err(fault)?;
        String::from_utf8(value)
            .map(Some)
            .map_err(|_| KeychainFault::Failed("not UTF-8".into()))
    }

    pub fn write(
        service: &str,
        account: &str,
        label: &str,
        secret: &str,
    ) -> Result<(), KeychainFault> {
        let s = Session::open()?;
        let collection = s.collection()?;
        let attrs = HashMap::from([
            ("service", service),
            ("account", account),
            ("xdg:schema", SCHEMA),
        ]);
        let props = HashMap::from([
            ("org.freedesktop.Secret.Item.Label", Value::from(label)),
            ("org.freedesktop.Secret.Item.Attributes", Value::from(attrs)),
        ]);
        let value = (
            s.path(),
            Vec::<u8>::new(),
            secret.as_bytes(),
            "text/plain; charset=utf8",
        );
        let (_, prompt): (OwnedObjectPath, OwnedObjectPath) =
            proxy(&s.conn, collection.as_str(), COLLECTION)?
                .call("CreateItem", &(props, value, true))
                .map_err(fault)?;
        // It wants to ask the user (to unlock): never done from here.
        if prompt.as_str() != "/" {
            return Err(KeychainFault::Locked);
        }
        Ok(())
    }

    pub fn delete(service: &str, account: &str) -> Result<(), KeychainFault> {
        let s = Session::open()?;
        let (unlocked, locked) = s.find(service, Some(account))?;
        if unlocked.is_empty() && !locked.is_empty() {
            return Err(KeychainFault::Locked);
        }
        for item in unlocked {
            let prompt: OwnedObjectPath = proxy(&s.conn, item.as_str(), ITEM)?
                .call("Delete", &())
                .map_err(fault)?;
            if prompt.as_str() != "/" {
                return Err(KeychainFault::Locked);
            }
        }
        Ok(())
    }

    /// The accounts of `service`'s unlocked items.
    pub fn list(service: &str) -> Result<Vec<String>, KeychainFault> {
        let s = Session::open()?;
        let (unlocked, _) = s.find(service, None)?;
        let mut accounts = Vec::new();
        for item in unlocked {
            let attrs: OwnedValue = proxy(&s.conn, item.as_str(), ITEM)?
                .get_property("Attributes")
                .map_err(fault)?;
            let attrs = HashMap::<String, String>::try_from(attrs)
                .map_err(|_| KeychainFault::Failed("attributes".into()))?;
            if attrs.get("service").is_some_and(|s| s == service)
                && let Some(account) = attrs.get("account")
            {
                accounts.push(account.clone());
            }
        }
        Ok(accounts)
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use security_framework::item::{ItemClass, ItemSearchOptions, Limit};
    use security_framework::passwords::{
        PasswordOptions, delete_generic_password, generic_password, set_generic_password_options,
    };

    use super::KeychainFault;

    const ITEM_NOT_FOUND: i32 = -25300;
    const INTERACTION_NOT_ALLOWED: i32 = -25308;
    const NO_SUCH_KEYCHAIN: i32 = -25294;
    const NOT_AVAILABLE: i32 = -25291;

    fn fault(e: security_framework::base::Error) -> KeychainFault {
        match e.code() {
            INTERACTION_NOT_ALLOWED => KeychainFault::Locked,
            NO_SUCH_KEYCHAIN | NOT_AVAILABLE => KeychainFault::Missing,
            code => KeychainFault::Failed(format!("Security error {code}")),
        }
    }

    pub fn probe(service: &str) -> Result<(), KeychainFault> {
        list(service).map(|_| ())
    }

    pub fn read(service: &str, account: &str) -> Result<Option<String>, KeychainFault> {
        match generic_password(PasswordOptions::new_generic_password(service, account)) {
            Ok(bytes) => String::from_utf8(bytes)
                .map(Some)
                .map_err(|_| KeychainFault::Failed("not UTF-8".into())),
            Err(e) if e.code() == ITEM_NOT_FOUND => Ok(None),
            Err(e) => Err(fault(e)),
        }
    }

    pub fn write(
        service: &str,
        account: &str,
        label: &str,
        secret: &str,
    ) -> Result<(), KeychainFault> {
        let mut options = PasswordOptions::new_generic_password(service, account);
        options.set_label(label);
        set_generic_password_options(secret.as_bytes(), options).map_err(fault)
    }

    pub fn delete(service: &str, account: &str) -> Result<(), KeychainFault> {
        match delete_generic_password(service, account) {
            Err(e) if e.code() != ITEM_NOT_FOUND => Err(fault(e)),
            _ => Ok(()),
        }
    }

    /// The accounts of `service`'s generic passwords (their attributes only, not their data).
    pub fn list(service: &str) -> Result<Vec<String>, KeychainFault> {
        let found = ItemSearchOptions::new()
            .class(ItemClass::generic_password())
            .service(service)
            .load_attributes(true)
            .limit(Limit::All)
            .search();
        match found {
            Ok(items) => Ok(items
                .iter()
                .filter_map(|i| i.simplify_dict()?.get("acct").cloned())
                .collect()),
            Err(e) if e.code() == ITEM_NOT_FOUND => Ok(Vec::new()),
            Err(e) => Err(fault(e)),
        }
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod imp {
    use super::KeychainFault;

    pub fn probe(_service: &str) -> Result<(), KeychainFault> {
        Err(KeychainFault::Missing)
    }

    pub fn read(_service: &str, _account: &str) -> Result<Option<String>, KeychainFault> {
        Ok(None)
    }

    pub fn write(
        _service: &str,
        _account: &str,
        _label: &str,
        _secret: &str,
    ) -> Result<(), KeychainFault> {
        Err(KeychainFault::Missing)
    }

    pub fn delete(_service: &str, _account: &str) -> Result<(), KeychainFault> {
        Ok(())
    }

    pub fn list(_service: &str) -> Result<Vec<String>, KeychainFault> {
        Ok(Vec::new())
    }
}
