//! FortiClient sous Linux : `forticlient-cli vpn status`.

// Compilé (et testé) sur tous les systèmes ; seul Linux l'utilise.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use super::{Status, field};
use std::process::Command;
use std::time::Duration;

/// Interroge `forticlient-cli vpn status`.
///
/// `Err` signifie que l'état est inconnu (binaire absent, sortie inattendue) : l'appelant ne doit pas
/// en déduire que le VPN est coupé.
pub fn read_status(cli_path: &str) -> Result<Status, String> {
    let output = Command::new(cli_path)
        .args(["vpn", "status"])
        .output()
        .map_err(|error| format!("impossible de lancer {cli_path} : {error}"))?;

    // La CLI écrit le même texte sur stdout et sur stderr (FortiClient 7.4.3) : on lit stdout et, à défaut,
    // stderr. On analyse même si le code retour n'est pas 0, au cas où il servirait pour « déconnecté ».
    let text = String::from_utf8_lossy(&output.stdout);
    let error_text = String::from_utf8_lossy(&output.stderr);
    if let Some(status) = parse_status(&text).or_else(|| parse_status(&error_text)) {
        return Ok(status);
    }

    if output.status.success() {
        Err(format!(
            "sortie inattendue de « {cli_path} vpn status » : {}",
            text.trim()
        ))
    } else {
        Err(format!(
            "« {cli_path} vpn status » a échoué ({}) : {}",
            output.status,
            error_text.trim()
        ))
    }
}

/// Analyse la sortie de `forticlient-cli vpn status`.
///
/// `None` si la sortie n'a pas la forme attendue (pas de ligne `Status:`, ou connecté sans durée lisible).
pub fn parse_status(output: &str) -> Option<Status> {
    if field(output, "Status")? != "Connected" {
        return Some(Status::Disconnected);
    }

    let elapsed = parse_duration(field(output, "Duration")?)?;
    Some(Status::Connected { elapsed })
}

/// `HH:MM:SS` (les heures peuvent dépasser 24) -> durée.
fn parse_duration(text: &str) -> Option<Duration> {
    let mut parts = text.split(':');
    let hours: u64 = parts.next()?.parse().ok()?;
    let minutes: u64 = parts.next()?.parse().ok()?;
    let seconds: u64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }

    let total = hours
        .checked_mul(3600)?
        .checked_add(minutes.checked_mul(60)?)?
        .checked_add(seconds)?;
    Some(Duration::from_secs(total))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDir;

    const CONNECTED_OUTPUT: &str = "Status: Connected
  VPN name: Corp
  Username: user@example.com
  IP: 10.0.0.2
  Sent bytes: 399508768
  Recv bytes: 3382736587
  Duration: 07:02:40
";

    fn connected_since(seconds: u64) -> Option<Status> {
        Some(Status::Connected {
            elapsed: Duration::from_secs(seconds),
        })
    }

    #[test]
    fn parses_a_real_connected_output() {
        let status = parse_status(CONNECTED_OUTPUT);

        assert_eq!(status, connected_since(7 * 3600 + 2 * 60 + 40));
    }

    #[test]
    fn parses_a_short_duration() {
        let status = parse_status("Status: Connected\n  Duration: 00:00:05\n");

        assert_eq!(status, connected_since(5));
    }

    #[test]
    fn parses_more_than_24_hours() {
        let status = parse_status("Status: Connected\n  Duration: 25:00:00\n");

        assert_eq!(status, connected_since(25 * 3600));
    }

    #[test]
    fn any_other_status_is_disconnected() {
        let status = parse_status("Status: Disconnected\n");

        assert_eq!(status, Some(Status::Disconnected));
    }

    #[test]
    fn parses_the_real_output_when_the_vpn_is_cut() {
        // Sortie observée après la coupure de la session de 8 h.
        let status = parse_status("Status: Not Running\n");

        assert_eq!(status, Some(Status::Disconnected));
    }

    #[test]
    fn connecting_is_not_connected() {
        let status = parse_status("Status: Connecting\n  VPN name: Corp\n");

        assert_eq!(status, Some(Status::Disconnected));
    }

    #[test]
    fn empty_output_is_unexpected() {
        assert_eq!(parse_status(""), None);
    }

    #[test]
    fn output_without_status_line_is_unexpected() {
        assert_eq!(parse_status("Error: daemon not running\n"), None);
    }

    #[test]
    fn connected_without_duration_is_unexpected() {
        assert_eq!(parse_status("Status: Connected\n  VPN name: Corp\n"), None);
    }

    #[test]
    fn connected_with_unreadable_duration_is_unexpected() {
        assert_eq!(
            parse_status("Status: Connected\n  Duration: bientôt\n"),
            None
        );
    }

    #[test]
    fn duration_with_wrong_number_of_parts_is_unexpected() {
        assert_eq!(parse_status("Status: Connected\n  Duration: 07:02\n"), None);
    }

    /// Faux `forticlient-cli` : un script `sh` dont le corps imite la sortie de la vraie CLI.
    fn fake_cli(dir: &TestDir, body: &str) -> String {
        dir.write_script("forticlient-cli", body)
            .to_str()
            .expect("chemin lisible")
            .to_owned()
    }

    #[test]
    fn read_status_reads_the_standard_output() {
        // Arrange
        let dir = TestDir::new("vpn-stdout");
        let cli_path = fake_cli(&dir, &format!("printf '%s' '{CONNECTED_OUTPUT}'"));

        // Act
        let status = read_status(&cli_path);

        // Assert
        assert_eq!(status.ok(), connected_since(7 * 3600 + 2 * 60 + 40));
    }

    #[test]
    fn read_status_reads_the_error_output_when_the_standard_output_is_empty() {
        // Arrange
        let dir = TestDir::new("vpn-stderr");
        let cli_path = fake_cli(&dir, "echo 'Status: Not Running' >&2");

        // Act
        let status = read_status(&cli_path);

        // Assert
        assert_eq!(status, Ok(Status::Disconnected));
    }

    #[test]
    fn read_status_reads_the_output_even_when_the_cli_fails() {
        // Arrange
        let dir = TestDir::new("vpn-exit-code");
        let cli_path = fake_cli(&dir, "echo 'Status: Not Running'\nexit 1");

        // Act
        let status = read_status(&cli_path);

        // Assert
        assert_eq!(status, Ok(Status::Disconnected));
    }

    #[test]
    fn read_status_of_a_missing_cli_is_an_error_not_a_disconnection() {
        // Arrange
        let dir = TestDir::new("vpn-missing");
        let cli_path = dir.path().join("absente").display().to_string();

        // Act
        let status = read_status(&cli_path);

        // Assert
        assert!(status.is_err());
    }

    #[test]
    fn read_status_of_an_unexpected_output_is_an_error() {
        // Arrange
        let dir = TestDir::new("vpn-unexpected");
        let cli_path = fake_cli(&dir, "echo 'Bonjour'");

        // Act
        let status = read_status(&cli_path);

        // Assert
        let error = status.expect_err("sortie inattendue");
        assert!(error.contains("sortie inattendue"), "{error}");
    }

    #[test]
    fn read_status_of_a_failing_cli_reports_its_error_output() {
        // Arrange
        let dir = TestDir::new("vpn-failing");
        let cli_path = fake_cli(&dir, "echo 'démon injoignable' >&2\nexit 3");

        // Act
        let status = read_status(&cli_path);

        // Assert
        let error = status.expect_err("CLI en échec");
        assert!(error.contains("démon injoignable"), "{error}");
    }
}
