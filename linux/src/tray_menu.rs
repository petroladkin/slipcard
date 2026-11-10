//! The tray menu as plain data: the StatusNotifierItem tray is built from
//! it, and the tests read it. Same rules as TrayMenu.swift.

use std::path::Path;

use crate::anki::State;
use crate::settings::{self, SaveError, Settings, INTERVAL_RANGE};

/// What the tray menu can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    NextWordNow,
    /// Leaves time to switch to another app and start typing.
    NextWordSoon,
    Pause,
    Resume,
    ToggleQuietHours,
    SetInterval(u32),
    OpenSettings,
    /// Starts Slipcard when the user logs in, or stops doing so.
    ToggleStartAtLogin,
    Quit,
}

impl Action {
    /// The actions by a name for the command line (`--tray-action
    /// interval:30`, `quiet-hours`, `start-at-login`).
    pub fn from_command_line(text: &str) -> Option<Action> {
        if text == "quiet-hours" {
            return Some(Action::ToggleQuietHours);
        }
        if text == "start-at-login" {
            return Some(Action::ToggleStartAtLogin);
        }
        text.strip_prefix("interval:").and_then(|n| n.parse().ok()).map(Action::SetInterval)
    }

    /// The settings after this action, or None when it does not change them
    /// (or would make them invalid).
    pub fn applied(self, settings: &Settings) -> Option<Settings> {
        let mut s = settings.clone();
        match self {
            Action::ToggleQuietHours => s.quiet_hours_enabled = !s.quiet_hours_enabled,
            Action::SetInterval(m) if INTERVAL_RANGE.contains(&m) && m != s.interval_minutes => {
                s.interval_minutes = m
            }
            _ => return None,
        }
        Some(s)
    }
}

/// A settings change from the tray: applied, then written to the file. The
/// tray and `slipcard --tray-action` both go through here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// The settings now in effect (in memory even if not written).
    pub settings: Settings,
    /// Why the file was not written, for the tray; None when it was.
    pub save_problem: Option<String>,
}

/// None when `action` does not change the settings.
pub fn perform(action: Action, on: &Settings, file: &Path) -> Option<Change> {
    let changed = action.applied(on)?;
    let save_problem = match settings::save(&changed, file) {
        Ok(()) => None,
        Err(SaveError::FileIsInvalid(_)) => Some("Not saved: the settings file is invalid".to_string()),
        Err(SaveError::CannotWrite(_)) => Some("Not saved: the settings file cannot be written".to_string()),
    };
    Some(Change { settings: changed, save_problem })
}

/// Whether Slipcard starts at login, as the system reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginItem {
    /// Not possible from this copy.
    Unavailable,
    Off,
    On,
    /// Registered, but the user has to allow it (macOS); kept for the same
    /// model as TrayMenu.swift, never set on Linux.
    #[allow(dead_code)]
    NeedsApproval,
}

/// What the tray shows; the app fills it from the scheduler, the last fetch
/// and the settings. Times are Unix seconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayState {
    pub anki: State,
    /// Cards matching the search, as of the last check; None when unknown.
    pub due_count: Option<usize>,
    /// Set while a pause runs.
    pub paused_until: Option<i64>,
    /// Set while quiet hours run: when they end.
    pub quiet_until: Option<i64>,
    /// The settings file cannot be used (its menu warning).
    pub settings_warning: Option<String>,
    /// The last change from the tray could not be written.
    pub save_problem: Option<String>,
    pub settings: Settings,
    pub start_at_login: LoginItem,
    /// The last Start at login change failed.
    pub login_problem: Option<String>,
}

impl Default for TrayState {
    fn default() -> Self {
        TrayState {
            anki: State::Unknown,
            due_count: None,
            paused_until: None,
            quiet_until: None,
            settings_warning: None,
            save_problem: None,
            settings: Settings::default(),
            start_at_login: LoginItem::Unavailable,
            login_problem: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// A line of state, not clickable.
    Info,
    Action(Action),
    /// A switch; `checked` shows its state.
    Toggle(Action, bool),
    Submenu(Vec<Item>),
    Separator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub title: String,
    pub kind: Kind,
}

impl Item {
    fn new(title: impl Into<String>, kind: Kind) -> Item {
        Item { title: title.into(), kind }
    }

    fn separator() -> Item {
        Item::new("", Kind::Separator)
    }
}

pub const INTERVAL_CHOICES: [u32; 5] = [5, 10, 15, 30, 60];

/// The message the settings file's state gives the tray.
pub fn settings_warning(file: &settings::File) -> Option<String> {
    matches!(file, settings::File::Invalid(_)).then(|| "Settings file is invalid: using the defaults".to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayMenu {
    pub items: Vec<Item>,
    /// The icon changes when Anki is not reachable.
    pub icon_dimmed: bool,
    /// The state lines alone, also used as the tooltip.
    pub state_lines: Vec<String>,
}

impl TrayMenu {
    pub fn new(state: &TrayState, utc_offset: impl Fn(i64) -> i64) -> TrayMenu {
        let clock = |t: i64| {
            let minute = (t + utc_offset(t)).rem_euclid(86400) / 60;
            format!("{:02}:{:02}", minute / 60, minute % 60)
        };
        let mut lines = Vec::new();
        lines.extend(state.settings_warning.clone());
        lines.extend(state.save_problem.clone());
        lines.extend(state.login_problem.clone());
        lines.push(state.anki.to_string());
        if let (State::Connected, Some(n)) = (&state.anki, state.due_count) {
            lines.push(match n {
                0 => "No cards due".to_string(),
                1 => "1 card due".to_string(),
                n => format!("{n} cards due"),
            });
        }
        if let Some(t) = state.paused_until {
            lines.push(format!("Paused until {}", clock(t)));
        }
        if let Some(t) = state.quiet_until {
            lines.push(format!("Quiet hours until {}", clock(t)));
        }

        let s = &state.settings;
        let mut minutes = INTERVAL_CHOICES.to_vec();
        if !minutes.contains(&s.interval_minutes) {
            minutes.push(s.interval_minutes);
            minutes.sort_unstable();
        }
        let interval = minutes
            .iter()
            .map(|&m| Item::new(format!("{m} min"), Kind::Toggle(Action::SetInterval(m), m == s.interval_minutes)))
            .collect();
        let mut items: Vec<Item> = lines.iter().map(|l| Item::new(l.clone(), Kind::Info)).collect();
        items.extend([
            Item::separator(),
            Item::new("Next word now", Kind::Action(Action::NextWordNow)),
            Item::new("Next word in 5 s", Kind::Action(Action::NextWordSoon)),
            if state.paused_until.is_none() {
                Item::new("Pause for an hour", Kind::Action(Action::Pause))
            } else {
                Item::new("Resume", Kind::Action(Action::Resume))
            },
            Item::new(
                format!("Quiet hours {}–{}", s.quiet_hours_start, s.quiet_hours_end),
                Kind::Toggle(Action::ToggleQuietHours, s.quiet_hours_enabled),
            ),
            Item::new("Interval", Kind::Submenu(interval)),
            Item::separator(),
            Item::new("Open settings file", Kind::Action(Action::OpenSettings)),
            start_at_login_item(state.start_at_login),
            Item::new("Quit Slipcard", Kind::Action(Action::Quit)),
        ]);
        TrayMenu { items, icon_dimmed: matches!(state.anki, State::Failed(_)), state_lines: lines }
    }
}

fn start_at_login_item(state: LoginItem) -> Item {
    match state {
        LoginItem::Unavailable => Item::new("Start at login: only from the installed app", Kind::Info),
        LoginItem::Off => Item::new("Start at login", Kind::Toggle(Action::ToggleStartAtLogin, false)),
        LoginItem::On => Item::new("Start at login", Kind::Toggle(Action::ToggleStartAtLogin, true)),
        LoginItem::NeedsApproval => {
            Item::new("Start at login: allow it in System Settings", Kind::Toggle(Action::ToggleStartAtLogin, true))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anki::Error;
    use crate::settings::ClockTime;

    /// 2026-11-10 10:00 UTC (12:00 at UTC+2).
    const TEN: i64 = 1_794_304_800;

    fn menu(state: &TrayState) -> TrayMenu {
        TrayMenu::new(state, |_| 7200)
    }

    fn item<'a>(m: &'a TrayMenu, title: &str) -> Option<&'a Item> {
        m.items.iter().find(|i| i.title == title)
    }

    fn interval(m: &TrayMenu) -> Vec<Item> {
        match item(m, "Interval").map(|i| &i.kind) {
            Some(Kind::Submenu(items)) => items.clone(),
            _ => vec![],
        }
    }

    fn connected(due: Option<usize>) -> TrayState {
        TrayState { anki: State::Connected, due_count: due, ..TrayState::default() }
    }

    // controls (AC-1)

    #[test]
    fn menu_offers_every_control() {
        let m = menu(&connected(Some(3)));
        for title in [
            "Next word now",
            "Next word in 5 s",
            "Pause for an hour",
            "Quiet hours 22:00–08:00",
            "Interval",
            "Open settings file",
            "Quit Slipcard",
        ] {
            assert!(item(&m, title).is_some(), "{title}");
        }
        assert_eq!(item(&m, "Next word now").unwrap().kind, Kind::Action(Action::NextWordNow));
        assert_eq!(item(&m, "Open settings file").unwrap().kind, Kind::Action(Action::OpenSettings));
        assert_eq!(item(&m, "Quit Slipcard").unwrap().kind, Kind::Action(Action::Quit));
    }

    // start at login

    #[test]
    fn start_at_login_is_a_switch_showing_its_state() {
        let off = menu(&TrayState { start_at_login: LoginItem::Off, ..TrayState::default() });
        let on = menu(&TrayState { start_at_login: LoginItem::On, ..TrayState::default() });
        assert_eq!(item(&off, "Start at login").unwrap().kind, Kind::Toggle(Action::ToggleStartAtLogin, false));
        assert_eq!(item(&on, "Start at login").unwrap().kind, Kind::Toggle(Action::ToggleStartAtLogin, true));
        // It sits between Open settings file and Quit.
        let titles: Vec<&str> = on.items.iter().map(|i| i.title.as_str()).collect();
        let i = titles.iter().position(|t| *t == "Start at login").unwrap();
        assert_eq!((titles[i - 1], titles[i + 1]), ("Open settings file", "Quit Slipcard"));
    }

    #[test]
    fn start_at_login_needing_approval_says_so() {
        let m = menu(&TrayState { start_at_login: LoginItem::NeedsApproval, ..TrayState::default() });
        assert_eq!(
            item(&m, "Start at login: allow it in System Settings").unwrap().kind,
            Kind::Toggle(Action::ToggleStartAtLogin, true)
        );
    }

    #[test]
    fn start_at_login_outside_the_installed_app_is_only_a_line() {
        let m = menu(&TrayState::default());
        assert_eq!(item(&m, "Start at login: only from the installed app").unwrap().kind, Kind::Info);
        assert!(item(&m, "Start at login").is_none());
    }

    #[test]
    fn a_login_problem_is_a_state_line() {
        let m = menu(&TrayState { anki: State::Connected, login_problem: Some("Start at login: no".into()), ..TrayState::default() });
        assert_eq!(m.state_lines.first().map(String::as_str), Some("Start at login: no"));
        assert_eq!(item(&m, "Start at login: no").unwrap().kind, Kind::Info);
    }

    #[test]
    fn start_at_login_is_not_a_settings_change() {
        assert_eq!(Action::from_command_line("start-at-login"), Some(Action::ToggleStartAtLogin));
        assert_eq!(Action::ToggleStartAtLogin.applied(&Settings::default()), None);
    }

    #[test]
    fn pause_becomes_resume_while_paused() {
        let paused = menu(&TrayState { paused_until: Some(TEN), ..TrayState::default() });
        assert_eq!(item(&paused, "Resume").unwrap().kind, Kind::Action(Action::Resume));
        assert!(item(&paused, "Pause for an hour").is_none());
        assert_eq!(item(&menu(&TrayState::default()), "Pause for an hour").unwrap().kind, Kind::Action(Action::Pause));
    }

    #[test]
    fn interval_choices_are_the_agreed_ones_with_the_current_checked() {
        let items = interval(&menu(&TrayState::default()));
        let titles: Vec<&str> = items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, ["5 min", "10 min", "15 min", "30 min", "60 min"]);
        let checked: Vec<&str> =
            items.iter().filter(|i| matches!(i.kind, Kind::Toggle(_, true))).map(|i| i.title.as_str()).collect();
        assert_eq!(checked, ["15 min"]);
        assert_eq!(items[3].kind, Kind::Toggle(Action::SetInterval(30), false));
    }

    #[test]
    fn a_custom_interval_from_the_file_is_listed_and_checked() {
        let state =
            TrayState { settings: Settings { interval_minutes: 20, ..Settings::default() }, ..TrayState::default() };
        let items = interval(&menu(&state));
        let titles: Vec<&str> = items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, ["5 min", "10 min", "15 min", "20 min", "30 min", "60 min"]);
        assert_eq!(items[3].kind, Kind::Toggle(Action::SetInterval(20), true));
    }

    #[test]
    fn quiet_hours_switch_shows_its_state_and_range() {
        assert_eq!(
            item(&menu(&TrayState::default()), "Quiet hours 22:00–08:00").unwrap().kind,
            Kind::Toggle(Action::ToggleQuietHours, false)
        );
        let s = Settings {
            quiet_hours_enabled: true,
            quiet_hours_start: ClockTime::new(23, 30).unwrap(),
            quiet_hours_end: ClockTime::new(7, 0).unwrap(),
            ..Settings::default()
        };
        let m = menu(&TrayState { settings: s, ..TrayState::default() });
        assert_eq!(item(&m, "Quiet hours 23:30–07:00").unwrap().kind, Kind::Toggle(Action::ToggleQuietHours, true));
    }

    // state

    #[test]
    fn connected_shows_the_due_count() {
        assert_eq!(menu(&connected(Some(0))).state_lines, ["Connected to Anki", "No cards due"]);
        assert_eq!(menu(&connected(Some(1))).state_lines, ["Connected to Anki", "1 card due"]);
        assert_eq!(menu(&connected(Some(12))).state_lines, ["Connected to Anki", "12 cards due"]);
    }

    #[test]
    fn an_unknown_count_shows_no_count_line() {
        assert_eq!(menu(&connected(None)).state_lines, ["Connected to Anki"]);
        assert_eq!(menu(&TrayState::default()).state_lines, ["Not connected yet"]);
    }

    #[test]
    fn anki_not_reachable_shows_the_state_and_dims_the_icon() {
        let m = menu(&TrayState {
            anki: State::Failed(Error::NotReachable("x".into())),
            due_count: Some(5),
            ..TrayState::default()
        });
        assert_eq!(m.state_lines, ["Anki is not running or not reachable"]);
        assert!(m.icon_dimmed);
        assert!(menu(&TrayState { anki: State::Failed(Error::WrongKey), ..TrayState::default() }).icon_dimmed);
        assert!(menu(&TrayState { anki: State::Failed(Error::TimedOut), ..TrayState::default() }).icon_dimmed);
    }

    #[test]
    fn connected_or_not_yet_checked_keeps_the_icon() {
        assert!(!menu(&connected(Some(0))).icon_dimmed);
        assert!(!menu(&TrayState::default()).icon_dimmed);
    }

    #[test]
    fn a_pause_shows_until_when_in_local_time() {
        let m = menu(&TrayState { paused_until: Some(TEN), ..connected(Some(2)) });
        assert_eq!(m.state_lines, ["Connected to Anki", "2 cards due", "Paused until 12:00"]);
    }

    #[test]
    fn quiet_hours_show_until_when_in_local_time() {
        let m = menu(&TrayState { quiet_until: Some(TEN - 4 * 3600), ..connected(None) });
        assert_eq!(m.state_lines, ["Connected to Anki", "Quiet hours until 08:00"]);
    }

    #[test]
    fn an_invalid_settings_file_is_the_first_line() {
        let warning = settings_warning(&settings::parse(b"oops").file);
        let m = menu(&TrayState { settings_warning: warning, ..connected(Some(1)) });
        assert_eq!(m.state_lines[0], "Settings file is invalid: using the defaults");
        assert_eq!(m.items[0].kind, Kind::Info);
        assert_eq!(settings_warning(&settings::File::Valid), None);
        assert_eq!(settings_warning(&settings::File::Missing), None);
    }

    #[test]
    fn a_change_that_could_not_be_saved_is_shown() {
        let m = menu(&TrayState {
            save_problem: Some("Not saved: the settings file is invalid".into()),
            ..TrayState::default()
        });
        assert_eq!(m.state_lines[0], "Not saved: the settings file is invalid");
    }

    #[test]
    fn state_lines_come_first_as_info_items() {
        let m = menu(&TrayState { paused_until: Some(TEN), ..connected(Some(4)) });
        let infos: Vec<String> = m.items.iter().take_while(|i| i.kind == Kind::Info).map(|i| i.title.clone()).collect();
        assert_eq!(infos, m.state_lines);
        assert_eq!(m.items[infos.len()].kind, Kind::Separator);
    }

    // actions and settings (AC-3)

    #[test]
    fn tray_settings_actions_change_only_their_setting() {
        let s = Settings::default();
        assert!(Action::ToggleQuietHours.applied(&s).unwrap().quiet_hours_enabled);
        assert_eq!(Action::SetInterval(30).applied(&s), Some(Settings { interval_minutes: 30, ..Settings::default() }));
        assert_eq!(Action::SetInterval(15).applied(&s), None); // already 15
        assert_eq!(Action::SetInterval(0).applied(&s), None);
        for a in [Action::NextWordNow, Action::NextWordSoon, Action::Pause, Action::Resume, Action::OpenSettings, Action::Quit] {
            assert_eq!(a.applied(&s), None, "{a:?}");
        }
    }

    #[test]
    fn command_line_names_the_settings_actions() {
        assert_eq!(Action::from_command_line("interval:30"), Some(Action::SetInterval(30)));
        assert_eq!(Action::from_command_line("quiet-hours"), Some(Action::ToggleQuietHours));
        assert_eq!(Action::from_command_line("pause"), None);
        assert_eq!(Action::from_command_line("interval:soon"), None);
    }

    fn temp_file(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("slipcard-tray-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("settings.json")
    }

    #[test]
    fn a_saved_change_survives_a_restart_and_keeps_unknown_keys() {
        let file = temp_file("restart");
        std::fs::write(&file, br#"{"cardQuery": "deck:English is:due", "mine": [1]}"#).unwrap();
        let mut s = settings::load(&file).settings;
        for a in [Action::SetInterval(30), Action::ToggleQuietHours] {
            let change = perform(a, &s, &file).unwrap();
            assert_eq!(change.save_problem, None);
            s = change.settings;
        }
        let restarted = settings::load(&file);
        assert_eq!(restarted.settings.interval_minutes, 30);
        assert!(restarted.settings.quiet_hours_enabled);
        assert_eq!(restarted.settings.card_query, "deck:English is:due");
        let raw: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(raw["mine"], serde_json::json!([1]));
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }

    #[test]
    fn a_change_to_an_invalid_file_applies_but_is_not_saved() {
        let file = temp_file("invalid");
        let broken = br#"{"intervalMinutes": 10,, oops"#;
        std::fs::write(&file, broken).unwrap();
        let change = perform(Action::SetInterval(30), &Settings::default(), &file).unwrap();
        assert_eq!(change.settings.interval_minutes, 30);
        assert_eq!(change.save_problem.as_deref(), Some("Not saved: the settings file is invalid"));
        assert_eq!(std::fs::read(&file).unwrap(), broken);
        assert_eq!(perform(Action::Pause, &Settings::default(), &file), None);
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }
}
