use crate::tray::{View, label, menu_lines};
use gtk::prelude::*;
use libappindicator::{AppIndicator, AppIndicatorStatus};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::{env, fs};

/// Nom de l'icône (le logo) : un fichier `forti-watch.svg` écrit au lancement dans `icon_dir()`.
const ICON_NAME: &str = "forti-watch";
/// Logo en couleur embarqué dans le binaire (distribué seul, sans fichier annexe).
const ICON_SVG: &str = include_str!("../../docs/images/logo.svg");
/// Icône du thème, utilisée si le logo ne peut pas être écrit sur le disque.
const FALLBACK_ICON: &str = "appointment-soon-symbolic";

/// Dossier où est écrit le logo (`$XDG_DATA_HOME/forti-watch/icons`, par défaut `~/.local/share/...`).
fn icon_dir() -> Option<PathBuf> {
    let data_home = match env::var_os("XDG_DATA_HOME") {
        Some(path) if !path.is_empty() => PathBuf::from(path),
        _ => PathBuf::from(env::var_os("HOME")?).join(".local/share"),
    };
    Some(data_home.join("forti-watch/icons"))
}

/// Écrit le logo sur le disque (la barre le charge par nom) ; renvoie son dossier, ou `None` en cas d'échec.
fn write_icon() -> Option<PathBuf> {
    let dir = icon_dir()?;
    fs::create_dir_all(&dir).ok()?;
    let file = dir.join(format!("{ICON_NAME}.svg"));
    if fs::read_to_string(&file).ok().as_deref() != Some(ICON_SVG) {
        fs::write(&file, ICON_SVG).ok()?;
    }
    Some(dir)
}

/// Indicateur dans la barre supérieure du bureau (StatusNotifier / AppIndicator).
pub struct Tray {
    indicator: AppIndicator,
    first_line: gtk::MenuItem,
    second_line: gtk::MenuItem,
}

impl Tray {
    /// `None` si la bibliothèque appindicator n'est pas installée (l'app continue sans indicateur).
    pub fn new() -> Option<Self> {
        let icon_dir = write_icon();
        let created = catch_unwind(AssertUnwindSafe(|| match &icon_dir {
            Some(dir) => AppIndicator::with_path("forti-watch", ICON_NAME, &dir.to_string_lossy()),
            None => AppIndicator::new("forti-watch", FALLBACK_ICON),
        }));
        let mut indicator = match created {
            Ok(indicator) => indicator,
            Err(_) => {
                eprintln!(
                    "Indicateur de la barre indisponible : installez libayatana-appindicator3-1 \
                     (sudo apt install libayatana-appindicator3-1)"
                );
                return None;
            }
        };

        let first_line = gtk::MenuItem::with_label("");
        let second_line = gtk::MenuItem::with_label("");
        // Lignes d'information : grisées, non cliquables.
        first_line.set_sensitive(false);
        second_line.set_sensitive(false);
        let quit = gtk::MenuItem::with_label("Quitter forti-watch");
        quit.connect_activate(|_| gtk::main_quit());

        let mut menu = gtk::Menu::new();
        menu.append(&first_line);
        menu.append(&second_line);
        menu.append(&gtk::SeparatorMenuItem::new());
        menu.append(&quit);
        menu.show_all();

        // Texte provisoire : la barre ignore le tout premier texte posé, elle ne réagit qu'à un changement.
        indicator.set_label("…", "");
        indicator.set_status(AppIndicatorStatus::Active);
        indicator.set_menu(&mut menu);

        Some(Self {
            indicator,
            first_line,
            second_line,
        })
    }

    /// Met à jour le texte et le menu (l'icône est toujours le logo).
    pub fn show(&mut self, view: &View, cut_at_text: &str) {
        self.indicator.set_label(&label(view), "");

        let (first, second) = menu_lines(view, cut_at_text);
        self.first_line.set_label(&first);
        self.second_line.set_label(&second);
        self.second_line.set_visible(!second.is_empty());
    }
}
