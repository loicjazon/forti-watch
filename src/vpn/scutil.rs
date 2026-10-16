//! FortiClient sous macOS : le VPN est une Network Extension, lue avec l'outil système `scutil --nc`.
//!
//! La sortie de `scutil --nc status` contient le jeton de session VPN, l'identifiant de l'utilisateur, des IP et
//! des domaines internes : elle n'est **jamais** recopiée dans un message.

// L'analyse est compilée (et testée) sur tous les systèmes ; seul macOS l'utilise.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

#[cfg(target_os = "macos")]
use super::Status;
use super::field;
use std::time::{Duration, SystemTime};

/// Marque de la ligne du VPN FortiClient dans `scutil --nc list`.
pub const FORTICLIENT_SERVICE: &str = "[VPN:com.fortinet.forticlient.macos.vpn]";

/// Date et heure locales, sans fuseau (telles qu'affichées par `scutil`).
#[derive(Debug, PartialEq, Eq)]
pub struct LocalDateTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ScutilStatus {
    /// Connecté depuis `since` (heure locale du dernier changement d'état).
    Connected {
        since: LocalDateTime,
    },
    Disconnected,
}

/// Identifiant (UUID) du service VPN FortiClient dans la sortie de `scutil --nc list`.
pub fn parse_service_id(list_output: &str) -> Option<&str> {
    // Ligne : `* (Connected)      <UUID> VPN (com.fortinet…) "<nom>"  [VPN:com.fortinet…]`.
    let line = list_output
        .lines()
        .find(|line| line.trim_end().ends_with(FORTICLIENT_SERVICE))?;
    let after_state = line.split_once(')')?.1;
    after_state.split_whitespace().next()
}

/// Analyse la sortie de `scutil --nc status <id>`.
///
/// `None` si l'état est inconnu : état non reconnu, connecté sans heure lisible, ou reconnexion en cours
/// (`Reasserting`), pendant laquelle la session en cours doit être gardée.
pub fn parse_scutil(status_output: &str) -> Option<ScutilStatus> {
    let state = status_output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    match state {
        "Connected" => {
            let since = parse_date_time(field(status_output, "LastStatusChangeTime")?)?;
            Some(ScutilStatus::Connected { since })
        }
        "Disconnected" | "Connecting" | "Disconnecting" | "Invalid" => {
            Some(ScutilStatus::Disconnected)
        }
        // `Reasserting` (reconnexion réseau en cours) ou réponse inconnue : la session en cours est gardée.
        _ => None,
    }
}

/// Durée écoulée depuis `start` ; nulle si `start` est dans le futur (horloge recalée).
pub fn elapsed_since(start: SystemTime, now: SystemTime) -> Duration {
    now.duration_since(start).unwrap_or(Duration::ZERO)
}

/// Interroge `scutil --nc` : `Err` signifie que l'état est inconnu (l'appelant ne doit pas en déduire que le VPN est
/// coupé). Les messages d'erreur ne contiennent jamais la sortie de `scutil`.
#[cfg(target_os = "macos")]
pub fn read_status(scutil_path: &str, now: SystemTime) -> Result<Status, String> {
    let list_output = run(scutil_path, &["--nc", "list"])?;
    let service_id = parse_service_id(&list_output).ok_or(
        "VPN FortiClient introuvable dans les services réseau de macOS (scutil --nc list)",
    )?;
    let status_output = run(scutil_path, &["--nc", "status", service_id])?;
    match parse_scutil(&status_output) {
        Some(ScutilStatus::Connected { since }) => {
            let start =
                to_system_time(&since).ok_or("heure de connexion du VPN FortiClient illisible")?;
            Ok(Status::Connected {
                elapsed: elapsed_since(start, now),
            })
        }
        Some(ScutilStatus::Disconnected) => Ok(Status::Disconnected),
        None => Err(String::from(
            "état du VPN FortiClient inconnu (reconnexion en cours ou réponse inattendue de scutil)",
        )),
    }
}

/// Sortie standard de `scutil_path args…`, sans jamais recopier ce qu'il écrit.
#[cfg(target_os = "macos")]
fn run(scutil_path: &str, args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new(scutil_path)
        .args(args)
        .output()
        .map_err(|error| format!("impossible de lancer {scutil_path} : {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "« {scutil_path} {} » a échoué ({})",
            args.first().copied().unwrap_or_default(),
            output.status
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Heure locale -> instant, avec le fuseau et l'heure d'été du système (`mktime`).
#[cfg(target_os = "macos")]
fn to_system_time(local: &LocalDateTime) -> Option<SystemTime> {
    // SAFETY : `tm` est une structure C de nombres, valide une fois remplie de zéros.
    let mut broken_down: libc::tm = unsafe { std::mem::zeroed() };
    broken_down.tm_year = local.year - 1900;
    broken_down.tm_mon = i32::try_from(local.month).ok()? - 1;
    broken_down.tm_mday = i32::try_from(local.day).ok()?;
    broken_down.tm_hour = i32::try_from(local.hour).ok()?;
    broken_down.tm_min = i32::try_from(local.minute).ok()?;
    broken_down.tm_sec = i32::try_from(local.second).ok()?;
    // Heure d'été : à déterminer par `mktime`.
    broken_down.tm_isdst = -1;
    // SAFETY : `mktime` lit et normalise la structure qu'on lui passe, sans la conserver.
    let seconds = unsafe { libc::mktime(&mut broken_down) };
    let seconds = u64::try_from(seconds).ok()?;
    Some(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
}

/// `MM/DD/YYYY HH:MM:SS` -> composantes, bornées (mois 1-12, jour 1-31, heure < 24, minutes et secondes < 60).
fn parse_date_time(text: &str) -> Option<LocalDateTime> {
    let (date, time) = text.split_once(' ')?;
    let [month, day, year] = numbers(date, '/')?;
    let [hour, minute, second] = numbers(time.trim(), ':')?;
    let valid = (1..=12).contains(&month)
        && (1..=31).contains(&day)
        && hour < 24
        && minute < 60
        && second < 60;
    valid.then_some(LocalDateTime {
        year: i32::try_from(year).ok()?,
        month,
        day,
        hour,
        minute,
        second,
    })
}

/// Exactement trois nombres séparés par `separator`.
fn numbers(text: &str, separator: char) -> Option<[u32; 3]> {
    let mut parts = text.split(separator).map(|part| part.parse::<u32>().ok());
    let numbers = [parts.next()??, parts.next()??, parts.next()??];
    parts.next().is_none().then_some(numbers)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERVICE_ID: &str = "00000000-0000-4000-8000-000000000001";

    /// Sortie réduite et anonymisée de `scutil --nc status` quand le VPN est connecté.
    const CONNECTED_OUTPUT: &str = "Connected
Extended Status <dictionary> {
  IsPrimaryInterface : 1
  LastStatusChangeTime : 10/16/2026 18:53:04
  NEStatus : 3
  Status : 2
  VPN : <dictionary> {
    ConnectTime : 31923
  }
}
";

    fn list_output() -> String {
        format!(
            "Available network connection services in the current set (*=enabled):
* (Disconnected)   11111111-2222-4333-8444-555555555555 IPSec              \"Autre VPN\"  [IPSec]
* (Connected)      {SERVICE_ID} VPN (com.fortinet.forticlient.macos.vpn) \"Mon VPN\"  {FORTICLIENT_SERVICE}
"
        )
    }

    #[test]
    fn finds_the_forticlient_service_among_others() {
        // Arrange
        let output = list_output();

        // Act
        let service_id = parse_service_id(&output);

        // Assert
        assert_eq!(service_id, Some(SERVICE_ID));
    }

    #[test]
    fn no_forticlient_service_is_none() {
        let output = "Available network connection services in the current set (*=enabled):\n";

        assert_eq!(parse_service_id(output), None);
    }

    #[test]
    fn connected_reads_the_last_status_change_time() {
        let status = parse_scutil(CONNECTED_OUTPUT);

        assert_eq!(
            status,
            Some(ScutilStatus::Connected {
                since: LocalDateTime {
                    year: 2026,
                    month: 10,
                    day: 16,
                    hour: 18,
                    minute: 53,
                    second: 4,
                }
            })
        );
    }

    #[test]
    fn disconnected_is_disconnected() {
        let status =
            parse_scutil("Disconnected\nExtended Status <dictionary> {\n  Status : 0\n}\n");

        assert_eq!(status, Some(ScutilStatus::Disconnected));
    }

    #[test]
    fn connecting_is_disconnected() {
        let status = parse_scutil("Connecting\nExtended Status <dictionary> {\n}\n");

        assert_eq!(status, Some(ScutilStatus::Disconnected));
    }

    #[test]
    fn reasserting_is_unknown() {
        let status = parse_scutil(&CONNECTED_OUTPUT.replace("Connected", "Reasserting"));

        assert_eq!(status, None);
    }

    #[test]
    fn an_unrecognized_state_is_unknown() {
        assert_eq!(parse_scutil("No service\n"), None);
    }

    #[test]
    fn connected_without_change_time_is_unknown() {
        assert_eq!(
            parse_scutil("Connected\nExtended Status <dictionary> {\n}\n"),
            None
        );
    }

    #[test]
    fn unreadable_change_time_is_unknown() {
        let output = CONNECTED_OUTPUT.replace("10/16/2026 18:53:04", "bientôt");

        assert_eq!(parse_scutil(&output), None);
    }

    #[test]
    fn out_of_range_change_time_is_unknown() {
        let output = CONNECTED_OUTPUT.replace("10/16/2026 18:53:04", "13/16/2026 25:53:04");

        assert_eq!(parse_scutil(&output), None);
    }

    #[test]
    fn empty_output_is_unknown() {
        assert_eq!(parse_scutil(""), None);
    }

    #[test]
    fn elapsed_is_the_time_since_the_start() {
        let start = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);

        let elapsed = elapsed_since(start, start + Duration::from_secs(90));

        assert_eq!(elapsed, Duration::from_secs(90));
    }

    #[test]
    fn elapsed_is_zero_when_the_start_is_in_the_future() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);

        let elapsed = elapsed_since(now + Duration::from_secs(60), now);

        assert_eq!(elapsed, Duration::ZERO);
    }
}

#[cfg(all(test, target_os = "macos"))]
mod macos_tests {
    use super::*;
    use crate::test_support::TestDir;
    use std::process::Command;

    const SERVICE_ID: &str = "00000000-0000-4000-8000-000000000001";
    /// Instant de connexion des faux `scutil` : 2026-10-16 vers 16:53 UTC.
    const CONNECTED_AT: u64 = 1_792_169_584;

    fn at(seconds: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
    }

    /// Heure locale de `seconds` au format de `scutil`, calculée par `date` (référence indépendante de `mktime`).
    fn local_text(seconds: u64) -> String {
        let output = Command::new("date")
            .args(["-r", &seconds.to_string(), "+%m/%d/%Y %H:%M:%S"])
            .output()
            .expect("date");
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    /// Faux `scutil` : `--nc list` liste le service FortiClient, `--nc status <id>` affiche `status_output`.
    fn fake_scutil(dir: &TestDir, status_output: &str) -> String {
        let list_output = format!(
            "* (Connected)      {SERVICE_ID} VPN (com.fortinet.forticlient.macos.vpn) \"Mon VPN\"  {FORTICLIENT_SERVICE}"
        );
        let body = format!(
            "case \"$2 $3\" in\n  'list ') printf '%s\\n' '{list_output}' ;;\n  'status {SERVICE_ID}') printf '%s\\n' '{status_output}' ;;\n  *) exit 1 ;;\nesac"
        );
        dir.write_script("scutil", &body)
            .to_str()
            .expect("chemin lisible")
            .to_owned()
    }

    #[test]
    fn reads_the_time_elapsed_since_the_connection() {
        // Arrange
        let dir = TestDir::new("scutil-connected");
        let output = format!(
            "Connected\nExtended Status <dictionary> {{\n  LastStatusChangeTime : {}\n}}",
            local_text(CONNECTED_AT)
        );
        let scutil_path = fake_scutil(&dir, &output);

        // Act
        let status = read_status(&scutil_path, at(CONNECTED_AT + 3600));

        // Assert
        assert_eq!(
            status,
            Ok(Status::Connected {
                elapsed: Duration::from_secs(3600)
            })
        );
    }

    #[test]
    fn reads_a_disconnected_vpn() {
        // Arrange
        let dir = TestDir::new("scutil-disconnected");
        let scutil_path = fake_scutil(&dir, "Disconnected");

        // Act
        let status = read_status(&scutil_path, at(CONNECTED_AT));

        // Assert
        assert_eq!(status, Ok(Status::Disconnected));
    }

    #[test]
    fn a_missing_forticlient_service_is_an_error_not_a_disconnection() {
        // Arrange
        let dir = TestDir::new("scutil-no-service");
        let scutil_path = dir
            .write_script("scutil", "echo 'Available network connection services'")
            .display()
            .to_string();

        // Act
        let status = read_status(&scutil_path, at(CONNECTED_AT));

        // Assert
        let error = status.expect_err("service absent");
        assert!(error.contains("introuvable"), "{error}");
    }

    #[test]
    fn a_missing_scutil_is_an_error() {
        // Arrange
        let dir = TestDir::new("scutil-missing");
        let scutil_path = dir.path().join("absent").display().to_string();

        // Act
        let status = read_status(&scutil_path, at(CONNECTED_AT));

        // Assert
        assert!(status.is_err());
    }

    #[test]
    fn the_error_message_never_repeats_the_output_of_scutil() {
        // Arrange
        let dir = TestDir::new("scutil-secret");
        let scutil_path = fake_scutil(&dir, "Bizarre\n  authToken : SECRET-123");

        // Act
        let status = read_status(&scutil_path, at(CONNECTED_AT));

        // Assert
        let error = status.expect_err("sortie inattendue");
        assert!(!error.contains("SECRET-123"), "{error}");
    }
}
