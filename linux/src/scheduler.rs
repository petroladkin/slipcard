//! Decides when a popup comes. Plain logic: it never reads the clock, the
//! caller passes the time (Unix seconds) to every call, and a function gives
//! the UTC offset at an instant so quiet hours work in local time. The app
//! arms one timer at `next_fire` and calls `tick` when it fires.
//!
//! Rules (the same as the macOS app's):
//! - the first popup comes one interval after the start;
//! - the next interval starts when the popup closes, so popups never stack;
//! - pause for an hour: no popup until the hour is over, then the first one
//!   one interval after the pause ends; `resume` ends the pause at once;
//! - quiet hours: no popup inside the range (it may cross midnight); a popup
//!   due inside it moves to the end of the range;
//! - next word now shows a popup at once, also during a pause or quiet
//!   hours, and the interval restarts when that popup closes;
//! - after sleep or a long absence at most one popup comes, one interval
//!   after wake: a tick more than `LATENESS` after the due time restarts the
//!   wait instead of showing the missed popup.

use crate::settings::{ClockTime, Settings};

/// How late a tick may be (seconds) and still show the popup it was armed for.
pub const LATENESS: i64 = 60;
pub const PAUSE_LENGTH: i64 = 3600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Waiting { due: i64 },
    Showing,
}

pub type UtcOffset = Box<dyn Fn(i64) -> i64>;

pub struct Scheduler {
    phase: Phase,
    paused_until: Option<i64>,
    interval: i64,
    quiet_hours: Option<(ClockTime, ClockTime)>,
    utc_offset: UtcOffset,
}

fn interval_of(s: &Settings) -> i64 {
    i64::from(s.interval_minutes) * 60
}

fn quiet_of(s: &Settings) -> Option<(ClockTime, ClockTime)> {
    s.quiet_hours_enabled.then_some((s.quiet_hours_start, s.quiet_hours_end))
}

impl Scheduler {
    /// Starts waiting: the first popup is one interval after `now`.
    pub fn new(settings: &Settings, now: i64, utc_offset: UtcOffset) -> Self {
        let interval = interval_of(settings);
        Scheduler {
            phase: Phase::Waiting { due: now + interval },
            paused_until: None,
            interval,
            quiet_hours: quiet_of(settings),
            utc_offset,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn interval(&self) -> i64 {
        self.interval
    }

    /// New interval or quiet hours (from the tray or the settings file);
    /// the wait restarts from `now`.
    pub fn apply(&mut self, settings: &Settings, now: i64) {
        self.interval = interval_of(settings);
        self.quiet_hours = quiet_of(settings);
        if let Phase::Waiting { .. } = self.phase {
            self.phase = Phase::Waiting { due: now + self.interval };
        }
    }

    /// When the app should arm its timer next; None while a popup is shown.
    pub fn next_fire(&self) -> Option<i64> {
        let Phase::Waiting { due } = self.phase else { return None };
        if let Some(p) = self.paused_until {
            if due < p {
                return Some(p);
            }
        }
        Some(if self.is_quiet(due) { self.quiet_end_after(due) } else { due })
    }

    /// Called when the timer fires (or at any time). True: show a popup now.
    pub fn tick(&mut self, now: i64) -> bool {
        let Phase::Waiting { due } = self.phase else { return false };
        if let Some(p) = self.paused_until {
            if now < p {
                return false;
            }
            self.paused_until = None;
        }
        if now < due {
            return false;
        }
        if now - due > LATENESS {
            // Asleep or suspended through the due time: start over, no burst.
            self.phase = Phase::Waiting { due: now + self.interval };
            return false;
        }
        if self.is_quiet(now) {
            self.phase = Phase::Waiting { due: self.quiet_end_after(now) };
            return false;
        }
        self.phase = Phase::Showing;
        true
    }

    /// The popup closed (answered, closed or timed out).
    pub fn popup_closed(&mut self, now: i64) {
        self.phase = Phase::Waiting { due: now + self.interval };
    }

    pub fn pause_for_an_hour(&mut self, now: i64) {
        let end = now + PAUSE_LENGTH;
        self.paused_until = Some(end);
        if let Phase::Waiting { .. } = self.phase {
            self.phase = Phase::Waiting { due: end + self.interval };
        }
    }

    pub fn resume(&mut self, now: i64) {
        if self.paused_until.take().is_some() {
            if let Phase::Waiting { .. } = self.phase {
                self.phase = Phase::Waiting { due: now + self.interval };
            }
        }
    }

    pub fn is_paused(&self, now: i64) -> bool {
        self.paused_until.is_some_and(|p| now < p)
    }

    /// "Next word now": true unless a popup is already on screen.
    pub fn next_word_now(&mut self, _now: i64) -> bool {
        if self.phase == Phase::Showing {
            return false;
        }
        self.phase = Phase::Showing;
        true
    }

    /// The machine woke up: one interval from now before the next popup.
    pub fn did_wake(&mut self, now: i64) {
        if let Phase::Waiting { .. } = self.phase {
            self.phase = Phase::Waiting { due: now + self.interval };
        }
    }

    /// Whether `at` falls inside quiet hours (start included, end not; a
    /// range with start equal to end is empty).
    pub fn is_quiet(&self, at: i64) -> bool {
        let Some((start, end)) = self.quiet_hours else { return false };
        let s = i64::from(start.hour) * 60 + i64::from(start.minute);
        let e = i64::from(end.hour) * 60 + i64::from(end.minute);
        let m = (at + (self.utc_offset)(at)).rem_euclid(86400) / 60;
        if s == e {
            return false;
        }
        if s < e { m >= s && m < e } else { m >= s || m < e }
    }

    /// The first instant after `at` at which quiet hours end.
    pub fn quiet_end_after(&self, at: i64) -> i64 {
        let Some((_, end)) = self.quiet_hours else { return at };
        let local = at + (self.utc_offset)(at);
        let mut end_local = local - local.rem_euclid(86400) + (i64::from(end.hour) * 60 + i64::from(end.minute)) * 60;
        if end_local <= local {
            end_local += 86400;
        }
        // The offset at the end itself, in case a DST change lies between.
        let guess = end_local - (self.utc_offset)(at);
        end_local - (self.utc_offset)(guess)
    }
}

#[cfg(test)]
mod tests {
    //! Every test drives the scheduler with explicit times: midnight UTC of
    //! a fixed day plus minutes, and a fixed UTC offset (0 unless a test
    //! says otherwise). The names mirror the macOS tests.
    use super::*;

    const MIDNIGHT: i64 = 1_794_268_800; // 00:00 UTC

    fn at_day(hhmm: &str, day: i64) -> i64 {
        let t = ClockTime::parse(hhmm).unwrap();
        MIDNIGHT + day * 86400 + (i64::from(t.hour) * 60 + i64::from(t.minute)) * 60
    }

    fn at(hhmm: &str) -> i64 {
        at_day(hhmm, 0)
    }

    fn settings(interval: u32, quiet: Option<(&str, &str)>) -> Settings {
        let mut s = Settings { interval_minutes: interval, ..Settings::default() };
        if let Some((start, end)) = quiet {
            s.quiet_hours_enabled = true;
            s.quiet_hours_start = ClockTime::parse(start).unwrap();
            s.quiet_hours_end = ClockTime::parse(end).unwrap();
        }
        s
    }

    fn scheduler(s: &Settings, start: &str, offset: i64) -> Scheduler {
        Scheduler::new(s, at(start), Box::new(move |_| offset))
    }

    fn plain(start: &str) -> Scheduler {
        scheduler(&settings(15, None), start, 0)
    }

    // AC-1 interval, no stacking

    #[test]
    fn first_popup_comes_one_interval_after_start() {
        let mut s = plain("10:00");
        assert_eq!(s.next_fire(), Some(at("10:15")));
        assert!(!s.tick(at("10:15") - 1));
        assert!(s.tick(at("10:15")));
        assert_eq!(s.phase(), Phase::Showing);
    }

    #[test]
    fn next_interval_starts_when_the_popup_closes() {
        let mut s = plain("10:00");
        assert!(s.tick(at("10:15")));
        s.popup_closed(at("10:17"));
        assert_eq!(s.next_fire(), Some(at("10:32")));
        assert!(!s.tick(at("10:31")));
        assert!(s.tick(at("10:32")));
    }

    #[test]
    fn popups_never_stack() {
        let mut s = plain("10:00");
        assert!(s.tick(at("10:15")));
        assert_eq!(s.next_fire(), None);
        assert!(!s.tick(at("10:30")));
        assert!(!s.tick(at("10:55")));
        assert!(!s.next_word_now(at("10:56")));
    }

    #[test]
    fn settings_change_restarts_the_wait() {
        let mut s = plain("10:00");
        s.apply(&settings(5, None), at("10:10"));
        assert_eq!(s.next_fire(), Some(at("10:15")));
        assert_eq!(s.interval(), 300);
    }

    // AC-2 pause

    #[test]
    fn pause_for_an_hour_holds_popups_until_the_hour_is_over() {
        let mut s = plain("10:00");
        s.pause_for_an_hour(at("10:05"));
        assert!(s.is_paused(at("11:04")));
        for minute in ["10:15", "10:30", "10:45", "11:00", "11:04"] {
            assert!(!s.tick(at(minute)), "{minute}");
        }
        assert!(!s.is_paused(at("11:05")));
    }

    #[test]
    fn after_the_pause_the_normal_interval_resumes() {
        let mut s = plain("10:00");
        s.pause_for_an_hour(at("10:05"));
        // The pause ends at 11:05; the first popup one interval later.
        assert_eq!(s.next_fire(), Some(at("11:20")));
        assert!(!s.tick(at("11:19")));
        assert!(s.tick(at("11:20")));
    }

    #[test]
    fn resume_ends_the_pause_at_once() {
        let mut s = plain("10:00");
        s.pause_for_an_hour(at("10:05"));
        s.resume(at("10:20"));
        assert!(!s.is_paused(at("10:20")));
        assert_eq!(s.next_fire(), Some(at("10:35")));
        assert!(s.tick(at("10:35")));
    }

    #[test]
    fn pause_during_a_popup_keeps_it_and_holds_the_next_one() {
        let mut s = plain("10:00");
        assert!(s.tick(at("10:15")));
        s.pause_for_an_hour(at("10:16"));
        s.popup_closed(at("10:17"));
        assert!(!s.tick(at("10:32")));
        assert!(!s.tick(at("11:15")));
        assert_eq!(s.next_fire(), Some(at("11:16")));
    }

    // AC-3 quiet hours

    #[test]
    fn no_popup_inside_quiet_hours() {
        let mut s = scheduler(&settings(15, Some(("12:00", "13:00"))), "11:50", 0);
        assert!(s.is_quiet(at("12:00")));
        assert!(s.is_quiet(at("12:59")));
        assert!(!s.is_quiet(at("13:00")));
        assert!(!s.tick(at("12:05")));
    }

    #[test]
    fn range_across_midnight_is_quiet_on_both_sides() {
        let s = scheduler(&settings(15, Some(("22:00", "08:00"))), "21:00", 0);
        assert!(s.is_quiet(at("22:00")));
        assert!(s.is_quiet(at("23:30")));
        assert!(s.is_quiet(at_day("03:00", 1)));
        assert!(s.is_quiet(at_day("07:59", 1)));
        assert!(!s.is_quiet(at_day("08:00", 1)));
        assert!(!s.is_quiet(at("12:00")));
        assert!(!s.is_quiet(at("21:59")));
    }

    #[test]
    fn popup_due_inside_quiet_hours_moves_to_their_end() {
        let mut s = scheduler(&settings(15, Some(("22:00", "08:00"))), "21:50", 0);
        // Due at 22:05, inside the range: moved to 08:00 the next day.
        assert_eq!(s.next_fire(), Some(at_day("08:00", 1)));
        assert!(!s.tick(at("22:05")));
        assert_eq!(s.next_fire(), Some(at_day("08:00", 1)));
        assert!(s.tick(at_day("08:00", 1)));
    }

    #[test]
    fn due_after_midnight_inside_the_range_moves_to_that_morning() {
        let s = scheduler(&settings(30, Some(("22:00", "08:00"))), "02:00", 0);
        assert_eq!(s.next_fire(), Some(at("08:00")));
    }

    #[test]
    fn quiet_hours_off_means_never_quiet() {
        let mut s = plain("21:50");
        assert!(!s.is_quiet(at("23:00")));
        assert!(s.tick(at("22:05")));
    }

    #[test]
    fn equal_start_and_end_is_never_quiet() {
        let s = scheduler(&settings(15, Some(("08:00", "08:00"))), "07:00", 0);
        assert!(!s.is_quiet(at("08:00")));
        assert!(!s.is_quiet(at("20:00")));
    }

    #[test]
    fn quiet_hours_use_local_time() {
        // UTC+2: 20:30 UTC is 22:30 local, inside 22:00-08:00; the range
        // ends at 08:00 local = 06:00 UTC.
        let s = scheduler(&settings(15, Some(("22:00", "08:00"))), "20:15", 7200);
        assert!(s.is_quiet(at("20:30")));
        assert!(!s.is_quiet(at("19:30")));
        assert_eq!(s.next_fire(), Some(at_day("06:00", 1)));
    }

    // AC-4 next word now

    #[test]
    fn next_word_now_shows_at_once() {
        let mut s = plain("10:00");
        assert!(s.next_word_now(at("10:01")));
        assert_eq!(s.phase(), Phase::Showing);
    }

    #[test]
    fn next_word_now_works_during_a_pause() {
        let mut s = plain("10:00");
        s.pause_for_an_hour(at("10:01"));
        assert!(s.next_word_now(at("10:02")));
        s.popup_closed(at("10:03"));
        // The pause still holds the regular popups.
        assert!(s.is_paused(at("10:20")));
        assert!(!s.tick(at("10:18")));
    }

    #[test]
    fn next_word_now_works_during_quiet_hours() {
        let mut s = scheduler(&settings(15, Some(("22:00", "08:00"))), "22:30", 0);
        assert!(s.next_word_now(at("23:00")));
    }

    #[test]
    fn next_word_now_restarts_the_interval() {
        let mut s = plain("10:00");
        assert!(s.next_word_now(at("10:10")));
        s.popup_closed(at("10:11"));
        assert_eq!(s.next_fire(), Some(at("10:26")));
        assert!(!s.tick(at("10:15")));
    }

    // AC-5 sleep and absence

    #[test]
    fn after_sleep_at_most_one_popup_one_interval_after_wake() {
        let mut s = plain("10:00");
        // Asleep 10:05-13:00: five popups were missed; none is shown at wake.
        assert!(!s.tick(at("13:00")));
        assert_eq!(s.next_fire(), Some(at("13:15")));
        assert!(!s.tick(at("13:10")));
        assert!(s.tick(at("13:15")));
        assert!(!s.tick(at("13:16")));
    }

    #[test]
    fn did_wake_restarts_the_wait() {
        let mut s = plain("10:00");
        s.did_wake(at("10:12"));
        assert_eq!(s.next_fire(), Some(at("10:27")));
    }

    #[test]
    fn slightly_late_timer_still_shows() {
        let mut s = plain("10:00");
        assert!(s.tick(at("10:15") + 59));
    }

    #[test]
    fn late_timer_after_quiet_hours_waits_one_interval() {
        let mut s = scheduler(&settings(15, Some(("22:00", "08:00"))), "21:50", 0);
        // The machine slept through the night and wakes at 09:30.
        assert!(!s.tick(at_day("09:30", 1)));
        assert_eq!(s.next_fire(), Some(at_day("09:45", 1)));
    }
}
