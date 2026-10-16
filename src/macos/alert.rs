use crate::session::{Alert, format_duration};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSAlert, NSAlertStyle, NSApplication};
use objc2_foundation::NSString;

const HEADING: &str = "Coupure du VPN imminente";

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

    let alert = NSAlert::new(main_thread);
    alert.setAlertStyle(NSAlertStyle::Warning);
    alert.setMessageText(&NSString::from_str(heading));
    alert.setInformativeText(&NSString::from_str(detail));
    alert.addButtonWithTitle(&NSString::from_str("Compris"));
    alert.runModal();
}
