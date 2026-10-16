use crate::tray::{View, label, menu_lines};
use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly, sel};
use objc2_app_kit::{
    NSCellImagePosition, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::NSString;

/// Logo monochrome affiché devant le texte : image « template », que macOS colore selon le thème de la barre.
const ICON_SVG: &str = include_str!("../../docs/images/logo-mono.svg");
/// Hauteur du logo dans la barre des menus, en points.
const ICON_SIDE: f64 = 16.0;

/// Indicateur dans la barre des menus (`NSStatusItem`) : temps restant et menu d'information.
pub struct Tray {
    main_thread: MainThreadMarker,
    item: Retained<NSStatusItem>,
    first_line: Retained<NSMenuItem>,
    second_line: Retained<NSMenuItem>,
}

impl Tray {
    /// `None` hors du fil principal (l'app continue alors sans indicateur).
    pub fn new() -> Option<Self> {
        let main_thread = MainThreadMarker::new()?;
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        if let Some(button) = item.button(main_thread) {
            if let Some(icon) = super::svg_image(ICON_SVG, ICON_SIDE) {
                icon.setTemplate(true);
                button.setImage(Some(&icon));
                button.setImagePosition(NSCellImagePosition::ImageLeft);
            }
            button.setTitle(&NSString::from_str("…"));
        }

        let menu = NSMenu::new(main_thread);
        // Sans cela, AppKit réactiverait les lignes d'information (elles n'ont pas d'action).
        menu.setAutoenablesItems(false);
        let first_line = info_line(main_thread);
        let second_line = info_line(main_thread);
        menu.addItem(&first_line);
        menu.addItem(&second_line);
        menu.addItem(&NSMenuItem::separatorItem(main_thread));
        // SAFETY : `terminate:` est une action de `NSApplication`, atteinte par la chaîne des répondeurs.
        let quit = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(main_thread),
                &NSString::from_str("Quitter forti-watch"),
                Some(sel!(terminate:)),
                &NSString::from_str("q"),
            )
        };
        menu.addItem(&quit);
        item.setMenu(Some(&menu));

        Some(Self {
            main_thread,
            item,
            first_line,
            second_line,
        })
    }

    pub fn show(&mut self, view: &View, cut_at_text: &str) {
        if let Some(button) = self.item.button(self.main_thread) {
            button.setTitle(&NSString::from_str(&format!(" {}", label(view))));
        }

        let (first, second) = menu_lines(view, cut_at_text);
        self.first_line.setTitle(&NSString::from_str(&first));
        self.second_line.setTitle(&NSString::from_str(&second));
        self.second_line.setHidden(second.is_empty());
    }
}

/// Ligne du menu qui informe sans rien déclencher.
fn info_line(main_thread: MainThreadMarker) -> Retained<NSMenuItem> {
    let line = NSMenuItem::new(main_thread);
    line.setEnabled(false);
    line
}
