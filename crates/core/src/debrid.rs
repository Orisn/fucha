//! Debrid service accounts (#44): where the services answer, the account handle a download
//! goes through, and the texts the app shows for an account.

use std::sync::Arc;

use mdm_debrid::{AccountInfo, Fault, Http, ServiceKind, Services};

/// Where the services answer. Debug builds only: MDM_DEBRID_LAB=http://127.0.0.1:PORT/ points
/// at the lab server's fakes (labserver `debrid`), for tests by hand.
pub fn services() -> Services {
    #[cfg(debug_assertions)]
    if let Some(base) = std::env::var("MDM_DEBRID_LAB")
        .ok()
        .and_then(|b| url::Url::parse(&b).ok())
    {
        return Services::at(&base);
    }
    Services::default()
}

/// A client for the services, through MDM's own network settings (proxy, DNS, adapter).
pub fn http() -> Result<Arc<Http>, String> {
    Http::new(&crate::settings::own_net(), services()).map(Arc::new)
}

/// Did the browser start this download itself (it came with the browser's size or type,
/// `ipc::Command`)? Then the host is already sending the file: by default it is downloaded
/// directly, not through a debrid account (`Item::direct`).
pub fn browser_started(context: &mdm_ipc::Command) -> bool {
    context.size.is_some() || context.mime.is_some()
}

/// Which account a download goes through (#44): the accounts' handle (`Core::debrid`), Options'
/// "Download supported file-host links through these accounts", and each account's name for the
/// windows that say it. Cheap to clone: Download File Info and File Properties keep one.
#[derive(Clone, Default)]
pub struct Route {
    debrid: mdm_debrid::Debrid,
    on: bool,
    /// Each account's id and its name at the service.
    names: Arc<Vec<(u64, String)>>,
}

impl Route {
    pub fn new(debrid: &mdm_debrid::Debrid, settings: &crate::settings::Settings) -> Self {
        Route {
            debrid: debrid.clone(),
            on: settings.debrid_on,
            names: Arc::new(
                settings
                    .debrid_accounts
                    .iter()
                    .map(|a| (a.id, a.user.clone()))
                    .collect(),
            ),
        }
    }

    /// The account whose service takes `url`'s host: None when turned off in Options, or no
    /// account takes it (or its host lists aren't read yet).
    pub fn account(&self, url: &url::Url) -> Option<(u64, ServiceKind)> {
        if !self.on {
            return None;
        }
        self.debrid.account_for(url)
    }

    /// The unlocker a download of `url` goes with (`mdm_engine::Options::unlocker`): none when
    /// it is downloaded directly (`direct`: the user's choice, or the browser had started it).
    pub fn unlocker(&self, url: &url::Url, direct: bool) -> Option<Arc<dyn mdm_engine::Unlocker>> {
        if direct || self.account(url).is_none() {
            return None;
        }
        self.debrid.unlocker_for(url)
    }

    /// The service that takes `url`, and the account's name: "Real-Debrid (labuser)".
    pub fn through(&self, url: &url::Url) -> Option<String> {
        let (id, service) = self.account(url)?;
        let user = self
            .names
            .iter()
            .find(|(a, _)| *a == id)
            .map(|(_, u)| u.as_str())
            .filter(|u| !u.is_empty());
        Some(match user {
            Some(u) => format!("{} ({})", service.name(), crate::i18n::ltr(u)),
            None => service.name().to_owned(),
        })
    }

    /// Just the service's name ("Real-Debrid"), for the narrow places.
    pub fn service(&self, url: &url::Url) -> Option<&'static str> {
        self.account(url).map(|(_, s)| s.name())
    }
}

/// "premium, 143 days left", "free account": what Options shows for an account.
pub fn state_text(info: &AccountInfo) -> String {
    match (info.premium, info.premium_left) {
        (true, Some(left)) => crate::i18n::fmt(
            // i18n: Options → Accounts: a debrid account's state; {days} the days of premium left
            crate::i18n::t("premium, {days} days left"),
            &[("days", &(left.as_secs() / 86400).to_string())],
        ),
        // i18n: Options → Accounts: a debrid account with premium (no end date given)
        (true, None) => crate::i18n::t("premium").to_owned(),
        // i18n: Options → Accounts: a debrid account without premium (it can't unlock links)
        (false, _) => crate::i18n::t("free account: links need premium").to_owned(),
    }
}

/// Why a sign-in or a check failed, in the interface language.
pub fn fault_text(service: ServiceKind, f: &Fault) -> String {
    match f {
        Fault::Service(f) => crate::errors::service(service.name(), f),
        // i18n: Options → Accounts: the service doesn't take this file host (shouldn't happen when signing in)
        Fault::Unsupported => crate::i18n::t("not supported by this service").to_owned(),
    }
}
