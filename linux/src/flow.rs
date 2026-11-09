//! What the app does when the scheduler says "now" (PopupFlow), and the
//! card popup's state apart from the window (PopupModel). Plain logic: the
//! GTK layer performs each step (arm the timer, fetch on a worker thread,
//! show the window) and reports back. Same rules as PopupFlow.swift.

use crate::anki::{self, Card, Ease, State};
use crate::card_text::plain;
use crate::countdown::Countdown;
use crate::scheduler::Scheduler;
use crate::settings::Settings;
use crate::tray_menu::TrayState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Nothing to show now: arm the timer at `next_fire`.
    Wait,
    /// Ask AnkiConnect for the next due card, then call `fetched`.
    Fetch,
    /// Show this card; call `popup_closed` when the popup goes away.
    Present(Card),
}

pub struct PopupFlow {
    scheduler: Scheduler,
    anki: State,
}

impl PopupFlow {
    pub fn new(scheduler: Scheduler) -> Self {
        PopupFlow { scheduler, anki: State::Unknown }
    }

    /// The connection state as of the last fetch, for the log and the tray.
    pub fn anki(&self) -> &State {
        &self.anki
    }

    /// When to arm the timer; None while a popup is shown or a card is fetched.
    pub fn next_fire(&self) -> Option<i64> {
        self.scheduler.next_fire()
    }

    /// The timer fired.
    pub fn timer_fired(&mut self, now: i64) -> Step {
        if self.scheduler.tick(now) { Step::Fetch } else { Step::Wait }
    }

    /// "Next word now": at once, unless a popup is up.
    pub fn next_word_now(&mut self, now: i64) -> Step {
        if self.scheduler.next_word_now(now) { Step::Fetch } else { Step::Wait }
    }

    /// The fetch finished: a card is shown; no card due, or an error, means
    /// no popup and one more interval to wait.
    pub fn fetched(&mut self, result: Result<Option<Card>, anki::Error>, now: i64) -> Step {
        match result {
            Ok(Some(card)) => {
                self.anki = State::Connected;
                return Step::Present(card);
            }
            Ok(None) => self.anki = State::Connected,
            Err(e) => self.anki = State::Failed(e),
        }
        self.scheduler.popup_closed(now);
        Step::Wait
    }

    /// The popup went away (graded, closed, or timed out).
    pub fn popup_closed(&mut self, now: i64) {
        self.scheduler.popup_closed(now);
    }

    /// The machine woke from sleep. The macOS app calls it on wake; on Linux
    /// the scheduler's lateness rule covers a suspend, so nothing calls it yet.
    #[allow(dead_code)]
    pub fn did_wake(&mut self, now: i64) {
        self.scheduler.did_wake(now);
    }

    // The tray.

    pub fn pause_for_an_hour(&mut self, now: i64) {
        self.scheduler.pause_for_an_hour(now);
    }

    pub fn resume(&mut self, now: i64) {
        self.scheduler.resume(now);
    }

    /// New interval or quiet hours from the tray; the wait restarts.
    pub fn apply(&mut self, settings: &Settings, now: i64) {
        self.scheduler.apply(settings, now);
    }

    /// A check outside a popup fetch (the tray's due count) learnt the state.
    pub fn connection_checked(&mut self, error: Option<anki::Error>) {
        self.anki = error.map_or(State::Connected, State::Failed);
    }

    /// What the tray shows at `now`.
    pub fn tray_state(
        &self,
        now: i64,
        due_count: Option<usize>,
        settings: &Settings,
        settings_warning: Option<String>,
        save_problem: Option<String>,
    ) -> TrayState {
        TrayState {
            anki: self.anki.clone(),
            due_count,
            paused_until: self.scheduler.paused_until(now),
            quiet_until: self.scheduler.is_quiet(now).then(|| self.scheduler.quiet_end_after(now)),
            settings_warning,
            save_problem,
            settings: settings.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    Front,
    Back,
    Sending(Ease),
    /// The grade was not recorded; the reason is shown, and grading again
    /// is allowed.
    Failed(String),
    Closed,
}

/// The card popup's state: what to show and what the buttons do. The window
/// renders it and calls it; the tests call it the same way.
pub struct PopupModel {
    pub card: Card,
    pub front: String,
    pub back: String,
    stage: Stage,
    /// The Close button's countdown; it stops for good on a reaction.
    countdown: Countdown,
}

impl PopupModel {
    /// `countdown_seconds` comes from the settings; `shown_at_ms` is when the
    /// popup appears, where the countdown starts.
    pub fn new(card: Card, countdown_seconds: u32, shown_at_ms: i64) -> Self {
        PopupModel {
            front: plain(&card.question_html),
            back: plain(&card.answer_html),
            card,
            stage: Stage::Front,
            countdown: Countdown::new(countdown_seconds, shown_at_ms),
        }
    }

    pub fn countdown(&self) -> &Countdown {
        &self.countdown
    }

    /// The Close button's label at `now_ms`: "Close (N)", or "Close" once the
    /// countdown stopped.
    pub fn close_label(&self, now_ms: i64) -> String {
        self.countdown.label(now_ms)
    }

    /// The countdown timer ticked. True when the count reached zero: the
    /// popup closes now, without an answer, the way the Close button closes it.
    pub fn tick(&mut self, now_ms: i64) -> bool {
        if self.stage == Stage::Closed || !self.countdown.is_expired(now_ms) {
            return false;
        }
        self.close()
    }

    /// The pointer moved over the popup: the user reacted, so the countdown
    /// stops for good. True when it stopped now (redraw the button). A
    /// pointer that only rests where the popup appeared sends no motion.
    pub fn pointer_moved(&mut self) -> bool {
        if self.countdown.is_stopped() {
            return false;
        }
        self.countdown.stop();
        true
    }

    pub fn stage(&self) -> &Stage {
        &self.stage
    }

    pub fn shows_answer(&self) -> bool {
        self.stage != Stage::Front
    }

    /// Again / Hard / Good / Easy are on screen (and enabled unless sending).
    pub fn shows_grades(&self) -> bool {
        matches!(self.stage, Stage::Back | Stage::Sending(_) | Stage::Failed(_))
    }

    pub fn is_sending(&self) -> bool {
        matches!(self.stage, Stage::Sending(_))
    }

    pub fn failure(&self) -> Option<&str> {
        match &self.stage {
            Stage::Failed(reason) => Some(reason),
            _ => None,
        }
    }

    /// Show; it stops the countdown for good, like a pointer over the popup.
    pub fn reveal(&mut self) {
        self.countdown.stop();
        if self.stage == Stage::Front {
            self.stage = Stage::Back;
        }
    }

    /// A grade button: the card id and ease to send, or None when grading is
    /// not possible now (before Show, while sending, after close).
    pub fn begin_grade(&mut self, ease: Ease) -> Option<(i64, Ease)> {
        if !self.shows_grades() || self.is_sending() {
            return None;
        }
        self.stage = Stage::Sending(ease);
        Some((self.card.id, ease))
    }

    /// The answer came back. True: the popup closes. On failure the card
    /// stays unanswered and the popup says why.
    pub fn finish_grade(&mut self, result: Result<(), anki::Error>) -> bool {
        if !self.is_sending() {
            return false;
        }
        match result {
            Ok(()) => self.close(),
            Err(e) => {
                self.stage = Stage::Failed(format!("Not saved: {}. The card was not answered.", State::Failed(e)));
                false
            }
        }
    }

    /// Close: the card is not answered. True only the first time.
    pub fn close(&mut self) -> bool {
        if self.stage == Stage::Closed {
            return false;
        }
        self.stage = Stage::Closed;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    const START: i64 = 1_800_000_000;

    fn card() -> Card {
        Card {
            id: 7,
            queue: 2,
            due: 0,
            question_html: "<b>w</b>".into(),
            answer_html: "a".into(),
            next_reviews: vec![],
        }
    }

    fn flow() -> PopupFlow {
        PopupFlow::new(Scheduler::new(&Settings::default(), START, Box::new(|_| 0)))
    }

    /// 2026-11-10 10:00 UTC (12:00 at UTC+2).
    const TEN: i64 = 1_794_304_800;

    fn tray_menu(state: &TrayState) -> crate::tray_menu::TrayMenu {
        crate::tray_menu::TrayMenu::new(state, |_| 7200)
    }

    #[test]
    fn tray_state_follows_the_pause_and_resume() {
        let s = Settings::default();
        let mut f = PopupFlow::new(Scheduler::new(&s, TEN, Box::new(|_| 7200)));
        f.pause_for_an_hour(TEN);
        let paused = f.tray_state(TEN, Some(3), &s, None, None);
        assert_eq!(paused.paused_until, Some(TEN + 3600));
        assert_eq!(tray_menu(&paused).state_lines.last().unwrap(), "Paused until 13:00");
        f.resume(TEN + 60);
        assert_eq!(f.tray_state(TEN, Some(3), &s, None, None).paused_until, None);
    }

    #[test]
    fn tray_state_shows_quiet_hours_only_inside_them() {
        let s = Settings { quiet_hours_enabled: true, ..Settings::default() }; // 22:00-08:00 local, UTC+2
        let f = PopupFlow::new(Scheduler::new(&s, TEN, Box::new(|_| 7200)));
        let night = 1_794_272_400; // 01:00 UTC = 03:00 local
        let state = f.tray_state(night, None, &s, None, None);
        assert_eq!(tray_menu(&state).state_lines.last().unwrap(), "Quiet hours until 08:00");
        assert_eq!(f.tray_state(TEN, None, &s, None, None).quiet_until, None);
    }

    #[test]
    fn applying_new_settings_restarts_the_wait() {
        let mut f = PopupFlow::new(Scheduler::new(&Settings::default(), TEN, Box::new(|_| 0)));
        f.apply(&Settings { interval_minutes: 30, ..Settings::default() }, TEN + 600);
        assert_eq!(f.next_fire(), Some(TEN + 600 + 1800));
    }

    #[test]
    fn a_count_check_sets_the_connection_state() {
        let mut f = flow();
        f.connection_checked(None);
        assert_eq!(*f.anki(), State::Connected);
        f.connection_checked(Some(anki::Error::NotReachable("x".into())));
        assert_eq!(*f.anki(), State::Failed(anki::Error::NotReachable("x".into())));
    }

    #[test]
    fn timer_at_the_interval_fetches() {
        let mut f = flow();
        assert_eq!(f.next_fire(), Some(START + 15 * 60));
        assert_eq!(f.timer_fired(START + 60), Step::Wait);
        assert_eq!(f.timer_fired(START + 15 * 60), Step::Fetch);
        assert_eq!(f.next_fire(), None);
    }

    #[test]
    fn a_due_card_is_presented() {
        let mut f = flow();
        let t = START + 15 * 60;
        f.timer_fired(t);
        assert_eq!(f.fetched(Ok(Some(card())), t), Step::Present(card()));
        assert_eq!(f.anki(), &State::Connected);
        assert_eq!(f.next_fire(), None);
    }

    #[test]
    fn no_due_card_means_no_popup_and_one_more_interval() {
        let mut f = flow();
        let t = START + 15 * 60;
        f.timer_fired(t);
        assert_eq!(f.fetched(Ok(None), t), Step::Wait);
        assert_eq!(f.anki(), &State::Connected);
        assert_eq!(f.next_fire(), Some(t + 15 * 60));
    }

    #[test]
    fn an_error_means_no_popup_and_a_state() {
        let mut f = flow();
        let t = START + 15 * 60;
        f.timer_fired(t);
        assert_eq!(f.fetched(Err(anki::Error::WrongKey), t), Step::Wait);
        assert_eq!(f.anki(), &State::Failed(anki::Error::WrongKey));
        assert_eq!(f.next_fire(), Some(t + 15 * 60));
    }

    #[test]
    fn closing_the_popup_starts_the_next_interval() {
        let mut f = flow();
        let t = START + 15 * 60;
        f.timer_fired(t);
        f.fetched(Ok(Some(card())), t);
        f.popup_closed(t + 20);
        assert_eq!(f.next_fire(), Some(t + 20 + 15 * 60));
    }

    #[test]
    fn next_word_now_fetches_unless_a_popup_is_up() {
        let mut f = flow();
        assert_eq!(f.next_word_now(START), Step::Fetch);
        f.fetched(Ok(Some(card())), START);
        assert_eq!(f.next_word_now(START + 5), Step::Wait);
    }

    #[test]
    fn wake_restarts_the_wait() {
        let mut f = flow();
        let t = START + 3 * 3600;
        f.did_wake(t);
        assert_eq!(f.next_fire(), Some(t + 15 * 60));
    }

    const T0: i64 = 1_800_000_000_000;

    fn model() -> PopupModel {
        PopupModel::new(
            Card {
                id: 42,
                queue: 2,
                due: 0,
                question_html: "<style>.card{}</style><b>reluctant</b>".into(),
                answer_html: "\n\nnot willing<div><i>He was reluctant.</i></div>".into(),
                next_reviews: vec![],
            },
            30,
            T0,
        )
    }

    fn model_with(seconds: u32) -> PopupModel {
        PopupModel::new(card(), seconds, T0)
    }

    #[test]
    fn the_default_countdown_is_the_settings_default() {
        let m = PopupModel::new(card(), Settings::default().countdown_seconds, T0);
        assert_eq!(m.close_label(T0), "Close (30)");
    }

    #[test]
    fn the_popup_closes_at_zero() {
        let mut m = model();
        assert!(!m.tick(T0 + 10_000));
        assert_eq!(m.stage(), &Stage::Front);
        assert!(m.tick(T0 + 30_000));
        assert_eq!(m.stage(), &Stage::Closed);
    }

    #[test]
    fn auto_close_is_not_an_answer() {
        // Closing by the countdown never begins a grade: nothing to send.
        let mut m = model_with(5);
        assert!(m.tick(T0 + 5_000));
        assert_eq!(m.stage(), &Stage::Closed);
        assert_eq!(m.begin_grade(Ease::Good), None);
        assert!(!m.finish_grade(Ok(())));
    }

    #[test]
    fn close_is_not_an_answer() {
        let mut m = model();
        assert!(m.close());
        assert_eq!(m.begin_grade(Ease::Again), None);
    }

    #[test]
    fn pointer_over_the_popup_stops_the_countdown_for_good() {
        let mut m = model();
        assert!(m.pointer_moved());
        assert_eq!(m.close_label(T0 + 1_000), "Close");
        assert!(!m.tick(T0 + 30_000));
        assert!(!m.tick(T0 + 3_600_000));
        assert_eq!(m.stage(), &Stage::Front);
        // Moving again changes nothing: it never restarts.
        assert!(!m.pointer_moved());
        assert_eq!(m.close_label(T0 + 3_600_000), "Close");
    }

    #[test]
    fn show_stops_the_countdown_for_good() {
        let mut m = model();
        m.tick(T0 + 3_000);
        m.reveal();
        assert_eq!(m.close_label(T0 + 4_000), "Close");
        assert!(!m.tick(T0 + 60_000));
        assert_eq!(m.stage(), &Stage::Back);
    }

    #[test]
    fn a_tick_after_close_does_nothing() {
        let mut m = model();
        assert!(m.close());
        assert!(!m.tick(T0 + 30_000));
    }

    #[test]
    fn a_grade_after_show_still_closes() {
        let mut m = model_with(5);
        m.reveal();
        assert!(!m.tick(T0 + 10_000));
        assert_eq!(m.begin_grade(Ease::Good), Some((7, Ease::Good)));
        assert!(m.finish_grade(Ok(())));
    }

    #[test]
    fn front_and_back_are_plain_text() {
        let m = model();
        assert_eq!(m.front, "reluctant");
        assert_eq!(m.back, "not willing\nHe was reluctant.");
        assert_eq!(m.stage(), &Stage::Front);
        assert!(!m.shows_answer());
        assert!(!m.shows_grades());
    }

    #[test]
    fn show_reveals_the_back_and_the_grades() {
        let mut m = model();
        m.reveal();
        assert_eq!(m.stage(), &Stage::Back);
        assert!(m.shows_answer());
        assert!(m.shows_grades());
    }

    #[test]
    fn grades_before_show_are_ignored() {
        let mut m = model();
        assert_eq!(m.begin_grade(Ease::Good), None);
        assert_eq!(m.stage(), &Stage::Front);
    }

    #[test]
    fn a_grade_is_sent_and_closes_the_popup() {
        for ease in [Ease::Again, Ease::Hard, Ease::Good, Ease::Easy] {
            let mut m = model();
            m.reveal();
            assert_eq!(m.begin_grade(ease), Some((42, ease)));
            assert!(m.is_sending());
            assert_eq!(m.begin_grade(Ease::Easy), None, "no second grade while sending");
            assert!(m.finish_grade(Ok(())));
            assert_eq!(m.stage(), &Stage::Closed);
        }
    }

    #[test]
    fn a_failed_grade_keeps_the_popup_and_says_why() {
        let mut m = model();
        m.reveal();
        m.begin_grade(Ease::Good);
        assert!(!m.finish_grade(Err(anki::Error::NotReachable("down".into()))));
        assert_eq!(m.failure(), Some("Not saved: Anki is not running or not reachable. The card was not answered."));
        assert!(m.shows_grades());
        // Grading again is allowed and can succeed.
        assert_eq!(m.begin_grade(Ease::Hard), Some((42, Ease::Hard)));
        assert!(m.finish_grade(Ok(())));
    }

    #[test]
    fn close_does_not_answer_and_counts_once() {
        let mut m = model();
        assert!(m.close());
        assert!(!m.close());
        assert_eq!(m.begin_grade(Ease::Good), None);
        assert!(!m.finish_grade(Ok(())));
    }

    /// The popup's own path against the real test Anki, on demand only (see
    /// anki.rs): begin_grade, the client, finish_grade, as the buttons do.
    #[test]
    #[ignore = "needs the test Anki"]
    fn grade_through_the_popup_model_is_recorded() {
        use serde_json::json;
        let url = std::env::var("SLIPCARD_ANKI_URL").unwrap_or_else(|_| "http://127.0.0.1:8765".into());
        let client = anki::Client::new(&url, std::env::var("SLIPCARD_ANKI_KEY").ok(), anki::DEFAULT_TIMEOUT);
        let profile = client.active_profile().expect("the test Anki answers");
        assert_eq!(profile, "Slipcard Test", "refusing: the active profile is {profile}");
        let card = client.next_due_card("deck:\"Slipcard Test\" is:review is:due").unwrap().expect("a review card");
        let reviews = |id: i64| {
            let r = client.call("getReviewsOfCards", json!({ "cards": [id.to_string()] })).unwrap();
            r[id.to_string()].as_array().cloned().unwrap_or_default()
        };
        let before = reviews(card.id).len();
        let mut m = PopupModel::new(card, 30, 0);
        assert!(!m.front.is_empty() && !m.front.contains('<'));
        m.reveal();
        let (id, ease) = m.begin_grade(Ease::Easy).unwrap();
        assert!(m.finish_grade(client.answer(id, ease)));
        let after = reviews(id);
        assert_eq!(after.len(), before + 1);
        assert_eq!(after.last().unwrap()["ease"], 4);
    }

    /// The countdown reaching zero and the Close button, through the model the
    /// window calls: no review is recorded and the card stays due.
    #[test]
    #[ignore = "needs the test Anki"]
    fn closing_without_a_grade_leaves_the_card_due() {
        use serde_json::json;
        let url = std::env::var("SLIPCARD_ANKI_URL").unwrap_or_else(|_| "http://127.0.0.1:8765".into());
        let client = anki::Client::new(&url, std::env::var("SLIPCARD_ANKI_KEY").ok(), anki::DEFAULT_TIMEOUT);
        let profile = client.active_profile().expect("the test Anki answers");
        assert_eq!(profile, "Slipcard Test", "refusing: the active profile is {profile}");
        for how in ["countdown", "close"] {
            let card = client.next_due_card("deck:\"Slipcard Test\" is:review is:due").unwrap().expect("a review card");
            let id = card.id;
            let reviews = || {
                let r = client.call("getReviewsOfCards", json!({ "cards": [id.to_string()] })).unwrap();
                r[id.to_string()].as_array().map_or(0, Vec::len)
            };
            let before = reviews();
            let mut m = PopupModel::new(card, 5, T0);
            let closed = if how == "countdown" { m.tick(T0 + 5_000) } else { m.close() };
            assert!(closed, "{how}");
            assert_eq!(reviews(), before, "{how}: no review recorded");
            let due = client.call("findCards", json!({ "query": format!("cid:{id} is:due") })).unwrap();
            assert_eq!(due, json!([id]), "{how}: the card stays due");
        }
    }
}
