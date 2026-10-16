mod alert;
mod config;
mod session;
#[cfg(test)]
mod test_support;
mod tray;
mod update;
mod vpn;

use session::{Alert, Tracker, format_duration};
use std::cell::RefCell;
use std::env;
use std::rc::Rc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tray::{Tray, View};
use update::Updater;

fn main() {
    // GTK doit être initialisé avant de créer une fenêtre (échoue s'il n'y a pas d'écran).
    if let Err(error) = gtk::init() {
        eprintln!("Impossible d'initialiser GTK : {error}");
        return;
    }

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
    // Sans indicateur (bibliothèque absente), les fenêtres d'alerte continuent de fonctionner.
    let mut tray = Tray::new();
    // `None` quand la mise à jour automatique est désactivée (voir `Updater::from_env`).
    let mut updater = Updater::from_env(Instant::now());

    let check = Rc::new(RefCell::new(move || {
        let now = SystemTime::now();
        let view = match vpn::read_status(&cli_path) {
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
    }));

    // Une vérification dès que la boucle tourne (le texte provisoire de l'indicateur est alors remplacé, ce que
    // la barre affiche), puis une par minute, dans la boucle d'événements GTK.
    let first_check = Rc::clone(&check);
    gtk::glib::timeout_add_seconds_local_once(config::FIRST_CHECK_SECONDS, move || {
        (first_check.borrow_mut())();
    });
    gtk::glib::timeout_add_seconds_local(config::CHECK_INTERVAL_SECONDS, move || {
        (check.borrow_mut())();
        gtk::glib::ControlFlow::Continue
    });
    gtk::main();
}

fn show_alert(alert: &Alert) {
    let cut_at_text = clock(alert.cut_at);
    println!(
        "Le VPN sera coupé dans {} (vers {cut_at_text}).",
        format_duration(alert.remaining_minutes),
    );
    alert::show(alert, &cut_at_text);
}

/// Heure locale `HH:MM` d'un instant donné.
fn clock(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());

    i64::try_from(seconds)
        .ok()
        .and_then(|seconds| gtk::glib::DateTime::from_unix_local(seconds).ok())
        .and_then(|date| date.format("%H:%M").ok())
        .map_or_else(|| String::from("?"), |formatted| formatted.to_string())
}
