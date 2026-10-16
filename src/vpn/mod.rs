//! État du VPN FortiClient, lu avec l'outil propre à chaque système.

pub mod forticlient;
pub mod scutil;

use std::time::Duration;
#[cfg(target_os = "linux")]
use std::time::SystemTime;

/// État du VPN tel que rapporté par FortiClient.
#[derive(Debug, PartialEq, Eq)]
pub enum Status {
    /// Connecté depuis `elapsed` (la « Duration » affichée par FortiClient).
    Connected {
        elapsed: Duration,
    },
    Disconnected,
}

/// Lit l'état du VPN avec `cli_path` (`forticlient-cli` sous Linux, `scutil` sous macOS).
/// `Err` : état inconnu, à ne pas confondre avec une déconnexion.
#[cfg(target_os = "linux")]
pub fn read_status(cli_path: &str, _now: SystemTime) -> Result<Status, String> {
    forticlient::read_status(cli_path)
}

#[cfg(target_os = "macos")]
pub use scutil::read_status;

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
