//! FortiClient sous macOS : le VPN est une Network Extension, lue avec l'outil système `scutil --nc`.
//!
//! La sortie de `scutil --nc status` contient le jeton de session VPN, l'identifiant de l'utilisateur, des IP et
//! des domaines internes : elle n'est **jamais** recopiée dans un message.

// L'analyse est compilée (et testée) sur tous les systèmes ; seul macOS l'utilise.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use super::field;

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
/// `None` si l'état est inconnu : sortie inattendue, connecté sans heure lisible, ou reconnexion en cours
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
        "Reasserting" => None,
        _ => Some(ScutilStatus::Disconnected),
    }
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
}
