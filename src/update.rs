use crate::config::{
    ASSET_NAME, FIRST_UPDATE_CHECK_DELAY, UPDATE_BASE_URL, UPDATE_CHECK_INTERVAL,
    WARNING_THRESHOLDS_MINUTES,
};
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::Instant;

const TAG_MARKER: &str = "/releases/tag/";
const DOWNLOAD_TIMEOUT_SECONDS: &str = "120";

/// Numéro de version `majeur.mineur.correctif` ; l'ordre dérivé compare chaque nombre (1.0.10 > 1.0.9).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(u64, u64, u64);

impl Version {
    /// `1.2.3` ou `v1.2.3` ; `None` si le texte n'a pas exactement trois nombres.
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.strip_prefix('v').unwrap_or(text).split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        parts.next().is_none().then_some(Self(major, minor, patch))
    }

    fn tag(&self) -> String {
        format!("v{}.{}.{}", self.0, self.1, self.2)
    }
}

/// Version de la dernière release, lue dans l'URL finale `…/releases/tag/vX.Y.Z` de `…/releases/latest`.
pub fn parse_latest_tag(url: &str) -> Option<Version> {
    let (_, tag) = url.trim().trim_end_matches('/').rsplit_once(TAG_MARKER)?;
    Version::parse(tag)
}

/// La mise à jour ne s'applique que si la version distante est strictement plus récente.
pub fn is_newer(latest: &Version, current: &Version) -> bool {
    latest > current
}

/// Relancer coupe l'application : le `Tracker` oublie les seuils déjà notifiés et rejouerait une alerte.
/// On relance donc hors session, ou tant qu'il reste plus que le premier seuil d'alerte.
pub fn can_restart(remaining_minutes: Option<u64>) -> bool {
    let first_threshold = WARNING_THRESHOLDS_MINUTES.iter().copied().max();
    match (remaining_minutes, first_threshold) {
        (Some(remaining), Some(threshold)) => remaining > threshold,
        _ => true,
    }
}

/// Recherche une release plus récente que `current` ; si elle existe, remplace le binaire `exe_path`.
/// Renvoie la nouvelle version installée, `None` si on est à jour. En cas d'échec, le binaire en place
/// n'est pas touché.
pub fn check_and_install(
    base_url: &str,
    exe_path: &Path,
    current: &Version,
) -> Result<Option<Version>, String> {
    let latest = find_latest(base_url)?;
    if !is_newer(&latest, current) {
        return Ok(None);
    }
    install(base_url, &latest, exe_path)?;
    Ok(Some(latest))
}

/// Version de la dernière release, d'après la redirection de `…/releases/latest`.
fn find_latest(base_url: &str) -> Result<Version, String> {
    let latest_url = run_curl(&[
        "-o",
        "/dev/null",
        "-w",
        "%{url_effective}",
        &format!("{base_url}/releases/latest"),
    ])?;
    parse_latest_tag(&latest_url)
        .ok_or_else(|| format!("version de la dernière release illisible ({latest_url})"))
}

/// Télécharge le binaire de `latest` et son `.sha256`, vérifie l'empreinte puis remplace `exe_path`.
/// En cas d'échec, `exe_path` n'est pas touché.
fn install(base_url: &str, latest: &Version, exe_path: &Path) -> Result<(), String> {
    // Même dossier que l'exécutable : le renommage final reste sur un seul système de fichiers (atomique).
    let work_dir = TempDir::create(
        exe_path
            .parent()
            .ok_or("dossier de l'exécutable introuvable")?,
    )?;
    let downloaded = work_dir.path().join(ASSET_NAME);
    let checksum = work_dir.path().join(format!("{ASSET_NAME}.sha256"));
    for (target, name) in [
        (&downloaded, ASSET_NAME),
        (&checksum, &format!("{ASSET_NAME}.sha256")),
    ] {
        let url = format!("{base_url}/releases/download/{}/{name}", latest.tag());
        let target = target.to_str().ok_or("chemin temporaire illisible")?;
        run_curl(&["--max-time", DOWNLOAD_TIMEOUT_SECONDS, "-o", target, &url])?;
    }

    verify_checksum(work_dir.path())?;
    fs::set_permissions(&downloaded, fs::Permissions::from_mode(0o755))
        .map_err(|error| format!("permissions du binaire téléchargé : {error}"))?;
    fs::rename(&downloaded, exe_path)
        .map_err(|error| format!("remplacement de {} : {error}", exe_path.display()))
}

type CheckResult = Result<Option<Version>, String>;

/// Cherche les mises à jour en arrière-plan (un thread, pour ne jamais figer la boucle GTK) et relance
/// l'application une fois le nouveau binaire en place.
pub struct Updater {
    base_url: String,
    exe_path: PathBuf,
    current: Version,
    next_check: Instant,
    running: Option<Receiver<CheckResult>>,
    /// Un nouveau binaire est installé : reste à relancer, dès que `can_restart` l'autorise.
    installed: bool,
    last_error: Option<String>,
}

impl Updater {
    /// `None` si la mise à jour automatique est désactivée : `FORTI_WATCH_NO_UPDATE`, mode test, ou build
    /// de développement sans `FORTI_WATCH_UPDATE_URL`.
    pub fn from_env(now: Instant) -> Option<Self> {
        let custom_url = env::var("FORTI_WATCH_UPDATE_URL").ok();
        let disabled = env::var_os("FORTI_WATCH_NO_UPDATE").is_some()
            || env::var_os("FORTI_WATCH_TEST_EVERY_MINUTE").is_some()
            || (cfg!(debug_assertions) && custom_url.is_none());
        if disabled {
            return None;
        }

        // Chemin lu maintenant : une fois le binaire remplacé, `current_exe()` renverrait « … (deleted) ».
        Some(Self {
            base_url: custom_url.unwrap_or_else(|| String::from(UPDATE_BASE_URL)),
            exe_path: env::current_exe().ok()?,
            current: Version::parse(env!("CARGO_PKG_VERSION"))?,
            next_check: now + FIRST_UPDATE_CHECK_DELAY,
            running: None,
            installed: false,
            last_error: None,
        })
    }

    /// À appeler à chaque vérification : lit le résultat d'une recherche terminée, en lance une si
    /// l'échéance est passée, relance l'application si un binaire attend et que c'est sans risque.
    pub fn poll(&mut self, now: Instant, remaining_minutes: Option<u64>) {
        if let Some(receiver) = &self.running {
            match receiver.try_recv() {
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.running = None,
                Ok(result) => {
                    self.running = None;
                    self.handle(result);
                }
            }
        }

        if self.running.is_none() && !self.installed && now >= self.next_check {
            self.next_check = now + UPDATE_CHECK_INTERVAL;
            self.running = Some(self.spawn_check());
        }

        if self.installed && can_restart(remaining_minutes) {
            self.relaunch();
        }
    }

    fn spawn_check(&self) -> Receiver<CheckResult> {
        let (sender, receiver) = mpsc::channel();
        let (base_url, exe_path, current) = (
            self.base_url.clone(),
            self.exe_path.clone(),
            self.current.clone(),
        );
        thread::spawn(move || {
            // Si la boucle principale a disparu, personne n'écoute plus : l'envoi échoue sans conséquence.
            let _ = sender.send(check_and_install(&base_url, &exe_path, &current));
        });
        receiver
    }

    fn handle(&mut self, result: CheckResult) {
        match result {
            Ok(Some(version)) => {
                println!("Mise à jour installée : {}", version.tag());
                self.last_error = None;
                self.installed = true;
            }
            Ok(None) => self.last_error = None,
            Err(error) => {
                // Même règle que pour la CLI : on ne répète pas la même erreur à chaque tentative.
                if self.last_error.as_deref() != Some(error.as_str()) {
                    eprintln!("Mise à jour impossible : {error}");
                    self.last_error = Some(error);
                }
            }
        }
    }

    fn relaunch(&mut self) {
        // `exec` ne revient que s'il échoue ; sinon le nouveau binaire a remplacé ce processus.
        let error = Command::new(&self.exe_path)
            .args(env::args_os().skip(1))
            .exec();
        eprintln!("Relance impossible : {error}");
        self.installed = false;
    }
}

fn run_curl(args: &[&str]) -> Result<String, String> {
    let output = Command::new("curl")
        .args(["-fsSL"])
        .args(args)
        .output()
        .map_err(|error| format!("curl introuvable : {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "curl a échoué ({}) : {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `sha256sum -c` dans `dir`, où le fichier `.sha256` désigne le binaire par son nom.
fn verify_checksum(dir: &Path) -> Result<(), String> {
    let status = Command::new("sha256sum")
        .args(["-c", &format!("{ASSET_NAME}.sha256")])
        .current_dir(dir)
        .output()
        .map_err(|error| format!("sha256sum introuvable : {error}"))?
        .status;
    if status.success() {
        Ok(())
    } else {
        Err(String::from(
            "l'empreinte SHA-256 du binaire téléchargé ne correspond pas",
        ))
    }
}

/// Dossier de travail supprimé à la fin, succès ou échec.
struct TempDir(PathBuf);

impl TempDir {
    fn create(parent: &Path) -> Result<Self, String> {
        let path = parent.join(format!(".forti-watch-update-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir(&path)
            .map_err(|error| format!("création de {} : {error}", path.display()))?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDir;
    use std::time::Duration;

    const OLD_BINARY: &str = "ancien binaire";
    const NEW_BINARY: &str = "nouveau binaire";

    fn version(text: &str) -> Version {
        Version::parse(text).expect("version valide")
    }

    /// Fausse release `v9.9.9` servie en `file://` (que `curl` sait lire) et binaire installé à remplacer.
    struct FakeRelease {
        dir: TestDir,
        exe_path: PathBuf,
    }

    impl FakeRelease {
        fn new(name: &str) -> Self {
            let dir = TestDir::new(name);
            let exe_path = dir.write("bin/forti-watch", OLD_BINARY);
            let asset = dir.write(
                &format!("releases/download/v9.9.9/{ASSET_NAME}"),
                NEW_BINARY,
            );
            // Empreinte au format de la release : `<sha256>  <nom du fichier>`.
            let checksum = Command::new("sha256sum")
                .arg(ASSET_NAME)
                .current_dir(asset.parent().expect("dossier de la release"))
                .output()
                .expect("sha256sum");
            dir.write(
                &format!("releases/download/v9.9.9/{ASSET_NAME}.sha256"),
                &String::from_utf8_lossy(&checksum.stdout),
            );
            Self { dir, exe_path }
        }

        fn install(&self) -> Result<(), String> {
            let base_url = format!("file://{}", self.dir.path().display());
            install(&base_url, &version("9.9.9"), &self.exe_path)
        }

        fn installed_binary(&self) -> String {
            fs::read_to_string(&self.exe_path).expect("binaire installé")
        }

        fn files_next_to_the_binary(&self) -> Vec<String> {
            fs::read_dir(self.exe_path.parent().expect("dossier du binaire"))
                .expect("lecture du dossier du binaire")
                .map(|entry| {
                    entry
                        .expect("entrée")
                        .file_name()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect()
        }
    }

    #[test]
    fn install_replaces_the_binary_when_the_checksum_matches() {
        // Arrange
        let release = FakeRelease::new("install-ok");

        // Act
        let result = release.install();

        // Assert
        assert_eq!(result, Ok(()));
        assert_eq!(release.installed_binary(), NEW_BINARY);
    }

    #[test]
    fn install_makes_the_new_binary_executable() {
        // Arrange
        let release = FakeRelease::new("install-mode");

        // Act
        release.install().expect("installation");

        // Assert
        let mode = fs::metadata(&release.exe_path)
            .expect("binaire installé")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755);
    }

    #[test]
    fn install_keeps_the_binary_when_the_checksum_does_not_match() {
        // Arrange
        let release = FakeRelease::new("install-bad-checksum");
        release.dir.write(
            &format!("releases/download/v9.9.9/{ASSET_NAME}"),
            "binaire corrompu",
        );

        // Act
        let result = release.install();

        // Assert
        assert!(result.is_err());
        assert_eq!(release.installed_binary(), OLD_BINARY);
    }

    #[test]
    fn install_keeps_the_binary_when_the_download_fails() {
        // Arrange
        let release = FakeRelease::new("install-missing-asset");
        fs::remove_file(
            release
                .dir
                .path()
                .join(format!("releases/download/v9.9.9/{ASSET_NAME}")),
        )
        .expect("suppression du binaire publié");

        // Act
        let result = release.install();

        // Assert
        assert!(result.is_err());
        assert_eq!(release.installed_binary(), OLD_BINARY);
    }

    #[test]
    fn install_removes_its_work_directory_after_a_success() {
        // Arrange
        let release = FakeRelease::new("install-cleanup-ok");

        // Act
        release.install().expect("installation");

        // Assert
        assert_eq!(release.files_next_to_the_binary(), ["forti-watch"]);
    }

    #[test]
    fn install_removes_its_work_directory_after_a_failure() {
        // Arrange
        let release = FakeRelease::new("install-cleanup-error");
        release.dir.write(
            &format!("releases/download/v9.9.9/{ASSET_NAME}.sha256"),
            "0000  forti-watch-linux-x86_64\n",
        );

        // Act
        let result = release.install();

        // Assert
        assert!(result.is_err());
        assert_eq!(release.files_next_to_the_binary(), ["forti-watch"]);
    }

    #[test]
    fn parse_accepts_a_version_with_or_without_the_v_prefix() {
        // Arrange / Act
        let with_prefix = Version::parse("v1.2.3");
        let without_prefix = Version::parse("1.2.3");

        // Assert
        assert_eq!(with_prefix, Some(Version(1, 2, 3)));
        assert_eq!(without_prefix, Some(Version(1, 2, 3)));
    }

    #[test]
    fn parse_rejects_text_that_is_not_three_numbers() {
        for text in ["", "1.2", "1.2.3.4", "1.2.x", "v", "latest"] {
            assert_eq!(Version::parse(text), None, "{text:?}");
        }
    }

    #[test]
    fn parse_latest_tag_reads_the_tag_of_the_final_url() {
        // Arrange
        let url = "https://github.com/loicjazon/forti-watch/releases/tag/v1.4.0\n";

        // Act
        let latest = parse_latest_tag(url);

        // Assert
        assert_eq!(latest, Some(Version(1, 4, 0)));
    }

    #[test]
    fn parse_latest_tag_ignores_a_url_without_release() {
        // Arrange : sans release publiée, GitHub redirige vers la liste des releases.
        let url = "https://github.com/loicjazon/forti-watch/releases";

        // Act / Assert
        assert_eq!(parse_latest_tag(url), None);
        assert_eq!(
            parse_latest_tag("https://github.com/x/y/releases/tag/nightly"),
            None
        );
    }

    #[test]
    fn is_newer_compares_each_number_not_the_text() {
        assert!(is_newer(&version("1.0.10"), &version("1.0.9")));
        assert!(is_newer(&version("1.1.0"), &version("1.0.9")));
        assert!(is_newer(&version("2.0.0"), &version("1.9.9")));
    }

    #[test]
    fn is_newer_refuses_the_same_or_an_older_version() {
        assert!(!is_newer(&version("1.0.2"), &version("1.0.2")));
        assert!(!is_newer(&version("1.0.1"), &version("1.0.2")));
    }

    #[test]
    fn can_restart_when_the_vpn_is_not_connected() {
        assert!(can_restart(None));
    }

    #[test]
    fn can_restart_only_while_more_than_the_first_threshold_remains() {
        assert!(can_restart(Some(61)));
        assert!(!can_restart(Some(60)));
        assert!(!can_restart(Some(10)));
    }

    /// `Updater` sans danger : la recherche vise une URL qui n'existe pas et la relance un exécutable absent
    /// (un `exec` réussi remplacerait le processus de test). Prochaine recherche dans une heure.
    fn safe_updater(now: Instant) -> Updater {
        Updater {
            base_url: String::from("file:///forti-watch-test/introuvable"),
            exe_path: PathBuf::from("/forti-watch-test/introuvable/forti-watch"),
            current: version("1.0.0"),
            next_check: now + Duration::from_secs(3600),
            running: None,
            installed: false,
            last_error: None,
        }
    }

    /// Recherche déjà terminée, dont le résultat attend d'être lu par `poll`.
    fn finished_search(result: CheckResult) -> Receiver<CheckResult> {
        let (sender, receiver) = mpsc::channel();
        sender.send(result).expect("envoi du résultat");
        receiver
    }

    #[test]
    fn poll_does_not_search_before_the_check_is_due() {
        // Arrange
        let now = Instant::now();
        let mut updater = safe_updater(now);

        // Act
        updater.poll(now, None);

        // Assert
        assert!(updater.running.is_none());
    }

    #[test]
    fn poll_starts_a_search_when_the_check_is_due() {
        // Arrange
        let now = Instant::now();
        let mut updater = safe_updater(now);
        updater.next_check = now;

        // Act
        updater.poll(now, None);

        // Assert
        assert!(updater.running.is_some());
        assert_eq!(updater.next_check, now + UPDATE_CHECK_INTERVAL);
    }

    #[test]
    fn poll_does_not_search_again_while_a_new_binary_waits() {
        // Arrange
        let now = Instant::now();
        let mut updater = safe_updater(now);
        updater.next_check = now;
        updater.installed = true;

        // Act : 10 min restantes, la relance est différée.
        updater.poll(now, Some(10));

        // Assert
        assert!(updater.running.is_none());
    }

    #[test]
    fn poll_defers_the_restart_while_one_hour_or_less_remains() {
        // Arrange
        let now = Instant::now();
        let mut updater = safe_updater(now);
        updater.installed = true;

        // Act
        updater.poll(now, Some(60));

        // Assert : aucune relance tentée, le binaire attend toujours.
        assert!(updater.installed);
    }

    #[test]
    fn poll_restarts_when_more_than_one_hour_remains() {
        // Arrange
        let now = Instant::now();
        let mut updater = safe_updater(now);
        updater.installed = true;

        // Act
        updater.poll(now, Some(61));

        // Assert : la relance a été tentée (et a échoué, l'exécutable n'existant pas).
        assert!(!updater.installed);
    }

    #[test]
    fn poll_reads_the_result_of_a_finished_search() {
        // Arrange
        let now = Instant::now();
        let mut updater = safe_updater(now);
        updater.running = Some(finished_search(Ok(Some(version("1.1.0")))));

        // Act : 10 min restantes, la relance est différée.
        updater.poll(now, Some(10));

        // Assert
        assert!(updater.running.is_none());
        assert!(updater.installed);
    }

    #[test]
    fn poll_forgets_a_search_whose_thread_has_stopped() {
        // Arrange
        let now = Instant::now();
        let mut updater = safe_updater(now);
        let (_, receiver) = mpsc::channel();
        updater.running = Some(receiver);

        // Act
        updater.poll(now, None);

        // Assert
        assert!(updater.running.is_none());
    }

    #[test]
    fn handle_remembers_the_error_to_show_it_only_once() {
        // Arrange
        let mut updater = safe_updater(Instant::now());

        // Act
        updater.handle(Err(String::from("réseau coupé")));

        // Assert
        assert_eq!(updater.last_error.as_deref(), Some("réseau coupé"));
        assert!(!updater.installed);
    }

    #[test]
    fn handle_forgets_the_error_once_a_search_succeeds() {
        // Arrange
        let mut updater = safe_updater(Instant::now());
        updater.handle(Err(String::from("réseau coupé")));

        // Act
        updater.handle(Ok(None));

        // Assert
        assert_eq!(updater.last_error, None);
    }
}
