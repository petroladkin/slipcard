//! "Start at login" on Linux: an XDG autostart entry,
//! `$XDG_CONFIG_HOME/autostart/slipcard.desktop` (or `~/.config/autostart`).
//! Its presence is the state, so it survives a restart by itself.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Where the autostart entry lives, from the environment.
pub fn path(env: impl Fn(&str) -> Option<String>) -> PathBuf {
    let config = env("XDG_CONFIG_HOME")
        .filter(|p| Path::new(p).is_absolute())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env("HOME").unwrap_or_default()).join(".config"));
    config.join("autostart").join("slipcard.desktop")
}

/// A desktop entry's `Exec` value: quoted when the path needs it.
fn exec_value(exec: &Path) -> String {
    let s = exec.to_string_lossy();
    if s.chars().any(|c| c.is_whitespace() || "\"'\\$`".contains(c)) {
        let escaped: String = s
            .chars()
            .flat_map(|c| if "\"`$\\".contains(c) { vec!['\\', c] } else { vec![c] })
            .collect();
        format!("\"{escaped}\"")
    } else {
        s.into_owned()
    }
}

/// The autostart entry for the binary at `exec`.
pub fn entry(exec: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Slipcard\nComment=Anki vocabulary cards in a timed popup\n\
         Exec={}\nIcon=accessories-dictionary\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        exec_value(exec)
    )
}

pub fn is_enabled(path: &Path) -> bool {
    path.is_file()
}

/// Writes the entry atomically (a temporary file and a rename).
pub fn enable(path: &Path, exec: &Path) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let tmp = dir.join(".slipcard.desktop.tmp");
    let mut f = fs::File::create(&tmp)?;
    f.write_all(entry(exec).as_bytes())?;
    f.sync_all()?;
    fs::rename(&tmp, path)
}

pub fn disable(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// Turns it on when off and off when on; the new state, or the problem for
/// the tray.
pub fn toggle(path: &Path, exec: &Path) -> Result<bool, String> {
    if is_enabled(path) {
        disable(path).map(|()| false).map_err(|e| format!("Start at login: cannot remove the autostart entry: {e}"))
    } else {
        enable(path, exec).map(|()| true).map_err(|e| format!("Start at login: cannot write the autostart entry: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("slipcard-autostart-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
    }

    #[test]
    fn autostart_path_follows_xdg_config_home() {
        assert_eq!(path(env(&[("XDG_CONFIG_HOME", "/cfg"), ("HOME", "/home/u")])), PathBuf::from("/cfg/autostart/slipcard.desktop"));
        assert_eq!(path(env(&[("XDG_CONFIG_HOME", "rel"), ("HOME", "/home/u")])), PathBuf::from("/home/u/.config/autostart/slipcard.desktop"));
        assert_eq!(path(env(&[("HOME", "/home/u")])), PathBuf::from("/home/u/.config/autostart/slipcard.desktop"));
    }

    #[test]
    fn enabling_writes_a_desktop_entry_and_disabling_removes_it() {
        let dir = TempDir::new("toggle");
        let p = dir.0.join("autostart").join("slipcard.desktop");
        let exec = Path::new("/home/u/.local/bin/slipcard");
        assert!(!is_enabled(&p));
        assert_eq!(toggle(&p, exec), Ok(true));
        let text = fs::read_to_string(&p).unwrap();
        assert!(text.starts_with("[Desktop Entry]\nType=Application\nName=Slipcard\n"));
        assert!(text.contains("\nExec=/home/u/.local/bin/slipcard\n"));
        assert!(text.contains("\nX-GNOME-Autostart-enabled=true\n"));
        // No temporary file left beside it.
        assert_eq!(fs::read_dir(p.parent().unwrap()).unwrap().count(), 1);
        assert_eq!(toggle(&p, exec), Ok(false));
        assert!(!p.exists());
        assert!(disable(&p).is_ok(), "disabling twice is fine");
    }

    #[test]
    fn the_state_survives_a_restart() {
        // The file is the state: a new process sees what the last one left.
        let dir = TempDir::new("restart");
        let p = dir.0.join("autostart").join("slipcard.desktop");
        enable(&p, Path::new("/usr/bin/slipcard")).unwrap();
        let fresh = path(|k| (k == "XDG_CONFIG_HOME").then(|| dir.0.to_string_lossy().into_owned()));
        assert_eq!(fresh, p);
        assert!(is_enabled(&fresh));
    }

    #[test]
    fn exec_with_spaces_is_quoted() {
        assert!(entry(Path::new("/home/u/My Apps/slipcard")).contains("\nExec=\"/home/u/My Apps/slipcard\"\n"));
        assert!(entry(Path::new("/opt/a$b")).contains("\nExec=\"/opt/a\\$b\"\n"));
    }

    #[test]
    fn a_failure_is_a_message_for_the_tray() {
        let dir = TempDir::new("fail");
        // A file where the autostart folder should be makes the write fail.
        fs::write(dir.0.join("autostart"), b"x").unwrap();
        let p = dir.0.join("autostart").join("slipcard.desktop");
        let e = toggle(&p, Path::new("/usr/bin/slipcard")).unwrap_err();
        assert!(e.starts_with("Start at login: cannot write the autostart entry"), "{e}");
    }
}
