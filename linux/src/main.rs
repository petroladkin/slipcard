//! Slipcard on Linux: a popup every interval with the next due card from
//! Anki, shown as a layer-shell surface on the overlay layer, so a Wayland
//! compositor such as COSMIC draws it centred above every window, full-screen
//! ones included, never tiles it and never gives it keyboard focus.
//!
//!   slipcard                    run: a popup every interval (settings.json)
//!   slipcard --show-after 5     also one word 5 s after the start
//!   slipcard --check            print the OS, desktop and layer-shell support
//!   slipcard --next-card        print the card a popup would show now, and exit
//!   slipcard --tray-action interval:30   run a settings action of the tray, and exit
//!
//! While it runs, the tray (a StatusNotifierItem) has the controls and the
//! state; Enter in the terminal is also "next word now"; q and Enter, or
//! Ctrl-C, quits.

#[allow(dead_code)]
mod anki;
mod args;
mod autostart;
mod card_text;
mod countdown;
mod flow;
#[allow(dead_code)]
mod scheduler;
#[allow(dead_code)]
mod settings;
mod tray;
mod tray_menu;

use std::cell::{Cell, RefCell};
use std::io::BufRead;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gtk::prelude::*;
use gtk::{gio, glib, pango};
use gtk4_layer_shell::{KeyboardMode, Layer, LayerShell};

use flow::{PopupFlow, PopupModel, Step};

const APP_ID: &str = "io.github.petroladkin.Slipcard";
const CSS: &str = "
.slipcard { border-radius: 14px; padding: 24px; }
.slipcard .word { font-size: 28pt; font-weight: 600; }
.slipcard .error { color: #c01c28; }
";

enum Command {
    Show,
    Quit,
    Tray(tray::Event),
}

fn main() -> glib::ExitCode {
    let mode = match args::parse(std::env::args().skip(1)) {
        Ok(mode) => mode,
        Err(message) => {
            eprintln!("{message}");
            return glib::ExitCode::FAILURE;
        }
    };
    match mode {
        args::Mode::Check => check(),
        args::Mode::NextCard => next_card(),
        args::Mode::TrayAction(name) => tray_action(&name),
        args::Mode::Run { show_after } => run(show_after),
    }
}

fn check() -> glib::ExitCode {
    let os = std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("PRETTY_NAME=").map(|v| v.trim_matches('"').to_string()))
        })
        .unwrap_or_else(|| "unknown".into());
    let env = |name: &str| std::env::var(name).unwrap_or_else(|_| "(unset)".into());
    println!("os: {os}");
    println!("XDG_CURRENT_DESKTOP: {}", env("XDG_CURRENT_DESKTOP"));
    println!("XDG_SESSION_TYPE: {}", env("XDG_SESSION_TYPE"));
    println!("WAYLAND_DISPLAY: {}", env("WAYLAND_DISPLAY"));
    // GTK and gtk4-layer-shell answer only once GTK is initialized, which
    // needs a display.
    match gtk::init() {
        Ok(()) => {
            println!(
                "gtk: {}.{}.{}",
                gtk::major_version(),
                gtk::minor_version(),
                gtk::micro_version()
            );
            println!(
                "gtk4-layer-shell library: {}.{}.{}",
                gtk4_layer_shell::major_version(),
                gtk4_layer_shell::minor_version(),
                gtk4_layer_shell::micro_version()
            );
            let display = gtk::gdk::Display::default().map(|d| d.type_().name().to_string());
            println!("gdk display: {}", display.unwrap_or_else(|| "none".into()));
            println!("layer shell supported by the compositor: {}", gtk4_layer_shell::is_supported());
        }
        Err(err) => println!("gtk: could not open a display ({err})"),
    }
    let (path, load) = load_settings();
    println!("settings: {} ({})", path.display(), describe(&load.file));
    for problem in &load.problems {
        println!("settings: {problem}");
    }
    glib::ExitCode::SUCCESS
}

/// Asks AnkiConnect for the card a popup would show now and prints it or the
/// connection state; no window. Exit status: 0 a card is due, 3 none is due,
/// 1 Anki failed.
fn next_card() -> glib::ExitCode {
    let (path, load) = load_settings();
    println!("settings: {} ({})", path.display(), describe(&load.file));
    for problem in &load.problems {
        println!("settings: {problem}");
    }
    let s = &load.settings;
    let key = if s.anki_connect_key.is_some() { "key set" } else { "no key" };
    println!("anki: {} ({key}), search: {}", s.anki_connect_url, s.card_query);
    let tray_line = |anki: anki::State, due: Option<usize>| {
        let state = tray_menu::TrayState {
            anki,
            due_count: due,
            settings_warning: tray_menu::settings_warning(&load.file),
            settings: s.clone(),
            ..tray_menu::TrayState::default()
        };
        let menu = tray_menu::TrayMenu::new(&state, local_utc_offset);
        println!("tray: {} (icon dimmed: {})", menu.state_lines.join(" | "), menu.icon_dimmed);
    };
    match anki::Client::from_settings(s).due_cards(&s.card_query) {
        Ok((Some(card), due)) => {
            println!("state: {}", anki::State::Connected);
            tray_line(anki::State::Connected, Some(due));
            println!(
                "next card: {}, queue {}, due {}, grades {:?}",
                card.id, card.queue, card.due, card.next_reviews
            );
            let front = card.question_html.split("</style>").last().unwrap_or("");
            println!("front (raw): {}", front.chars().take(120).collect::<String>());
            println!("back (raw): {}", card.answer_html.chars().take(120).collect::<String>());
            glib::ExitCode::SUCCESS
        }
        Ok((None, due)) => {
            println!("state: {}", anki::State::Connected);
            tray_line(anki::State::Connected, Some(due));
            println!("next card: none due");
            glib::ExitCode::from(3)
        }
        Err(e) => {
            println!("state: {} ({e:?})", anki::State::Failed(e.clone()));
            tray_line(anki::State::Failed(e), None);
            glib::ExitCode::FAILURE
        }
    }
}

/// Runs a settings action of the tray the way the tray does
/// (tray_menu::perform) and exits; no window, no tray. Exit status: 0 saved,
/// 2 applied but not saved, 1 not a settings action or nothing changed.
fn tray_action(name: &str) -> glib::ExitCode {
    let (path, load) = load_settings();
    println!("settings: {} ({})", path.display(), describe(&load.file));
    let Some(action) = tray_menu::Action::from_command_line(name) else {
        println!("tray: not a settings action: {name} (interval:N, quiet-hours or start-at-login)");
        return glib::ExitCode::FAILURE;
    };
    if action == tray_menu::Action::ToggleStartAtLogin {
        let path = autostart::path(|name| std::env::var(name).ok());
        return match toggle_start_at_login(&path) {
            None => glib::ExitCode::SUCCESS,
            Some(_) => glib::ExitCode::from(2),
        };
    }
    let Some(change) = tray_menu::perform(action, &load.settings, &path) else {
        println!("tray: {name} changes nothing");
        return glib::ExitCode::FAILURE;
    };
    let quiet = if change.settings.quiet_hours_enabled { "on" } else { "off" };
    println!("tray: interval {} min, quiet hours {quiet}", change.settings.interval_minutes);
    match change.save_problem {
        None => {
            println!("tray: saved");
            glib::ExitCode::SUCCESS
        }
        Some(problem) => {
            println!("{problem}");
            glib::ExitCode::from(2)
        }
    }
}

fn load_settings() -> (std::path::PathBuf, settings::Load) {
    let path = settings::default_path(|name| std::env::var(name).ok());
    let load = settings::load(&path);
    (path, load)
}

fn describe(file: &settings::File) -> String {
    match file {
        settings::File::Missing => "no file yet, using the defaults".into(),
        settings::File::Valid => "read".into(),
        settings::File::Invalid(reason) => format!("invalid: {reason}; using the defaults"),
    }
}

/// Logs what reading the settings gave; the tray shows the same state later.
fn log_settings(path: &std::path::Path, load: &settings::Load) {
    match &load.file {
        settings::File::Invalid(_) => eprintln!("settings: {}: {}", path.display(), describe(&load.file)),
        _ => println!("settings: {}: {}", path.display(), describe(&load.file)),
    }
    for problem in &load.problems {
        eprintln!("settings: {problem}");
    }
}


fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// The local UTC offset at a Unix time, in seconds (quiet hours are local).
fn local_utc_offset(t: i64) -> i64 {
    glib::DateTime::from_unix_local(t).map(|d| d.utc_offset().as_seconds()).unwrap_or(0)
}

/// What the running app holds, on the GTK main thread only.
struct App {
    app: gtk::Application,
    flow: RefCell<PopupFlow>,
    client: anki::Client,
    query: String,
    countdown_seconds: u32,
    timer: RefCell<Option<glib::SourceId>>,
    settings: RefCell<settings::Settings>,
    settings_path: std::path::PathBuf,
    settings_warning: Option<String>,
    save_problem: RefCell<Option<String>>,
    due_count: Cell<Option<usize>>,
    tray: RefCell<Option<ksni::blocking::Handle<tray::Tray>>>,
    /// The XDG autostart entry: its presence is "Start at login".
    autostart_path: std::path::PathBuf,
    login_problem: RefCell<Option<String>>,
}

fn run(show_after: Option<u32>) -> glib::ExitCode {
    let (path, load) = load_settings();
    log_settings(&path, &load);
    let warning = tray_menu::settings_warning(&load.file);
    let settings = load.settings;
    let app = gtk::Application::builder().application_id(APP_ID).build();
    app.connect_startup(|_| {
        let css = gtk::CssProvider::new();
        css.load_from_string(CSS);
        gtk::style_context_add_provider_for_display(
            &gtk::gdk::Display::default().expect("a display"),
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });
    app.connect_activate(move |app| {
        // Keep running with no window open: popups come and go.
        std::mem::forget(app.hold());
        if !gtk4_layer_shell::is_supported() {
            eprintln!(
                "warning: the compositor does not support layer shell; \
                 the popup is an ordinary window and may be tiled or take focus"
            );
        }
        let scheduler = scheduler::Scheduler::new(&settings, now_secs(), Box::new(local_utc_offset));
        let state = Rc::new(App {
            app: app.clone(),
            flow: RefCell::new(PopupFlow::new(scheduler)),
            client: anki::Client::from_settings(&settings),
            query: settings.card_query.clone(),
            countdown_seconds: settings.countdown_seconds,
            timer: RefCell::new(None),
            settings: RefCell::new(settings.clone()),
            settings_path: path.clone(),
            settings_warning: warning.clone(),
            save_problem: RefCell::new(None),
            due_count: Cell::new(None),
            tray: RefCell::new(None),
            autostart_path: autostart::path(|name| std::env::var(name).ok()),
            login_problem: RefCell::new(None),
        });
        arm(&state);
        println!(
            "scheduler: every {} min; first popup in {} s",
            settings.interval_minutes,
            state.flow.borrow().next_fire().map_or(0, |f| f - now_secs())
        );

        let (tx, rx) = mpsc::channel();
        let to_main = tx.clone();
        let menu = tray_menu::TrayMenu::new(&tray_state(&state), local_utc_offset);
        *state.tray.borrow_mut() = tray::spawn(menu, Box::new(move |e| {
            let _ = to_main.send(Command::Tray(e));
        }));
        refresh_due_count(&state);
        std::thread::spawn(move || {
            for line in std::io::stdin().lock().lines() {
                let Ok(line) = line else { break };
                let command = if line.trim() == "q" { Command::Quit } else { Command::Show };
                if tx.send(command).is_err() {
                    break;
                }
            }
        });
        let for_commands = state.clone();
        glib::timeout_add_local(Duration::from_millis(100), move || {
            while let Ok(command) = rx.try_recv() {
                match command {
                    Command::Show => next_word_now(&for_commands),
                    Command::Quit => for_commands.app.quit(),
                    Command::Tray(event) => tray_event(&for_commands, event),
                }
            }
            glib::ControlFlow::Continue
        });

        println!("Enter is \"next word now\"; q and Enter, or Ctrl-C, quits.");
        if let Some(seconds) = show_after {
            let for_first = state.clone();
            glib::timeout_add_seconds_local_once(seconds, move || next_word_now(&for_first));
        }
    });
    // Options are parsed above; GApplication gets none of them.
    app.run_with_args(&["slipcard"])
}

/// What the tray shows now.
fn tray_state(state: &App) -> tray_menu::TrayState {
    let mut t = state.flow.borrow().tray_state(
        now_secs(),
        state.due_count.get(),
        &state.settings.borrow(),
        state.settings_warning.clone(),
        state.save_problem.borrow().clone(),
    );
    t.start_at_login = login_item(&state.autostart_path);
    t.login_problem = state.login_problem.borrow().clone();
    t
}

fn login_item(path: &std::path::Path) -> tray_menu::LoginItem {
    if autostart::is_enabled(path) { tray_menu::LoginItem::On } else { tray_menu::LoginItem::Off }
}

/// Turns Start at login on or off for the running binary; the problem for
/// the tray, or None.
fn toggle_start_at_login(path: &std::path::Path) -> Option<String> {
    let exec = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => return Some(format!("Start at login: cannot find this program: {e}")),
    };
    match autostart::toggle(path, &exec) {
        Ok(on) => {
            println!("tray: start at login {} ({})", if on { "on" } else { "off" }, path.display());
            None
        }
        Err(problem) => {
            eprintln!("tray: {problem}");
            Some(problem)
        }
    }
}

/// The tray is rebuilt from the model whenever the state changes.
fn update_tray(state: &Rc<App>) {
    let Some(handle) = state.tray.borrow().clone() else { return };
    let menu = tray_menu::TrayMenu::new(&tray_state(state), local_utc_offset);
    handle.update(move |t| t.menu = menu);
}

/// The due count for the tray (one findCards on a worker thread); it also
/// tells whether Anki answers.
fn refresh_due_count(state: &Rc<App>) {
    let (client, query, state) = (state.client.clone(), state.query.clone(), state.clone());
    glib::spawn_future_local(async move {
        let result = gio::spawn_blocking(move || client.due_count(&query))
            .await
            .unwrap_or_else(|_| Err(anki::Error::BadResponse("the worker thread panicked".into())));
        let before = state.flow.borrow().anki().clone();
        match result {
            Ok(n) => {
                state.due_count.set(Some(n));
                state.flow.borrow_mut().connection_checked(None);
            }
            Err(e) => state.flow.borrow_mut().connection_checked(Some(e)),
        }
        let after = state.flow.borrow().anki().clone();
        if after != before {
            println!("anki: {after}");
        }
        update_tray(&state);
    });
}

fn tray_event(state: &Rc<App>, event: tray::Event) {
    use tray_menu::Action;
    let action = match event {
        tray::Event::MenuOpened => {
            refresh_due_count(state);
            return;
        }
        tray::Event::Action(a) => a,
    };
    match action {
        Action::NextWordNow => next_word_now(state),
        Action::NextWordSoon => {
            let s = state.clone();
            glib::timeout_add_seconds_local_once(5, move || next_word_now(&s));
        }
        Action::Pause => {
            state.flow.borrow_mut().pause_for_an_hour(now_secs());
            println!("tray: paused for an hour");
            arm(state);
        }
        Action::Resume => {
            state.flow.borrow_mut().resume(now_secs());
            println!("tray: resumed");
            arm(state);
        }
        Action::ToggleQuietHours | Action::SetInterval(_) => {
            let change = tray_menu::perform(action, &state.settings.borrow(), &state.settings_path);
            if let Some(c) = change {
                state.flow.borrow_mut().apply(&c.settings, now_secs());
                let quiet = if c.settings.quiet_hours_enabled { "on" } else { "off" };
                let saved = c.save_problem.clone().unwrap_or_else(|| "saved".into());
                println!("tray: interval {} min, quiet hours {quiet}; {saved}", c.settings.interval_minutes);
                *state.save_problem.borrow_mut() = c.save_problem;
                *state.settings.borrow_mut() = c.settings;
                arm(state);
            }
        }
        Action::OpenSettings => open_settings_file(state),
        Action::ToggleStartAtLogin => {
            *state.login_problem.borrow_mut() = toggle_start_at_login(&state.autostart_path);
        }
        Action::Quit => state.app.quit(),
    }
    update_tray(state);
}

/// Opens the file in the default editor; a missing file is created first
/// with the settings in effect.
fn open_settings_file(state: &App) {
    let path = &state.settings_path;
    if !path.exists() {
        if let Err(e) = settings::save(&state.settings.borrow(), path) {
            eprintln!("settings: cannot create {}: {e}", path.display());
        }
    }
    let uri = gio::File::for_path(path).uri();
    if let Err(e) = gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>) {
        eprintln!("settings: cannot open {}: {e}", path.display());
    }
}

fn next_word_now(state: &Rc<App>) {
    let step = state.flow.borrow_mut().next_word_now(now_secs());
    perform(state, step);
}

/// One timer, at the scheduler's next fire time (a second late on purpose,
/// so it never fires before it); none while a popup is up or a card is being
/// fetched. After a suspend the monotonic timer fires late in wall time, and
/// the scheduler's lateness rule turns that into one more interval.
fn arm(state: &Rc<App>) {
    if let Some(id) = state.timer.borrow_mut().take() {
        id.remove();
    }
    let Some(fire) = state.flow.borrow().next_fire() else { return };
    let delay = Duration::from_secs((fire - now_secs()).max(0) as u64 + 1);
    let for_timer = state.clone();
    let id = glib::timeout_add_local_once(delay, move || {
        // A one-shot source is gone once it fired: forget it, do not remove it.
        for_timer.timer.borrow_mut().take();
        let step = for_timer.flow.borrow_mut().timer_fired(now_secs());
        perform(&for_timer, step);
    });
    *state.timer.borrow_mut() = Some(id);
}

fn perform(state: &Rc<App>, step: Step) {
    match step {
        Step::Wait => arm(state),
        Step::Fetch => fetch(state),
        Step::Present(card) => present(state, card),
    }
}

/// Asks AnkiConnect on a worker thread; the result comes back to the main loop.
fn fetch(state: &Rc<App>) {
    if let Some(id) = state.timer.borrow_mut().take() {
        id.remove();
    }
    let client = state.client.clone();
    let query = state.query.clone();
    let state = state.clone();
    glib::spawn_future_local(async move {
        let result = gio::spawn_blocking(move || client.due_cards(&query))
            .await
            .unwrap_or_else(|_| Err(anki::Error::BadResponse("the worker thread panicked".into())));
        let result = result.map(|(next, count)| {
            state.due_count.set(Some(count));
            next
        });
        let none_due = matches!(result, Ok(None));
        let before = state.flow.borrow().anki().clone();
        let step = state.flow.borrow_mut().fetched(result, now_secs());
        let after = state.flow.borrow().anki().clone();
        if after != before {
            println!("anki: {after}");
        }
        if none_due {
            println!("popup: no card due for {}", state.query);
        }
        update_tray(&state);
        perform(&state, step);
    });
}

/// The popup's widgets, so a stage change can redraw them.
struct Widgets {
    window: gtk::Window,
    answer: gtk::Label,
    error: gtk::Label,
    show: gtk::Button,
    close: gtk::Button,
    grades: gtk::Box,
    grade_buttons: Vec<gtk::Button>,
}

fn render(w: &Widgets, m: &PopupModel) {
    w.show.set_visible(!m.shows_answer());
    w.answer.set_visible(m.shows_answer());
    w.grades.set_visible(m.shows_grades());
    for b in &w.grade_buttons {
        b.set_sensitive(!m.is_sending());
    }
    w.error.set_text(m.failure().unwrap_or(""));
    w.error.set_visible(m.failure().is_some());
    w.close.set_label(&m.close_label(now_ms()));
}

fn label(text: &str, class: Option<&str>, lines: i32) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    if let Some(class) = class {
        l.add_css_class(class);
    }
    l.set_wrap(true);
    l.set_wrap_mode(pango::WrapMode::WordChar);
    l.set_max_width_chars(36);
    l.set_lines(lines);
    l.set_ellipsize(pango::EllipsizeMode::End);
    l.set_justify(gtk::Justification::Center);
    l
}

fn present(state: &Rc<App>, card: anki::Card) {
    let window = gtk::Window::builder().application(&state.app).title("Slipcard").build();
    // Add to the window's CSS classes, do not replace them: the default
    // "background" class is what gives the popup the theme's background.
    window.add_css_class("slipcard");
    window.set_size_request(420, 220);

    if gtk4_layer_shell::is_supported() {
        // A layer surface with no anchors is centred by the compositor; no
        // output set lets the compositor pick it (COSMIC: the focused one).
        // KeyboardMode::None: the surface never receives keyboard focus, so
        // typing keeps going into the app below. Pointer input still works.
        window.init_layer_shell();
        window.set_layer(Layer::Overlay);
        window.set_namespace(Some("slipcard"));
        window.set_keyboard_mode(KeyboardMode::None);
    }

    let id = card.id;
    let model = Rc::new(RefCell::new(PopupModel::new(card, state.countdown_seconds, now_ms())));
    let word = label(&model.borrow().front, Some("word"), 4);
    let answer = label(&model.borrow().back, None, 12);
    let error = label("", Some("error"), 3);
    let show = gtk::Button::with_label("Show");
    let close = gtk::Button::with_label("Close");
    let grades = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let grade_buttons: Vec<gtk::Button> = ["Again", "Hard", "Good", "Easy"]
        .into_iter()
        .map(|t| {
            let b = gtk::Button::with_label(t);
            grades.append(&b);
            b
        })
        .collect();
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    buttons.set_halign(gtk::Align::Center);
    buttons.append(&show);
    buttons.append(&grades);
    buttons.append(&close);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 14);
    content.set_valign(gtk::Align::Center);
    content.append(&word);
    content.append(&answer);
    content.append(&error);
    content.append(&buttons);
    window.set_child(Some(&content));

    let w = Rc::new(Widgets {
        window: window.clone(),
        answer,
        error,
        show: show.clone(),
        close: close.clone(),
        grades,
        grade_buttons,
    });
    render(&w, &model.borrow());

    // The countdown: one tick a second; at zero the popup closes without an
    // answer. Stops ticking once the user reacted or the popup is gone.
    {
        let (m, w) = (model.clone(), w.clone());
        glib::timeout_add_local(Duration::from_secs(1), move || {
            if m.borrow_mut().tick(now_ms()) {
                println!("popup: card {id} closed by the countdown, not answered");
                w.window.close();
                return glib::ControlFlow::Break;
            }
            render(&w, &m.borrow());
            let done = m.borrow().countdown().is_stopped() || m.borrow().stage() == &flow::Stage::Closed;
            if done { glib::ControlFlow::Break } else { glib::ControlFlow::Continue }
        });
    }
    // The pointer moving over the popup is a reaction: it stops the countdown.
    // Only motion counts; a pointer resting where the popup appears does not.
    {
        let (m, w) = (model.clone(), w.clone());
        let motion = gtk::EventControllerMotion::new();
        motion.connect_motion(move |_, _, _| {
            if m.borrow_mut().pointer_moved() {
                render(&w, &m.borrow());
            }
        });
        window.add_controller(motion);
    }

    {
        let (m, w) = (model.clone(), w.clone());
        show.connect_clicked(move |_| {
            m.borrow_mut().reveal();
            render(&w, &m.borrow());
        });
    }
    for (button, ease) in w.grade_buttons.iter().zip([anki::Ease::Again, anki::Ease::Hard, anki::Ease::Good, anki::Ease::Easy]) {
        let (m, w, client) = (model.clone(), w.clone(), state.client.clone());
        button.connect_clicked(move |_| {
            let Some((card_id, ease)) = m.borrow_mut().begin_grade(ease) else { return };
            render(&w, &m.borrow());
            let (m, w, client) = (m.clone(), w.clone(), client.clone());
            glib::spawn_future_local(async move {
                let result = gio::spawn_blocking(move || client.answer(card_id, ease))
                    .await
                    .unwrap_or_else(|_| Err(anki::Error::BadResponse("the worker thread panicked".into())));
                if let Err(e) = &result {
                    eprintln!("popup: card {card_id} not answered: {e:?}");
                }
                if m.borrow_mut().finish_grade(result) {
                    println!("popup: card {card_id} answered {ease:?}");
                    w.window.close();
                } else {
                    render(&w, &m.borrow());
                }
            });
        });
    }
    {
        let (m, w) = (model.clone(), w.clone());
        close.connect_clicked(move |_| {
            m.borrow_mut().close();
            w.window.close();
        });
    }
    // However the window goes, the scheduler hears it once.
    let closed = Rc::new(Cell::new(false));
    let for_close = state.clone();
    window.connect_close_request(move |_| {
        if !closed.replace(true) {
            for_close.flow.borrow_mut().popup_closed(now_secs());
            println!(
                "popup: closed; next in {} s",
                for_close.flow.borrow().next_fire().map_or(0, |f| f - now_secs())
            );
            arm(&for_close);
            refresh_due_count(&for_close);
        }
        glib::Propagation::Proceed
    });

    window.present();
    println!("popup: card {id} shown");
}
