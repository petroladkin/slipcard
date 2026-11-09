//! What the app does when the scheduler says "now" (PopupFlow), and the
//! card popup's state apart from the window (PopupModel). Plain logic: the
//! GTK layer performs each step (arm the timer, fetch on a worker thread,
//! show the window) and reports back. Same rules as PopupFlow.swift.

use crate::anki::{self, Card, Ease, State};
use crate::card_text::plain;
use crate::scheduler::Scheduler;

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
}

impl PopupModel {
    pub fn new(card: Card) -> Self {
        PopupModel { front: plain(&card.question_html), back: plain(&card.answer_html), card, stage: Stage::Front }
    }

    #[cfg_attr(not(test), allow(dead_code))] // read by the tests
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

    /// Show.
    pub fn reveal(&mut self) {
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

    fn model() -> PopupModel {
        PopupModel::new(Card {
            id: 42,
            queue: 2,
            due: 0,
            question_html: "<style>.card{}</style><b>reluctant</b>".into(),
            answer_html: "\n\nnot willing<div><i>He was reluctant.</i></div>".into(),
            next_reviews: vec![],
        })
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
        let mut m = PopupModel::new(card);
        assert!(!m.front.is_empty() && !m.front.contains('<'));
        m.reveal();
        let (id, ease) = m.begin_grade(Ease::Easy).unwrap();
        assert!(m.finish_grade(client.answer(id, ease)));
        let after = reviews(id);
        assert_eq!(after.len(), before + 1);
        assert_eq!(after.last().unwrap()["ease"], 4);
    }
}
