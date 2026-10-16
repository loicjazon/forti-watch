//! Interface Linux : boucle GTK, indicateur AppIndicator, fenêtre d'alerte.

pub mod alert;
mod tray;

use crate::config;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

pub use tray::Tray;

/// Initialise GTK, puis `setup` (qui peut créer l'indicateur) renvoie la vérification, appelée peu après le
/// démarrage puis toutes les minutes. Lance la boucle d'événements (ne revient pas tant que l'application
/// tourne). Sans écran, l'application s'arrête.
pub fn run<Check: FnMut() + 'static>(setup: impl FnOnce() -> Check) {
    // GTK doit être initialisé avant de créer une fenêtre (échoue s'il n'y a pas d'écran).
    if let Err(error) = gtk::init() {
        eprintln!("Impossible d'initialiser GTK : {error}");
        return;
    }

    let check = Rc::new(RefCell::new(setup()));
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

/// Heure locale `HH:MM` d'un instant donné.
pub fn clock(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());

    i64::try_from(seconds)
        .ok()
        .and_then(|seconds| gtk::glib::DateTime::from_unix_local(seconds).ok())
        .and_then(|date| date.format("%H:%M").ok())
        .map_or_else(|| String::from("?"), |formatted| formatted.to_string())
}
