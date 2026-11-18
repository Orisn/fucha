//! The keychain the settings' sign-in secrets live in, as the app provides it (#44).
//!
//! The settings keep a reference in a secret's place, `keychain:<service>/<scope>/<key>`, and
//! the secret itself lives in the system's keychain: Windows Credential Manager, the Secret
//! Service on Linux or the macOS keychain. That part is the app's (`crates/app/src/secrets.rs`,
//! with win32.rs and unix.rs behind it); this module is the interface the settings' logic uses
//! once that logic moves here, installed once at start ([`install`]).
//!
//! Without an installed vault nothing is asked of any keychain: [`is_prepared`] is false,
//! [`open`] and [`pass`] are None, and the caller keeps sealing its secrets as before.

use std::sync::{Arc, OnceLock};

/// What the settings hold in a secret's place.
pub const REF: &str = "keychain:";

/// The system's keychain, as the app implements it (`crates/app/src/secrets.rs`).
pub trait Vault: Send + Sync {
    /// Whether the settings are being made ready for the disk: their secret fields already hold
    /// what is written (a reference or a sealed secret) and go as they are.
    fn is_prepared(&self) -> bool;
    /// Runs `f` with the settings made ready for the disk, as [`prepared`] says.
    fn prepared(&self, f: &mut dyn FnMut());
    /// A reference read from the settings: its secret, or "" when it can't be read now (the
    /// reference is then kept for the next save).
    fn open(&self, stored: &str) -> String;
    /// A save's look at the secrets.
    fn pass(&self) -> Box<dyn Pass + '_>;
    /// The list was written, as [`after_write`] says.
    fn after_write(&self);
}

/// One save's secrets ([`Vault::pass`]).
pub trait Pass {
    /// What the settings write for the secret `key` (`debrid/3`): its reference once the
    /// keychain holds it, else None (the caller seals it as before) and the keychain is asked
    /// to take it. `label` names it in the system's own UI.
    fn secret(&mut self, key: &str, label: &dyn Fn() -> String, secret: &str) -> Option<String>;
    /// The places these settings use: kept; the others may go after the next write.
    fn finish(&mut self);
}

/// The keychain the app installed; none before [`install`].
static INSTALLED: OnceLock<Arc<dyn Vault>> = OnceLock::new();

/// Installs the keychain the settings use. The first call wins; later ones are ignored.
pub fn install(v: Arc<dyn Vault>) {
    let _ = INSTALLED.set(v);
}

/// Whether the settings are being made ready for the disk; false when no vault is installed.
pub fn is_prepared() -> bool {
    INSTALLED.get().is_some_and(|v| v.is_prepared())
}

/// Runs `f` with the settings made ready for the disk: their secret fields already hold what is
/// written (a reference or a sealed secret) and go as they are. Settings read inside it are
/// read as the disk has them: nothing is opened nor asked of the keychain. Without an installed
/// vault, just `f`.
pub fn prepared<R>(f: impl FnOnce() -> R) -> R {
    let Some(vault) = INSTALLED.get() else {
        return f();
    };
    let mut result = None;
    let mut f = Some(f);
    // One `FnMut` through the vault (a generic method wouldn't be object-safe).
    vault.prepared(&mut || result = Some(f.take().expect("prepared runs once")()));
    result.expect("the installed vault runs the prepared closure")
}

/// A reference's secret ([`Vault::open`]), or None when no vault is installed.
pub fn open(stored: &str) -> Option<String> {
    INSTALLED.get().map(|v| v.open(stored))
}

/// A save's pass over the secrets ([`Vault::pass`]), or None when no vault is installed.
pub fn pass() -> Option<Box<dyn Pass>> {
    INSTALLED.get().map(|v| v.pass())
}

/// The list was written: entries it no longer refers to may go. Nothing without an installed
/// vault.
pub fn after_write() {
    if let Some(vault) = INSTALLED.get() {
        vault.after_write();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestVault;

    impl Vault for TestVault {
        fn is_prepared(&self) -> bool {
            true
        }
        fn prepared(&self, f: &mut dyn FnMut()) {
            f();
        }
        fn open(&self, stored: &str) -> String {
            format!("opened:{stored}")
        }
        fn pass(&self) -> Box<dyn Pass + '_> {
            Box::new(TestPass)
        }
        fn after_write(&self) {}
    }

    struct TestPass;

    impl Pass for TestPass {
        fn secret(
            &mut self,
            key: &str,
            _label: &dyn Fn() -> String,
            secret: &str,
        ) -> Option<String> {
            Some(format!("{key}={secret}"))
        }
        fn finish(&mut self) {}
    }

    /// The module asks the installed vault for everything, one global for the process.
    #[test]
    fn the_installed_vault_answers_every_call() {
        install(Arc::new(TestVault));
        // A second install is ignored (the first call wins).
        install(Arc::new(TestVault));
        assert!(is_prepared());
        assert_eq!(
            open("keychain:a/b/c").as_deref(),
            Some("opened:keychain:a/b/c")
        );
        let mut pass = pass().unwrap();
        assert_eq!(
            pass.secret("debrid/1", &|| "label".into(), "s3cret")
                .as_deref(),
            Some("debrid/1=s3cret")
        );
        pass.finish();
        assert_eq!(prepared(|| 42), 42);
        after_write();
    }
}
