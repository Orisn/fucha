//! What a download learnt of its server (#140), kept with the download: its address, the round
//! trip the download's own connections measured, whether a proxy carried it, and the CDN that
//! answered. The app draws it (crates/app/src/whereis.rs); this is the part the list and the
//! Properties window keep.

use std::net::IpAddr;

use serde::{Deserialize, Serialize};

/// What a download learnt of its server, kept with it (Properties shows it after it ends).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seen {
    #[serde(default)]
    pub ip: Option<IpAddr>,
    #[serde(default)]
    pub rtt_ms: Option<u32>,
    #[serde(default)]
    pub proxied: bool,
    #[serde(default)]
    pub cdn: Option<String>,
}

impl From<&mdm_engine::Server> for Seen {
    fn from(s: &mdm_engine::Server) -> Self {
        Seen {
            ip: s.ip,
            rtt_ms: s.rtt.map(|d| d.as_millis().min(u32::MAX as u128) as u32),
            proxied: s.proxied,
            cdn: s.cdn.map(str::to_owned),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_was_seen_is_kept_in_milliseconds() {
        let s = mdm_engine::Server {
            ip: Some("8.8.8.8".parse().unwrap()),
            rtt: Some(std::time::Duration::from_micros(23_400)),
            proxied: false,
            cdn: Some("Cloudflare"),
        };
        let seen = Seen::from(&s);
        assert_eq!(seen.rtt_ms, Some(23));
        assert_eq!(seen.cdn.as_deref(), Some("Cloudflare"));
        let back: Seen = serde_json::from_str(&serde_json::to_string(&seen).unwrap()).unwrap();
        assert_eq!(back, seen);
        // Kept from before: nothing.
        let old: Seen = serde_json::from_str("{}").unwrap();
        assert_eq!(old, Seen::default());
    }
}
