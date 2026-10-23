use std::time::Duration;

/// Commande qui donne l'état du VPN (surchargeable par la variable d'environnement `FORTI_WATCH_CLI`) : la CLI de
/// FortiClient sous Linux, l'outil système `scutil` sous macOS.
#[cfg(target_os = "linux")]
pub const FORTICLIENT_CLI: &str = "/opt/forticlient/forticlient-cli";
#[cfg(target_os = "macos")]
pub const FORTICLIENT_CLI: &str = "/usr/sbin/scutil";
/// Première vérification, juste après le démarrage de la boucle GTK.
pub const FIRST_CHECK_SECONDS: u32 = 1;
pub const CHECK_INTERVAL_SECONDS: u32 = 60;
pub const SESSION_DURATION: Duration = Duration::from_secs(8 * 60 * 60);
/// Alertes déclenchées quand il reste 1 h puis 15 min.
pub const WARNING_THRESHOLDS_MINUTES: [u64; 2] = [60, 15];
