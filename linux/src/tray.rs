//! The tray: a StatusNotifierItem (the KDE/freedesktop protocol COSMIC's
//! status area speaks), built from TrayMenu. Its callbacks run on ksni's own
//! thread; they only hand the event to the GTK main loop, which owns the app.

use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::{CheckmarkItem, StandardItem, SubMenu};
use ksni::MenuItem;

use crate::tray_menu::{Action, Item, Kind, TrayMenu};

/// What the tray tells the main loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Action(Action),
    /// The menu is about to open: a good moment to refresh the due count.
    MenuOpened,
}

pub type Notify = Box<dyn Fn(Event) + Send>;

pub struct Tray {
    pub menu: TrayMenu,
    notify: Notify,
}

/// The icon: the normal one, or another when Anki is not reachable (an SNI
/// icon cannot be greyed out by the item itself).
pub fn icon_name(menu: &TrayMenu) -> &'static str {
    if menu.icon_dimmed { "network-offline-symbolic" } else { "accessories-dictionary-symbolic" }
}

impl ksni::Tray for Tray {
    // A left click opens the menu too.
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        "slipcard".into()
    }

    fn title(&self) -> String {
        "Slipcard".into()
    }

    fn icon_name(&self) -> String {
        icon_name(&self.menu).into()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "Slipcard".into(),
            description: self.menu.state_lines.join("\n"),
            ..Default::default()
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        self.menu.items.iter().map(to_ksni).collect()
    }

    fn menu_about_to_show(&mut self) {
        (self.notify)(Event::MenuOpened);
    }
}

fn to_ksni(item: &Item) -> MenuItem<Tray> {
    let send = |a: Action| -> Box<dyn Fn(&mut Tray) + Send> { Box::new(move |t: &mut Tray| (t.notify)(Event::Action(a))) };
    match &item.kind {
        Kind::Separator => MenuItem::Separator,
        Kind::Info => StandardItem { label: item.title.clone(), enabled: false, ..Default::default() }.into(),
        Kind::Action(a) => StandardItem { label: item.title.clone(), activate: send(*a), ..Default::default() }.into(),
        Kind::Toggle(a, checked) => {
            CheckmarkItem { label: item.title.clone(), checked: *checked, activate: send(*a), ..Default::default() }.into()
        }
        Kind::Submenu(items) => {
            SubMenu { label: item.title.clone(), submenu: items.iter().map(to_ksni).collect(), ..Default::default() }
                .into()
        }
    }
}

/// Puts the tray up; None (with the reason logged) when the desktop has no
/// StatusNotifierItem host or no session bus.
pub fn spawn(menu: TrayMenu, notify: Notify) -> Option<Handle<Tray>> {
    match (Tray { menu, notify }).spawn() {
        Ok(handle) => Some(handle),
        Err(e) => {
            eprintln!("tray: not available ({e}); Enter in the terminal is still \"next word now\"");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anki::{Error, State};
    use crate::tray_menu::TrayState;
    use std::sync::{Arc, Mutex};

    fn tray(state: &TrayState) -> (Tray, Arc<Mutex<Vec<Event>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let log = events.clone();
        let t = Tray { menu: TrayMenu::new(state, |_| 0), notify: Box::new(move |e| log.lock().unwrap().push(e)) };
        (t, events)
    }

    #[test]
    fn the_sni_menu_mirrors_the_model() {
        use ksni::Tray as _;
        let (t, _) = tray(&TrayState { anki: State::Connected, due_count: Some(2), ..TrayState::default() });
        let items = t.menu();
        assert_eq!(items.len(), t.menu.items.len());
        let labels: Vec<String> = items
            .iter()
            .map(|i| match i {
                MenuItem::Standard(s) => s.label.clone(),
                MenuItem::Checkmark(c) => format!("[{}] {}", if c.checked { "x" } else { " " }, c.label),
                MenuItem::SubMenu(s) => format!("{} >", s.label),
                MenuItem::Separator => "-".into(),
                _ => "?".into(),
            })
            .collect();
        assert_eq!(
            labels,
            [
                "Connected to Anki",
                "2 cards due",
                "-",
                "Next word now",
                "Next word in 5 s",
                "Pause for an hour",
                "[ ] Quiet hours 22:00–08:00",
                "Interval >",
                "-",
                "Open settings file",
                "Quit Slipcard",
            ]
        );
        assert!(matches!(&items[0], MenuItem::Standard(s) if !s.enabled));
    }

    #[test]
    fn choosing_an_item_hands_its_action_to_the_main_loop() {
        use ksni::Tray as _;
        let (mut t, events) = tray(&TrayState::default());
        let items = t.menu();
        let MenuItem::Standard(next) = &items[2] else { panic!("Next word now") };
        (next.activate)(&mut t);
        let MenuItem::SubMenu(interval) = &items[6] else { panic!("Interval") };
        let MenuItem::Checkmark(thirty) = &interval.submenu[3] else { panic!("30 min") };
        (thirty.activate)(&mut t);
        t.menu_about_to_show();
        assert_eq!(
            *events.lock().unwrap(),
            [Event::Action(Action::NextWordNow), Event::Action(Action::SetInterval(30)), Event::MenuOpened]
        );
    }

    #[test]
    fn the_icon_changes_when_anki_is_not_reachable() {
        let (ok, _) = tray(&TrayState { anki: State::Connected, ..TrayState::default() });
        let (down, _) = tray(&TrayState { anki: State::Failed(Error::NotReachable("x".into())), ..TrayState::default() });
        assert_eq!(icon_name(&ok.menu), "accessories-dictionary-symbolic");
        assert_eq!(icon_name(&down.menu), "network-offline-symbolic");
    }

    #[test]
    fn the_tooltip_is_the_state() {
        use ksni::Tray as _;
        let (t, _) = tray(&TrayState { anki: State::Connected, due_count: Some(0), ..TrayState::default() });
        assert_eq!(t.tool_tip().description, "Connected to Anki\nNo cards due");
    }
}
