mod config;
#[cfg(target_os = "linux")]
#[path = "linux/mod.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "macos/mod.rs"]
mod platform;
mod session;
#[cfg(test)]
mod test_support;
mod tray;
mod update;
mod vpn;

use platform::{Tray, clock};
use session::{Alert, Tracker, format_duration};
use std::env;
use std::time::{Instant, SystemTime};
use tray::View;
use update::Updater;

fn main() {
    // Mode test : `FORTI_WATCH_TEST_EVERY_MINUTE=1 forti-watch` alerte à chaque minute au lieu de 1 h / 15 min.
    let mut tracker = if env::var_os("FORTI_WATCH_TEST_EVERY_MINUTE").is_some() {
        println!("Mode test : une alerte par minute");
        Tracker::new((1..=session::SESSION_MINUTES).collect())
    } else {
        Tracker::default()
    };

    let cli_path =
        env::var("FORTI_WATCH_CLI").unwrap_or_else(|_| String::from(config::FORTICLIENT_CLI));
    // Dernière erreur de lecture affichée : on ne la répète pas à chaque minute.
    let mut last_error: Option<String> = None;
    // `None` quand la mise à jour automatique est désactivée (voir `Updater::from_env`).
    let mut updater = Updater::from_env(Instant::now());

    // L'indicateur se crée une fois l'interface initialisée, d'où la préparation passée à `run`.
    platform::run(move || {
        // Sans indicateur (bibliothèque absente), les fenêtres d'alerte continuent de fonctionner.
        let mut tray = Tray::new();
        move || {
            let now = SystemTime::now();
            let view = match vpn::read_status(&cli_path, now) {
                Ok(status) => {
                    last_error = None;
                    if let Some(alert) = tracker.update(&status, now) {
                        show_alert(&alert);
                    }
                    match tracker.current(now) {
                        Some(info) => View::Connected(info),
                        None => View::Off,
                    }
                }
                Err(error) => {
                    if last_error.as_deref() != Some(error.as_str()) {
                        eprintln!("{error}");
                        last_error = Some(error);
                    }
                    View::Unknown
                }
            };

            if let Some(updater) = updater.as_mut() {
                let remaining_minutes = tracker.current(now).map(|info| info.remaining_minutes);
                updater.poll(Instant::now(), remaining_minutes);
            }

            if let Some(tray) = tray.as_mut() {
                let cut_at_text = match &view {
                    View::Connected(info) => clock(info.cut_at),
                    View::Off | View::Unknown => String::new(),
                };
                tray.show(&view, &cut_at_text);
            }
        }
    });
}

fn show_alert(alert: &Alert) {
    let cut_at_text = clock(alert.cut_at);
    println!(
        "Le VPN sera coupé dans {} (vers {cut_at_text}).",
        format_duration(alert.remaining_minutes),
    );
    platform::alert::show(alert, &cut_at_text);
}
