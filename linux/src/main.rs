//! Proof of concept of the popup on Linux: a GTK 4 window shown as a
//! layer-shell surface on the overlay layer, so a Wayland compositor such as
//! COSMIC draws it centred above every window, full-screen ones included,
//! never tiles it and never gives it keyboard focus.
//!
//!   slipcard                    show a popup now
//!   slipcard --show-after 5     show the first popup after 5 s
//!   slipcard --check            print the OS, desktop and layer-shell support
//!   slipcard --next-card        print the card a popup would show now, and exit
//!
//! While it runs, Enter in the terminal shows another popup; q and Enter, or
//! Ctrl-C, quits.

#[allow(dead_code)]
mod anki;
mod args;
// The popup and the tray (next steps) drive it.
#[allow(dead_code)]
mod scheduler;
// Read now; the scheduler and the tray (next steps) use the rest of it.
#[allow(dead_code)]
mod settings;

use std::io::BufRead;
use std::sync::mpsc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use gtk4_layer_shell::{KeyboardMode, Layer, LayerShell};

const APP_ID: &str = "io.github.petroladkin.Slipcard";
const CSS: &str = "
.slipcard { border-radius: 14px; padding: 24px; }
.slipcard .word { font-size: 28pt; font-weight: 600; }
";

enum Command {
    Show,
    Quit,
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
    match anki::Client::from_settings(s).next_due_card(&s.card_query) {
        Ok(Some(card)) => {
            println!("state: {}", anki::State::Connected);
            println!(
                "next card: {}, queue {}, due {}, grades {:?}",
                card.id, card.queue, card.due, card.next_reviews
            );
            let front = card.question_html.split("</style>").last().unwrap_or("");
            println!("front (raw): {}", front.chars().take(120).collect::<String>());
            println!("back (raw): {}", card.answer_html.chars().take(120).collect::<String>());
            glib::ExitCode::SUCCESS
        }
        Ok(None) => {
            println!("state: {}", anki::State::Connected);
            println!("next card: none due");
            glib::ExitCode::from(3)
        }
        Err(e) => {
            println!("state: {} ({e:?})", anki::State::Failed(e.clone()));
            glib::ExitCode::FAILURE
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

fn run(show_after: u32) -> glib::ExitCode {
    let (path, load) = load_settings();
    log_settings(&path, &load);
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

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in std::io::stdin().lock().lines() {
                let Ok(line) = line else { break };
                let command = if line.trim() == "q" { Command::Quit } else { Command::Show };
                if tx.send(command).is_err() {
                    break;
                }
            }
        });
        let app_for_commands = app.clone();
        glib::timeout_add_local(Duration::from_millis(100), move || {
            match rx.try_recv() {
                Ok(Command::Show) => show_popup(&app_for_commands),
                Ok(Command::Quit) => app_for_commands.quit(),
                Err(_) => {}
            }
            glib::ControlFlow::Continue
        });

        println!("Enter shows another popup; q and Enter, or Ctrl-C, quits.");
        let app_for_first = app.clone();
        glib::timeout_add_seconds_local_once(show_after, move || show_popup(&app_for_first));
    });
    // Options are parsed above; GApplication gets none of them.
    app.run_with_args(&["slipcard"])
}

fn show_popup(app: &gtk::Application) {
    let window = gtk::Window::builder()
        .application(app)
        .title("Slipcard")
        .build();
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

    let word = gtk::Label::new(Some("serendipity"));
    word.add_css_class("word");
    let answer = gtk::Label::new(Some(
        "a happy accident\n\u{201c}Finding this caf\u{e9} was pure serendipity.\u{201d}",
    ));
    answer.set_justify(gtk::Justification::Center);
    answer.set_visible(false);

    let show = gtk::Button::with_label("Show");
    let close = gtk::Button::with_label("Close");
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    buttons.set_halign(gtk::Align::Center);
    buttons.append(&show);
    buttons.append(&close);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 14);
    content.set_valign(gtk::Align::Center);
    content.append(&word);
    content.append(&answer);
    content.append(&buttons);
    window.set_child(Some(&content));

    show.connect_clicked(move |show| {
        answer.set_visible(true);
        show.set_visible(false);
        println!("popup: Show clicked");
    });
    let window_for_close = window.clone();
    close.connect_clicked(move |_| {
        println!("popup: Close clicked");
        window_for_close.close();
    });

    window.present();
    println!("popup: shown");
}
