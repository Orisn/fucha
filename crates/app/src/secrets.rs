//! Sign-in secrets in the system's keychain (#44): the debrid accounts' credentials, Sites
//! Logins' passwords and the proxy's password. The vault's logic and its worker now live in
//! `mdm_core::secrets` (where the settings use them, which the Fucha backend shares), and the
//! system's own keychain in `mdm_platform::keychain` (Windows Credential Manager, the Secret
//! Service on Linux, the macOS keychain).
//!
//! This module is the app's door to them: [`install`] once at start, `--forget-sign-ins` for the
//! Windows uninstaller ([`forget_all`]), and what Options → Accounts shows ([`vault`],
//! [`Kept`]).

pub use mdm_core::secrets::{Kept, forget_all, install, vault};
