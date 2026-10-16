use std::time::Duration;

/// CLI de FortiClient (surchargeable par la variable d'environnement `FORTI_WATCH_CLI`).
pub const FORTICLIENT_CLI: &str = "/opt/forticlient/forticlient-cli";
/// Première vérification, juste après le démarrage de la boucle GTK.
pub const FIRST_CHECK_SECONDS: u32 = 1;
pub const CHECK_INTERVAL_SECONDS: u32 = 60;
pub const SESSION_DURATION: Duration = Duration::from_secs(8 * 60 * 60);
/// Alertes déclenchées quand il reste 1 h puis 15 min.
pub const WARNING_THRESHOLDS_MINUTES: [u64; 2] = [60, 15];

/// Dépôt GitHub des releases (surchargeable par `FORTI_WATCH_UPDATE_URL`, utile pour tester).
pub const UPDATE_BASE_URL: &str = "https://github.com/loicjazon/forti-watch";
/// Binaire publié par la release, avec son `.sha256`.
pub const ASSET_NAME: &str = "forti-watch-linux-x86_64";
/// Première recherche de mise à jour, peu après le démarrage ; les suivantes toutes les 6 h.
pub const FIRST_UPDATE_CHECK_DELAY: Duration = Duration::from_secs(30);
pub const UPDATE_CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
