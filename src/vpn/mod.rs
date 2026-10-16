//! État du VPN FortiClient, lu avec l'outil propre à chaque système.

pub mod forticlient;

use std::time::Duration;

/// État du VPN tel que rapporté par FortiClient.
#[derive(Debug, PartialEq, Eq)]
pub enum Status {
    /// Connecté depuis `elapsed` (la « Duration » affichée par FortiClient).
    Connected {
        elapsed: Duration,
    },
    Disconnected,
}

#[cfg(target_os = "linux")]
pub use forticlient::read_status;
