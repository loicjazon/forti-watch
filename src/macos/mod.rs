//! Interface macOS (AppKit) : barre des menus, alerte, boucle d'événements.

pub mod alert;
mod tray;

use crate::config;
use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
use objc2_foundation::{NSRunLoop, NSRunLoopCommonModes, NSTimer};
use std::cell::RefCell;
use std::ptr::NonNull;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

pub use tray::Tray;

/// Initialise l'application, puis `setup` (qui peut créer l'indicateur) renvoie la vérification, appelée peu après
/// le démarrage puis toutes les minutes. Lance la boucle d'événements (ne revient pas tant que l'application tourne).
pub fn run<Check: FnMut() + 'static>(setup: impl FnOnce() -> Check) {
    let Some(main_thread) = MainThreadMarker::new() else {
        eprintln!("L'interface macOS doit démarrer sur le fil principal.");
        return;
    };
    let app = NSApplication::sharedApplication(main_thread);
    // Application « accessoire » : pas d'icône dans le Dock, seulement la barre des menus.
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let check = Rc::new(RefCell::new(setup()));
    let first_check = Rc::clone(&check);
    schedule(f64::from(config::FIRST_CHECK_SECONDS), false, move || {
        (first_check.borrow_mut())();
    });
    schedule(f64::from(config::CHECK_INTERVAL_SECONDS), true, move || {
        (check.borrow_mut())();
    });
    app.run();
}

/// Lance `action` dans `seconds` secondes (puis à chaque intervalle si `repeats`), sur le fil principal.
///
/// Le minuteur tourne dans tous les modes courants de la boucle : il continue pendant qu'une alerte ou le menu
/// est ouvert.
fn schedule(seconds: f64, repeats: bool, action: impl Fn() + 'static) {
    let block = RcBlock::new(move |_timer: NonNull<NSTimer>| action());
    // SAFETY : le bloc n'est appelé que par la boucle du fil principal, où il a été créé.
    let timer = unsafe { NSTimer::timerWithTimeInterval_repeats_block(seconds, repeats, &block) };
    // SAFETY : minuteur valide, ajouté à la boucle principale dans un mode prédéfini.
    unsafe { NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes) };
}

/// Heure locale `HH:MM` d'un instant donné.
pub fn clock(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let Ok(seconds) = libc::time_t::try_from(seconds) else {
        return String::from("?");
    };

    // SAFETY : `tm` est une structure C de nombres, valide une fois remplie de zéros.
    let mut broken_down: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY : les deux pointeurs viennent de références valides pendant l'appel.
    if unsafe { libc::localtime_r(&seconds, &mut broken_down) }.is_null() {
        return String::from("?");
    }
    format!("{:02}:{:02}", broken_down.tm_hour, broken_down.tm_min)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::time::Duration;

    #[test]
    fn clock_gives_the_local_time_like_the_date_command() {
        // Arrange
        let seconds: u64 = 1_792_169_584;
        let output = Command::new("date")
            .args(["-r", &seconds.to_string(), "+%H:%M"])
            .output()
            .expect("date");
        let expected = String::from_utf8_lossy(&output.stdout).trim().to_owned();

        // Act
        let text = clock(UNIX_EPOCH + Duration::from_secs(seconds));

        // Assert
        assert_eq!(text, expected);
    }
}
