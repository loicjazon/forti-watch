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

/// Dépôt GitHub des releases (surchargeable par `FORTI_WATCH_UPDATE_URL`, utile pour tester).
pub const UPDATE_BASE_URL: &str = "https://github.com/loicjazon/forti-watch";
/// Binaire publié par la release, avec son `.sha256`.
#[cfg(target_os = "linux")]
pub const ASSET_NAME: &str = "forti-watch-linux-x86_64";
/// Binaire universel (Intel et Apple Silicon).
#[cfg(target_os = "macos")]
pub const ASSET_NAME: &str = "forti-watch-macos-universal";
/// Calcul d'empreinte SHA-256 (`-c` pour vérifier) : `sha256sum` n'existe sur macOS qu'à partir de la 14, `shasum`
/// y est toujours présent.
#[cfg(target_os = "linux")]
pub const CHECKSUM_COMMAND: [&str; 1] = ["sha256sum"];
#[cfg(target_os = "macos")]
pub const CHECKSUM_COMMAND: [&str; 3] = ["shasum", "-a", "256"];
/// Première recherche de mise à jour, peu après le démarrage ; les suivantes toutes les 6 h.
pub const FIRST_UPDATE_CHECK_DELAY: Duration = Duration::from_secs(30);
pub const UPDATE_CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
