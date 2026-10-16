use crate::session::{Alert, format_duration};
use gtk::prelude::*;

const WINDOW_TITLE: &str = "VPN : coupure imminente";
const HEADING: &str = "Coupure du VPN imminente";
const SHADOW_MARGIN: i32 = 24;

/// Couleurs du thème GTK (clair ou sombre) : la fenêtre suit le thème du bureau.
/// Le fond de la fenêtre est transparent pour laisser voir les coins arrondis et l'ombre de la carte.
const CSS: &str = "
window.alert-window { background: transparent; }
.alert-card {
    background-color: @theme_bg_color;
    color: @theme_fg_color;
    border-radius: 16px;
    padding: 28px 36px 24px 36px;
    box-shadow: 0 6px 24px alpha(black, 0.35);
}
.alert-icon { color: @warning_color; }
.alert-heading { font-weight: 600; font-size: 13pt; }
.alert-time { font-weight: 800; font-size: 40pt; color: @warning_color; }
.alert-detail { color: alpha(@theme_fg_color, 0.65); }
.alert-button { border-radius: 10px; padding: 8px 24px; }
";

/// Fenêtre d'alerte « il reste `alert.remaining_minutes` avant la coupure à `cut_at_text` ».
/// `show()` ne bloque pas : la boucle de vérification continue pendant que la fenêtre est ouverte.
pub fn show(alert: &Alert, cut_at_text: &str) {
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title(WINDOW_TITLE);
    window.set_decorated(false);
    window.set_resizable(false);
    window.set_keep_above(true);
    window.set_position(gtk::WindowPosition::Center);
    window.set_app_paintable(true);
    window.style_context().add_class("alert-window");
    // Sans visuel RGBA, le fond transparent serait rendu en noir autour de la carte.
    if let Some(visual) =
        gtk::prelude::WidgetExt::screen(&window).and_then(|screen| screen.rgba_visual())
    {
        window.set_visual(Some(&visual));
    }
    apply_css(&window);

    let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
    card.style_context().add_class("alert-card");
    card.set_margin_top(SHADOW_MARGIN);
    card.set_margin_bottom(SHADOW_MARGIN);
    card.set_margin_start(SHADOW_MARGIN);
    card.set_margin_end(SHADOW_MARGIN);

    let icon = gtk::Image::from_icon_name(Some("dialog-warning-symbolic"), gtk::IconSize::Dialog);
    icon.set_pixel_size(48);
    icon.style_context().add_class("alert-icon");
    card.pack_start(&icon, false, false, 0);
    card.pack_start(&label(HEADING, "alert-heading"), false, false, 0);
    card.pack_start(
        &label(&format_duration(alert.remaining_minutes), "alert-time"),
        false,
        false,
        0,
    );
    card.pack_start(
        &label(&format!("restantes · vers {cut_at_text}"), "alert-detail"),
        false,
        false,
        8,
    );

    let button = gtk::Button::with_label("Compris");
    button.style_context().add_class("alert-button");
    button.style_context().add_class("suggested-action");
    let closing = window.clone();
    button.connect_clicked(move |_| closing.close());
    card.pack_start(&button, false, false, 0);

    window.add(&card);
    window.connect_key_press_event(|window, event| {
        use gtk::gdk::keys::constants as key;
        let pressed = event.keyval();
        if pressed == key::Escape || pressed == key::Return || pressed == key::KP_Enter {
            window.close();
            gtk::glib::Propagation::Stop
        } else {
            gtk::glib::Propagation::Proceed
        }
    });
    window.show_all();
}

fn label(text: &str, css_class: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.style_context().add_class(css_class);
    label
}

fn apply_css(window: &gtk::Window) {
    let provider = gtk::CssProvider::new();
    if let Err(error) = provider.load_from_data(CSS.as_bytes()) {
        // Sans style personnalisé, la fenêtre reste utilisable avec le thème du système.
        eprintln!("Style de l'alerte invalide : {error}");
        return;
    }
    if let Some(screen) = gtk::prelude::WidgetExt::screen(window) {
        gtk::StyleContext::add_provider_for_screen(
            &screen,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
