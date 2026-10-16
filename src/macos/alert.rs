use crate::session::{Alert, format_duration};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSAlert, NSAlertStyle, NSApplication};
use objc2_foundation::NSString;
use std::cell::RefCell;

const HEADING: &str = "Coupure du VPN imminente";

thread_local! {
    /// Alerte affichée en ce moment. `runModal` lance une boucle d'événements : ouvrir une seconde alerte
    /// pendant la première imbriquerait les boucles (plantage après quelques centaines), on met donc à jour
    /// celle qui est ouverte.
    static OPEN_ALERT: RefCell<Option<Retained<NSAlert>>> = const { RefCell::new(None) };
}

/// Alerte « il reste `alert.remaining_minutes` avant la coupure à `cut_at_text` ».
///
/// `show()` ne bloque pas : l'alerte (modale) s'ouvre juste après, depuis la boucle d'événements, et la
/// vérification de chaque minute continue pendant qu'elle est affichée.
pub fn show(alert: &Alert, cut_at_text: &str) {
    let heading = format!("{HEADING} : {}", format_duration(alert.remaining_minutes));
    let detail = format!(
        "{} restantes · coupure vers {cut_at_text}",
        format_duration(alert.remaining_minutes)
    );
    super::schedule(0.0, false, move || present(&heading, &detail));
}

fn present(heading: &str, detail: &str) {
    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };
    // Une application « accessoire » n'est pas au premier plan : sans cela, l'alerte s'ouvrirait derrière.
    #[allow(deprecated)]
    NSApplication::sharedApplication(main_thread).activateIgnoringOtherApps(true);

    let open_alert = OPEN_ALERT.with_borrow(Clone::clone);
    if let Some(alert) = open_alert {
        set_texts(&alert, heading, detail);
        alert.layout();
        alert.window().makeKeyAndOrderFront(None);
        return;
    }

    let alert = NSAlert::new(main_thread);
    alert.setAlertStyle(NSAlertStyle::Warning);
    set_texts(&alert, heading, detail);
    alert.addButtonWithTitle(&NSString::from_str("Compris"));
    OPEN_ALERT.set(Some(alert.clone()));
    // Pas d'emprunt de `OPEN_ALERT` pendant `runModal` : la vérification suivante peut y accéder.
    alert.runModal();
    OPEN_ALERT.set(None);
}

fn set_texts(alert: &NSAlert, heading: &str, detail: &str) {
    alert.setMessageText(&NSString::from_str(heading));
    alert.setInformativeText(&NSString::from_str(detail));
}
