use crate::config::{SESSION_DURATION, WARNING_THRESHOLDS_MINUTES};
use crate::vpn::Status;
use std::time::{Duration, SystemTime};

const SECONDS_PER_MINUTE: u64 = 60;
pub const SESSION_MINUTES: u64 = SESSION_DURATION.as_secs() / SECONDS_PER_MINUTE;

/// Si le début de session estimé avance de plus que cela, c'est une nouvelle connexion
/// (l'ancienne a été remplacée sans qu'on ait vu l'état « déconnecté »).
const NEW_SESSION_TOLERANCE: Duration = Duration::from_secs(2 * SECONDS_PER_MINUTE);

/// Alerte à afficher : il reste `remaining_minutes` avant la coupure à `cut_at`.
#[derive(Debug, PartialEq, Eq)]
pub struct Alert {
    pub remaining_minutes: u64,
    pub cut_at: SystemTime,
}

/// Photographie de la session en cours, pour l'affichage (barre du bureau).
#[derive(Debug, PartialEq, Eq)]
pub struct SessionInfo {
    pub remaining_minutes: u64,
    pub elapsed_minutes: u64,
    pub cut_at: SystemTime,
}

struct Session {
    start: SystemTime,
    /// Seuils (en minutes) déjà franchis, pour ne pas alerter deux fois.
    notified: Vec<u64>,
}

/// Suit la session VPN en cours à partir de la durée de connexion rapportée par FortiClient.
pub struct Tracker {
    thresholds: Vec<u64>,
    session: Option<Session>,
}

impl Default for Tracker {
    fn default() -> Self {
        Self::new(WARNING_THRESHOLDS_MINUTES.to_vec())
    }
}

impl Tracker {
    /// `thresholds` : minutes restantes auxquelles une alerte doit partir.
    pub fn new(thresholds: Vec<u64>) -> Self {
        Self {
            thresholds,
            session: None,
        }
    }

    /// Session en cours à l'instant `now`, `None` si le VPN n'est pas connecté.
    pub fn current(&self, now: SystemTime) -> Option<SessionInfo> {
        let session = self.session.as_ref()?;
        let elapsed = now.duration_since(session.start).unwrap_or_default();

        Some(SessionInfo {
            remaining_minutes: remaining_minutes(session.start, now),
            elapsed_minutes: elapsed.as_secs() / SECONDS_PER_MINUTE,
            cut_at: session.start + SESSION_DURATION,
        })
    }

    /// Intègre l'état du VPN ; renvoie une alerte si un seuil vient d'être franchi.
    pub fn update(&mut self, status: &Status, now: SystemTime) -> Option<Alert> {
        let Status::Connected { elapsed } = status else {
            self.session = None;
            return None;
        };

        // Début réel de la connexion, déduit de la durée affichée par FortiClient.
        let start = now.checked_sub(*elapsed).unwrap_or(now);

        let is_same_session = self
            .session
            .as_ref()
            .is_some_and(|session| start <= session.start + NEW_SESSION_TOLERANCE);
        if !is_same_session {
            self.session = None;
        }
        let session = self.session.get_or_insert_with(|| Session {
            start,
            notified: Vec::new(),
        });

        let remaining = remaining_minutes(session.start, now);
        session.notified = due_warning(remaining, &self.thresholds, &session.notified)?;

        Some(Alert {
            remaining_minutes: remaining,
            cut_at: session.start + SESSION_DURATION,
        })
    }
}

/// Minutes restantes avant la fin de la session, arrondies au supérieur (0 si dépassée).
pub fn remaining_minutes(start: SystemTime, now: SystemTime) -> u64 {
    match (start + SESSION_DURATION).duration_since(now) {
        Ok(left) => left.as_secs().div_ceil(SECONDS_PER_MINUTE),
        Err(_) => 0,
    }
}

/// S'il existe un seuil franchi et pas encore notifié, renvoie tous les seuils franchis
/// (à mémoriser comme « notifiés » : une seule alerte même si plusieurs seuils sont passés d'un coup).
fn due_warning(remaining: u64, thresholds: &[u64], notified: &[u64]) -> Option<Vec<u64>> {
    if remaining == 0 {
        return None;
    }

    let crossed: Vec<u64> = thresholds
        .iter()
        .copied()
        .filter(|threshold| remaining <= *threshold)
        .collect();
    let has_pending = crossed
        .iter()
        .any(|threshold| !notified.contains(threshold));

    has_pending.then_some(crossed)
}

/// 75 -> "1h15", 15 -> "15 min".
pub fn format_duration(minutes: u64) -> String {
    let hours = minutes / 60;
    let rest = minutes % 60;
    if hours == 0 {
        format!("{rest} min")
    } else {
        format!("{hours}h{rest:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minutes(count: u64) -> Duration {
        Duration::from_secs(count * SECONDS_PER_MINUTE)
    }

    fn at_minute(count: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + minutes(count)
    }

    /// Observe le VPN à la minute `now`, connecté depuis la minute `connected_at`.
    fn observe(tracker: &mut Tracker, connected_at: u64, now: u64) -> Option<Alert> {
        let status = Status::Connected {
            elapsed: minutes(now - connected_at),
        };
        tracker.update(&status, at_minute(now))
    }

    fn remaining_of(alert: Option<Alert>) -> Option<u64> {
        alert.map(|alert| alert.remaining_minutes)
    }

    #[test]
    fn no_alert_right_after_connection() {
        let mut tracker = Tracker::default();

        let alert = observe(&mut tracker, 0, 0);

        assert_eq!(alert, None);
    }

    #[test]
    fn alerts_one_hour_before_the_cut() {
        let mut tracker = Tracker::default();

        let alert = observe(&mut tracker, 0, SESSION_MINUTES - 60);

        assert_eq!(remaining_of(alert), Some(60));
    }

    #[test]
    fn alerts_once_per_threshold() {
        let mut tracker = Tracker::default();
        observe(&mut tracker, 0, SESSION_MINUTES - 60);

        let again = observe(&mut tracker, 0, SESSION_MINUTES - 59);

        assert_eq!(again, None);
    }

    #[test]
    fn alerts_fifteen_minutes_before_the_cut() {
        let mut tracker = Tracker::default();
        observe(&mut tracker, 0, SESSION_MINUTES - 60);

        let alert = observe(&mut tracker, 0, SESSION_MINUTES - 15);

        assert_eq!(remaining_of(alert), Some(15));
    }

    #[test]
    fn reports_the_cut_time() {
        let mut tracker = Tracker::default();

        let alert = observe(&mut tracker, 0, SESSION_MINUTES - 60);

        assert_eq!(
            alert.map(|alert| alert.cut_at),
            Some(at_minute(SESSION_MINUTES))
        );
    }

    #[test]
    fn app_started_mid_session_uses_the_real_connection_time() {
        // Cas réel : FortiClient affiche 07:02 de connexion au démarrage de l'app.
        let mut tracker = Tracker::default();
        let status = Status::Connected {
            elapsed: minutes(7 * 60 + 2),
        };

        let alert = tracker.update(&status, at_minute(1000));

        assert_eq!(
            alert,
            Some(Alert {
                remaining_minutes: 58,
                cut_at: at_minute(1000 - (7 * 60 + 2) + SESSION_MINUTES),
            })
        );
    }

    #[test]
    fn mid_session_start_only_fires_the_lowest_crossed_threshold() {
        let mut tracker = Tracker::default();

        let alert = observe(&mut tracker, 0, SESSION_MINUTES - 10);

        assert_eq!(remaining_of(alert), Some(10));
        assert_eq!(observe(&mut tracker, 0, SESSION_MINUTES - 9), None);
    }

    #[test]
    fn small_clock_drift_does_not_restart_the_session() {
        let mut tracker = Tracker::default();
        observe(&mut tracker, 0, SESSION_MINUTES - 60);
        let drifted = Status::Connected {
            elapsed: minutes(SESSION_MINUTES - 59) - Duration::from_secs(1),
        };

        let alert = tracker.update(&drifted, at_minute(SESSION_MINUTES - 59));

        assert_eq!(alert, None);
    }

    #[test]
    fn disconnection_ends_the_session_and_reconnection_restarts_the_clock() {
        let mut tracker = Tracker::default();
        observe(&mut tracker, 0, SESSION_MINUTES - 60);
        tracker.update(&Status::Disconnected, at_minute(SESSION_MINUTES - 50));

        let alert = observe(&mut tracker, 600, 600 + SESSION_MINUTES - 60);

        assert_eq!(remaining_of(alert), Some(60));
    }

    #[test]
    fn reconnection_without_seeing_the_disconnection_is_a_new_session() {
        let mut tracker = Tracker::default();
        observe(&mut tracker, 0, SESSION_MINUTES - 60);
        // La durée repart de zéro : nouvelle connexion à la minute 500.
        observe(&mut tracker, 500, 505);

        let alert = observe(&mut tracker, 500, 500 + SESSION_MINUTES - 60);

        assert_eq!(remaining_of(alert), Some(60));
    }

    #[test]
    fn no_alert_once_the_session_is_over() {
        let mut tracker = Tracker::default();

        let alert = observe(&mut tracker, 0, SESSION_MINUTES + 1);

        assert_eq!(alert, None);
    }

    #[test]
    fn one_threshold_per_minute_alerts_every_minute() {
        let mut tracker = Tracker::new((1..=SESSION_MINUTES).collect());

        let first = observe(&mut tracker, 0, 0);
        let second = observe(&mut tracker, 0, 1);
        let third = observe(&mut tracker, 0, 2);

        assert_eq!(remaining_of(first), Some(480));
        assert_eq!(remaining_of(second), Some(479));
        assert_eq!(remaining_of(third), Some(478));
    }

    #[test]
    fn no_current_session_before_any_observation() {
        let tracker = Tracker::default();

        let info = tracker.current(at_minute(0));

        assert_eq!(info, None);
    }

    #[test]
    fn current_session_reports_remaining_elapsed_and_cut_time() {
        let mut tracker = Tracker::default();
        let status = Status::Connected {
            elapsed: minutes(7 * 60 + 6),
        };
        tracker.update(&status, at_minute(1000));

        let info = tracker.current(at_minute(1000));

        assert_eq!(
            info,
            Some(SessionInfo {
                remaining_minutes: 54,
                elapsed_minutes: 7 * 60 + 6,
                cut_at: at_minute(1000 - (7 * 60 + 6) + SESSION_MINUTES),
            })
        );
    }

    #[test]
    fn current_session_follows_the_clock() {
        let mut tracker = Tracker::default();
        observe(&mut tracker, 0, 10);

        let info = tracker.current(at_minute(70));

        assert_eq!(
            info.map(|info| (info.remaining_minutes, info.elapsed_minutes)),
            Some((SESSION_MINUTES - 70, 70))
        );
    }

    #[test]
    fn no_current_session_after_disconnection() {
        let mut tracker = Tracker::default();
        observe(&mut tracker, 0, 10);
        tracker.update(&Status::Disconnected, at_minute(11));

        let info = tracker.current(at_minute(11));

        assert_eq!(info, None);
    }

    #[test]
    fn remaining_minutes_rounds_up() {
        let now = at_minute(0) + Duration::from_secs(30);

        let remaining = remaining_minutes(at_minute(0), now);

        assert_eq!(remaining, SESSION_MINUTES);
    }

    #[test]
    fn formats_durations() {
        assert_eq!(format_duration(60), "1h00");
        assert_eq!(format_duration(75), "1h15");
        assert_eq!(format_duration(15), "15 min");
    }
}
