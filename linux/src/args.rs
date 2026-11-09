//! Command-line options of the proof of concept.

#[derive(Debug, PartialEq, Eq)]
pub enum Mode {
    /// Print what the app sees of the system and exit.
    Check,
    /// Print the card a popup would show now, or the connection state, and exit.
    NextCard,
    /// Run: a popup every interval; with `show_after`, also one word that
    /// many seconds after the start.
    Run { show_after: Option<u32> },
}

pub const USAGE: &str = "usage: slipcard [--show-after SECONDS] | --check | --next-card";

pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Mode, String> {
    let mut show_after = None;
    let mut check = false;
    let mut next_card = false;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--check" => check = true,
            "--next-card" => next_card = true,
            "--show-after" => {
                let value = args.next().ok_or("--show-after needs a number of seconds")?;
                show_after = Some(
                    value
                        .parse()
                        .map_err(|_| format!("--show-after: not a number of seconds: {value}"))?,
                );
            }
            "-h" | "--help" => return Err(USAGE.to_string()),
            other => return Err(format!("unknown option: {other}\n{USAGE}")),
        }
    }
    Ok(if check {
        Mode::Check
    } else if next_card {
        Mode::NextCard
    } else {
        Mode::Run { show_after }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(args: &[&str]) -> Result<Mode, String> {
        parse(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn no_options_waits_for_the_scheduler() {
        assert_eq!(parse_str(&[]), Ok(Mode::Run { show_after: None }));
    }

    #[test]
    fn show_after_takes_seconds() {
        assert_eq!(parse_str(&["--show-after", "5"]), Ok(Mode::Run { show_after: Some(5) }));
    }

    #[test]
    fn show_after_needs_a_number() {
        assert!(parse_str(&["--show-after"]).is_err());
        assert!(parse_str(&["--show-after", "soon"]).is_err());
        assert!(parse_str(&["--show-after", "-1"]).is_err());
    }

    #[test]
    fn check_wins_over_run() {
        assert_eq!(parse_str(&["--show-after", "5", "--check"]), Ok(Mode::Check));
    }

    #[test]
    fn next_card_is_its_own_mode() {
        assert_eq!(parse_str(&["--next-card"]), Ok(Mode::NextCard));
        assert_eq!(parse_str(&["--next-card", "--check"]), Ok(Mode::Check));
    }

    #[test]
    fn unknown_option_is_an_error() {
        assert!(parse_str(&["--tray"]).is_err());
    }
}
