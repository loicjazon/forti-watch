//! État du VPN FortiClient, lu avec l'outil propre à chaque système.

pub mod forticlient;
pub mod scutil;

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

/// Valeur de la ligne `<name>: <valeur>` ou `<name> : <valeur>` (les lignes de détail sont indentées).
fn field<'a>(output: &'a str, name: &str) -> Option<&'a str> {
    output.lines().find_map(|line| {
        line.trim()
            .strip_prefix(name)?
            .trim_start()
            .strip_prefix(':')
            .map(str::trim)
    })
}
