//! Slipcard's settings, read from `settings.json`. The file may hold any
//! subset of the keys; a missing or invalid value falls back to its default.
//! The keys, defaults and ranges are the same on every platform (see
//! docs/development.md and docs/settings.example.json).

use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

/// A time of day, "HH:MM" in the settings file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockTime {
    pub hour: u8,
    pub minute: u8,
}

impl ClockTime {
    pub fn new(hour: u8, minute: u8) -> Option<Self> {
        (hour < 24 && minute < 60).then_some(ClockTime { hour, minute })
    }

    /// Parses "HH:MM" (24-hour clock, two digits each).
    pub fn parse(text: &str) -> Option<Self> {
        let (h, m) = text.split_once(':')?;
        let two_digits = |s: &str| s.len() == 2 && s.bytes().all(|b| b.is_ascii_digit());
        if !two_digits(h) || !two_digits(m) {
            return None;
        }
        ClockTime::new(h.parse().ok()?, m.parse().ok()?)
    }
}

impl fmt::Display for ClockTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}", self.hour, self.minute)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub anki_connect_url: String,
    pub anki_connect_key: Option<String>,
    pub card_query: String,
    pub interval_minutes: u32,
    pub countdown_seconds: u32,
    pub quiet_hours_enabled: bool,
    pub quiet_hours_start: ClockTime,
    pub quiet_hours_end: ClockTime,
}

pub const INTERVAL_RANGE: std::ops::RangeInclusive<u32> = 1..=1440;
pub const COUNTDOWN_RANGE: std::ops::RangeInclusive<u32> = 5..=600;

/// The keys as they appear in the file.
pub const KEYS: [&str; 8] = [
    "ankiConnectUrl",
    "ankiConnectKey",
    "cardQuery",
    "intervalMinutes",
    "countdownSeconds",
    "quietHoursEnabled",
    "quietHoursStart",
    "quietHoursEnd",
];

impl Default for Settings {
    fn default() -> Self {
        Settings {
            anki_connect_url: "http://127.0.0.1:8765".into(),
            anki_connect_key: None,
            card_query: "is:due -is:suspended".into(),
            interval_minutes: 15,
            countdown_seconds: 30,
            quiet_hours_enabled: false,
            quiet_hours_start: ClockTime { hour: 22, minute: 0 },
            quiet_hours_end: ClockTime { hour: 8, minute: 0 },
        }
    }
}

/// The state of the settings file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum File {
    /// No file yet: the defaults apply, and the first save creates it.
    Missing,
    Valid,
    /// Not a JSON object: the defaults apply, and saving is refused so the
    /// file is never overwritten.
    Invalid(String),
}

/// What reading the settings file gave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Load {
    pub settings: Settings,
    pub file: File,
    /// One line per key whose value was ignored, for the log.
    pub problems: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SaveError {
    FileIsInvalid(String),
    CannotWrite(String),
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveError::FileIsInvalid(reason) => write!(f, "the settings file is {reason}; not overwriting it"),
            SaveError::CannotWrite(e) => write!(f, "cannot write the settings file: {e}"),
        }
    }
}

/// `SLIPCARD_SETTINGS` if set, otherwise `$XDG_CONFIG_HOME/slipcard/settings.json`,
/// falling back to `~/.config/slipcard/settings.json`.
pub fn default_path(env: impl Fn(&str) -> Option<String>) -> PathBuf {
    if let Some(p) = env("SLIPCARD_SETTINGS").filter(|p| !p.is_empty()) {
        return PathBuf::from(p);
    }
    let config = env("XDG_CONFIG_HOME")
        .filter(|p| Path::new(p).is_absolute())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env("HOME").unwrap_or_default()).join(".config"));
    config.join("slipcard").join("settings.json")
}

pub fn load(path: &Path) -> Load {
    match fs::read(path) {
        Ok(data) => parse(&data),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Load { settings: Settings::default(), file: File::Missing, problems: vec![] }
        }
        Err(_) => Load {
            settings: Settings::default(),
            file: File::Invalid("cannot be read".into()),
            problems: vec![],
        },
    }
}

pub fn parse(data: &[u8]) -> Load {
    let Ok(Value::Object(obj)) = serde_json::from_slice::<Value>(data) else {
        return Load {
            settings: Settings::default(),
            file: File::Invalid("not a JSON object".into()),
            problems: vec![],
        };
    };
    let mut s = Settings::default();
    let mut problems = Vec::new();
    let mut ignore = |key: &str, expected: &str, default: &dyn fmt::Display| {
        problems.push(format!("{key}: expected {expected}; using {default}"));
    };
    let whole = |v: &Value, range: &std::ops::RangeInclusive<u32>| {
        v.as_u64().and_then(|n| u32::try_from(n).ok()).filter(|n| range.contains(n))
    };
    for key in KEYS {
        let Some(v) = obj.get(key) else { continue };
        match key {
            "ankiConnectUrl" => match v.as_str() {
                Some(u) if u.starts_with("http://") || u.starts_with("https://") => s.anki_connect_url = u.into(),
                _ => ignore(key, "an http:// or https:// address", &s.anki_connect_url.clone()),
            },
            "ankiConnectKey" => match v {
                Value::Null => s.anki_connect_key = None,
                Value::String(k) => s.anki_connect_key = (!k.is_empty()).then(|| k.clone()),
                _ => ignore(key, "a string or null", &"no key"),
            },
            "cardQuery" => match v.as_str() {
                Some(q) if !q.trim().is_empty() => s.card_query = q.into(),
                _ => ignore(key, "a non-empty Anki search", &s.card_query.clone()),
            },
            "intervalMinutes" => match whole(v, &INTERVAL_RANGE) {
                Some(n) => s.interval_minutes = n,
                None => ignore(key, "a whole number from 1 to 1440", &s.interval_minutes),
            },
            "countdownSeconds" => match whole(v, &COUNTDOWN_RANGE) {
                Some(n) => s.countdown_seconds = n,
                None => ignore(key, "a whole number from 5 to 600", &s.countdown_seconds),
            },
            "quietHoursEnabled" => match v.as_bool() {
                Some(b) => s.quiet_hours_enabled = b,
                None => ignore(key, "true or false", &s.quiet_hours_enabled),
            },
            "quietHoursStart" => match v.as_str().and_then(ClockTime::parse) {
                Some(t) => s.quiet_hours_start = t,
                None => ignore(key, "a time \"HH:MM\"", &s.quiet_hours_start),
            },
            "quietHoursEnd" => match v.as_str().and_then(ClockTime::parse) {
                Some(t) => s.quiet_hours_end = t,
                None => ignore(key, "a time \"HH:MM\"", &s.quiet_hours_end),
            },
            _ => unreachable!("every key in KEYS has a branch"),
        }
    }
    Load { settings: s, file: File::Valid, problems }
}

fn file_values(s: &Settings) -> [(&'static str, Value); 8] {
    [
        ("ankiConnectUrl", Value::from(s.anki_connect_url.clone())),
        ("ankiConnectKey", s.anki_connect_key.clone().map_or(Value::Null, Value::from)),
        ("cardQuery", Value::from(s.card_query.clone())),
        ("intervalMinutes", Value::from(s.interval_minutes)),
        ("countdownSeconds", Value::from(s.countdown_seconds)),
        ("quietHoursEnabled", Value::from(s.quiet_hours_enabled)),
        ("quietHoursStart", Value::from(s.quiet_hours_start.to_string())),
        ("quietHoursEnd", Value::from(s.quiet_hours_end.to_string())),
    ]
}

/// Writes `settings` to `path` atomically (a temporary file in the same
/// folder, then a rename), keeping every key the file has that Slipcard
/// does not know. Refuses to touch a file that is not a JSON object, so a
/// hand edit with a typo is never lost.
pub fn save(settings: &Settings, path: &Path) -> Result<(), SaveError> {
    let mut obj = Map::new();
    if path.exists() {
        match fs::read(path).ok().and_then(|d| serde_json::from_slice::<Value>(&d).ok()) {
            Some(Value::Object(existing)) => obj = existing,
            _ => return Err(SaveError::FileIsInvalid("not a readable JSON object".into())),
        }
    }
    for (key, value) in file_values(settings) {
        obj.insert(key.into(), value);
    }
    let write = || -> std::io::Result<()> {
        let dir = path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(dir)?;
        let tmp = dir.join(format!(
            ".{}.tmp",
            path.file_name().and_then(|n| n.to_str()).unwrap_or("settings.json")
        ));
        let mut f = fs::File::create(&tmp)?;
        f.write_all(serde_json::to_string_pretty(&Value::Object(obj.clone())).expect("a JSON map").as_bytes())?;
        f.write_all(b"\n")?;
        f.sync_all()?;
        fs::rename(&tmp, path)
    };
    write().map_err(|e| SaveError::CannotWrite(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("slipcard-settings-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
        fn settings(&self) -> PathBuf {
            self.0.join("sub").join("settings.json")
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn object(path: &Path) -> Map<String, Value> {
        match serde_json::from_slice(&fs::read(path).unwrap()).unwrap() {
            Value::Object(o) => o,
            other => panic!("not an object: {other}"),
        }
    }

    fn defaults_load(file: File) -> Load {
        Load { settings: Settings::default(), file, problems: vec![] }
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = TempDir::new("missing");
        assert_eq!(load(&dir.settings()), defaults_load(File::Missing));
    }

    #[test]
    fn empty_object_gives_defaults() {
        assert_eq!(parse(b"{}"), defaults_load(File::Valid));
    }

    /// docs/settings.example.json lists every key at its default and is read
    /// by the macOS tests too, so the platforms cannot drift apart.
    #[test]
    fn shared_example_has_every_key_at_its_default() {
        let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/settings.example.json");
        assert_eq!(load(&example), defaults_load(File::Valid));
        let mut keys: Vec<String> = object(&example).keys().cloned().collect();
        let mut expected: Vec<String> = KEYS.iter().map(|k| k.to_string()).collect();
        keys.sort();
        expected.sort();
        assert_eq!(keys, expected);
    }

    #[test]
    fn reads_every_key() {
        let l = parse(
            br#"{"ankiConnectUrl": "https://anki.example:443", "ankiConnectKey": "k",
                 "cardQuery": "deck:English is:due", "intervalMinutes": 10, "countdownSeconds": 45,
                 "quietHoursEnabled": true, "quietHoursStart": "23:30", "quietHoursEnd": "07:05"}"#,
        );
        assert_eq!(l.file, File::Valid);
        assert!(l.problems.is_empty());
        assert_eq!(
            l.settings,
            Settings {
                anki_connect_url: "https://anki.example:443".into(),
                anki_connect_key: Some("k".into()),
                card_query: "deck:English is:due".into(),
                interval_minutes: 10,
                countdown_seconds: 45,
                quiet_hours_enabled: true,
                quiet_hours_start: ClockTime::new(23, 30).unwrap(),
                quiet_hours_end: ClockTime::new(7, 5).unwrap(),
            }
        );
    }

    #[test]
    fn missing_keys_keep_their_defaults() {
        let l = parse(br#"{"intervalMinutes": 5}"#);
        assert_eq!(l.settings, Settings { interval_minutes: 5, ..Settings::default() });
        assert!(l.problems.is_empty());
    }

    #[test]
    fn invalid_values_fall_back_one_by_one() {
        let l = parse(
            br#"{"ankiConnectUrl": "ftp://x", "ankiConnectKey": 5, "cardQuery": "  ",
                 "intervalMinutes": 0, "countdownSeconds": 2.5, "quietHoursEnabled": "yes",
                 "quietHoursStart": "24:00", "quietHoursEnd": "7:00"}"#,
        );
        assert_eq!(l.file, File::Valid);
        assert_eq!(l.settings, Settings::default());
        assert_eq!(l.problems.len(), 8);
        assert!(l.problems.contains(&"intervalMinutes: expected a whole number from 1 to 1440; using 15".to_string()));
    }

    #[test]
    fn booleans_are_not_numbers_and_numbers_are_not_booleans() {
        let l = parse(br#"{"intervalMinutes": true, "quietHoursEnabled": 1}"#);
        assert_eq!(l.settings, Settings::default());
        assert_eq!(l.problems.len(), 2);
    }

    #[test]
    fn empty_or_null_key_means_no_key() {
        assert_eq!(parse(br#"{"ankiConnectKey": ""}"#).settings.anki_connect_key, None);
        let l = parse(br#"{"ankiConnectKey": null}"#);
        assert_eq!(l.settings.anki_connect_key, None);
        assert!(l.problems.is_empty());
    }

    #[test]
    fn not_an_object_is_an_invalid_file() {
        for json in ["not json", "[1, 2]", "\"text\"", ""] {
            assert_eq!(parse(json.as_bytes()), defaults_load(File::Invalid("not a JSON object".into())), "{json}");
        }
    }

    #[test]
    fn clock_time_needs_two_digits_each() {
        assert_eq!(ClockTime::parse("00:00"), ClockTime::new(0, 0));
        assert_eq!(ClockTime::parse("23:59").unwrap().to_string(), "23:59");
        for bad in ["24:00", "12:60", "7:00", "07:5", "+1:00", "ab:cd", "0700", "07:00:00"] {
            assert_eq!(ClockTime::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = TempDir::new("roundtrip");
        let s = Settings {
            interval_minutes: 30,
            anki_connect_key: Some("secret".into()),
            quiet_hours_enabled: true,
            ..Settings::default()
        };
        save(&s, &dir.settings()).unwrap();
        assert_eq!(load(&dir.settings()), Load { settings: s, file: File::Valid, problems: vec![] });
    }

    #[test]
    fn save_keeps_keys_it_does_not_know() {
        let dir = TempDir::new("unknown");
        fs::create_dir_all(dir.settings().parent().unwrap()).unwrap();
        fs::write(dir.settings(), br#"{"intervalMinutes": 5, "futureOption": {"a": [1, 2]}, "note": "mine"}"#).unwrap();
        let mut s = load(&dir.settings()).settings;
        s.quiet_hours_enabled = true;
        save(&s, &dir.settings()).unwrap();
        let saved = object(&dir.settings());
        assert_eq!(saved["note"], "mine");
        assert_eq!(saved["futureOption"]["a"], serde_json::json!([1, 2]));
        assert_eq!(saved["intervalMinutes"], 5);
        assert_eq!(saved["quietHoursEnabled"], true);
    }

    #[test]
    fn save_refuses_an_invalid_file_and_leaves_it_alone() {
        let dir = TempDir::new("invalid");
        fs::create_dir_all(dir.settings().parent().unwrap()).unwrap();
        let original = br#"{"intervalMinutes": 5,, oops"#;
        fs::write(dir.settings(), original).unwrap();
        assert_eq!(
            save(&Settings::default(), &dir.settings()),
            Err(SaveError::FileIsInvalid("not a readable JSON object".into()))
        );
        assert_eq!(fs::read(dir.settings()).unwrap(), original);
    }

    #[test]
    fn save_creates_the_folder_and_writes_every_key() {
        let dir = TempDir::new("create");
        save(&Settings::default(), &dir.settings()).unwrap();
        let saved = object(&dir.settings());
        let mut keys: Vec<&str> = saved.keys().map(String::as_str).collect();
        let mut expected = KEYS.to_vec();
        keys.sort();
        expected.sort();
        assert_eq!(keys, expected);
        assert_eq!(saved["ankiConnectKey"], Value::Null);
        assert_eq!(saved["quietHoursStart"], "22:00");
        // No temporary file is left behind.
        assert_eq!(fs::read_dir(dir.settings().parent().unwrap()).unwrap().count(), 1);
    }

    #[test]
    fn path_follows_the_environment() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
        };
        assert_eq!(default_path(env(&[("SLIPCARD_SETTINGS", "/tmp/x/s.json")])), PathBuf::from("/tmp/x/s.json"));
        assert_eq!(
            default_path(env(&[("XDG_CONFIG_HOME", "/cfg"), ("HOME", "/home/u")])),
            PathBuf::from("/cfg/slipcard/settings.json")
        );
        assert_eq!(
            default_path(env(&[("XDG_CONFIG_HOME", "relative"), ("HOME", "/home/u")])),
            PathBuf::from("/home/u/.config/slipcard/settings.json")
        );
        assert_eq!(default_path(env(&[("HOME", "/home/u")])), PathBuf::from("/home/u/.config/slipcard/settings.json"));
    }
}
