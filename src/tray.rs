//! Ce que montre l'indicateur de la barre (texte et lignes du menu), commun à toutes les plateformes.

use crate::session::{SessionInfo, format_duration};

/// Ce que l'indicateur doit montrer.
pub enum View {
    Connected(SessionInfo),
    /// FortiClient dit que le VPN est coupé.
    Off,
    /// Impossible de lire l'état de FortiClient.
    Unknown,
}

/// Texte à côté de l'icône : temps restant, ou état du VPN.
pub fn label(view: &View) -> String {
    match view {
        View::Connected(info) => format_duration(info.remaining_minutes),
        View::Off => String::from("VPN off"),
        View::Unknown => String::from("VPN ?"),
    }
}

/// Deux lignes d'information du menu ; la seconde est vide quand il n'y a rien à dire.
/// `cut_at_text` : heure de coupure déjà formatée (`HH:MM`).
pub fn menu_lines(view: &View, cut_at_text: &str) -> (String, String) {
    match view {
        View::Connected(info) => (
            format!("Coupure vers {cut_at_text}"),
            format!("Connecté depuis {}", format_duration(info.elapsed_minutes)),
        ),
        View::Off => (String::from("VPN déconnecté"), String::new()),
        View::Unknown => (
            String::from("État du VPN inconnu"),
            String::from("FortiClient est introuvable ou illisible"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn connected(remaining_minutes: u64, elapsed_minutes: u64) -> View {
        View::Connected(SessionInfo {
            remaining_minutes,
            elapsed_minutes,
            cut_at: SystemTime::UNIX_EPOCH,
        })
    }

    #[test]
    fn label_shows_minutes_under_an_hour() {
        assert_eq!(label(&connected(54, 426)), "54 min");
    }

    #[test]
    fn label_shows_hours_and_minutes_above_an_hour() {
        assert_eq!(label(&connected(65, 415)), "1h05");
    }

    #[test]
    fn label_shows_zero_when_the_session_is_over() {
        assert_eq!(label(&connected(0, 480)), "0 min");
    }

    #[test]
    fn label_when_the_vpn_is_off() {
        assert_eq!(label(&View::Off), "VPN off");
    }

    #[test]
    fn label_when_the_state_is_unknown() {
        assert_eq!(label(&View::Unknown), "VPN ?");
    }

    #[test]
    fn menu_when_connected_gives_cut_time_and_elapsed_time() {
        let lines = menu_lines(&connected(54, 426), "14:52");

        assert_eq!(
            lines,
            (
                String::from("Coupure vers 14:52"),
                String::from("Connecté depuis 7h06")
            )
        );
    }

    #[test]
    fn menu_when_the_vpn_is_off_has_a_single_line() {
        let lines = menu_lines(&View::Off, "");

        assert_eq!(lines, (String::from("VPN déconnecté"), String::new()));
    }

    #[test]
    fn menu_when_the_state_is_unknown_explains_why() {
        let (first, second) = menu_lines(&View::Unknown, "");

        assert_eq!(first, "État du VPN inconnu");
        assert!(second.contains("FortiClient"), "{second}");
    }
}
