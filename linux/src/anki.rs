//! A client of AnkiConnect (API version 6): the next due card, answering
//! it, and every way that can fail as a state the tray shows.

use std::fmt;
use std::time::Duration;

use serde_json::{json, Value};

/// A card to show, as AnkiConnect's `cardsInfo` describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub id: i64,
    /// Anki's queue: 0 new, 1 learning, 2 review, 3 learning across days;
    /// below 0 suspended or buried.
    pub queue: i64,
    /// For learning cards (queue 1) a Unix time in seconds; for review and
    /// day-learning cards a day number; for new cards a position.
    pub due: i64,
    /// The front, as Anki renders it (HTML, may start with a <style>).
    pub question_html: String,
    /// The back without the front: Anki's rendered answer after `<hr id=answer>`.
    pub answer_html: String,
    /// What each grade would schedule, e.g. ["<10m", "2d", "3d", "4d"].
    pub next_reviews: Vec<String>,
}

/// The part of Anki's rendered answer after the `<hr id=answer>` marker
/// (the whole answer if a note type has no marker).
pub fn back_of_answer(html: &str) -> &str {
    for marker in ["<hr id=answer>", "<hr id=\"answer\">", "<hr id='answer'>"] {
        if let Some(i) = html.find(marker) {
            return &html[i + marker.len()..];
        }
    }
    html
}

/// A grade, as AnkiConnect's `answerCards` takes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ease {
    Again = 1,
    Hard = 2,
    Good = 3,
    Easy = 4,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Nothing answered at the address: Anki is not running, AnkiConnect is
    /// not installed, or the host is down.
    NotReachable(String),
    TimedOut,
    /// AnkiConnect wants an API key and did not get the right one.
    WrongKey,
    /// AnkiConnect answered with an error.
    Anki(String),
    /// Something answered that is not AnkiConnect, or not as expected.
    BadResponse(String),
}

/// The connection state the tray shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Unknown,
    Connected,
    Failed(Error),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            State::Unknown => write!(f, "Not connected yet"),
            State::Connected => write!(f, "Connected to Anki"),
            State::Failed(Error::NotReachable(_)) => write!(f, "Anki is not running or not reachable"),
            State::Failed(Error::TimedOut) => write!(f, "Anki did not answer in time"),
            State::Failed(Error::WrongKey) => write!(f, "AnkiConnect refused the API key"),
            State::Failed(Error::Anki(m)) => write!(f, "AnkiConnect: {m}"),
            State::Failed(Error::BadResponse(_)) => write!(f, "Unexpected answer from AnkiConnect"),
        }
    }
}

/// Learning cards first (the earliest due first), then learning across days
/// and review cards (the most overdue first), then new cards in their order;
/// suspended and buried cards never.
pub fn sorted(mut cards: Vec<Card>) -> Vec<Card> {
    let group = |c: &Card| match c.queue {
        1 => 0,
        3 => 1,
        2 => 2,
        _ => 3,
    };
    cards.retain(|c| c.queue >= 0);
    cards.sort_by_key(|c| (group(c), c.due, c.id));
    cards
}

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone)]
pub struct Client {
    pub url: String,
    pub key: Option<String>,
    agent: ureq::Agent,
}

impl Client {
    pub fn new(url: &str, key: Option<String>, timeout: Duration) -> Self {
        let agent = ureq::AgentBuilder::new().timeout(timeout).build();
        Client { url: url.to_string(), key, agent }
    }

    pub fn from_settings(s: &crate::settings::Settings) -> Self {
        Client::new(&s.anki_connect_url, s.anki_connect_key.clone(), DEFAULT_TIMEOUT)
    }

    pub fn active_profile(&self) -> Result<String, Error> {
        match self.call("getActiveProfile", json!({}))? {
            Value::String(name) => Ok(name),
            _ => Err(Error::BadResponse("getActiveProfile: not a string".into())),
        }
    }

    /// The card to show next for `query` (an Anki search), or None when none is due.
    pub fn next_due_card(&self, query: &str) -> Result<Option<Card>, Error> {
        let ids = match self.call("findCards", json!({ "query": query }))? {
            Value::Array(ids) if ids.iter().all(Value::is_i64) => ids,
            _ => return Err(Error::BadResponse("findCards: not a list of card ids".into())),
        };
        let mut cards = Vec::new();
        for chunk in ids.chunks(100) {
            match self.call("cardsInfo", json!({ "cards": chunk }))? {
                Value::Array(infos) => cards.extend(infos.iter().filter_map(card)),
                _ => return Err(Error::BadResponse("cardsInfo: not a list of cards".into())),
            }
        }
        Ok(sorted(cards).into_iter().next())
    }

    /// Records a review of `card_id` with `ease`; Anki schedules it.
    pub fn answer(&self, card_id: i64, ease: Ease) -> Result<(), Error> {
        let result = self.call("answerCards", json!({ "answers": [{ "cardId": card_id, "ease": ease as i64 }] }))?;
        if result == json!([true]) {
            Ok(())
        } else {
            Err(Error::Anki("the card was not answered (it may no longer exist)".into()))
        }
    }

    /// Sends one action and returns its `result`, or what went wrong.
    pub fn call(&self, action: &str, params: Value) -> Result<Value, Error> {
        let mut body = json!({ "action": action, "version": 6, "params": params });
        if let Some(key) = &self.key {
            body["key"] = Value::from(key.clone());
        }
        let response = self
            .agent
            .post(&self.url)
            .set("Content-Type", "application/json")
            .send_string(&body.to_string());
        let text = match response {
            Ok(r) => r.into_string().map_err(|e| transport_error(&e))?,
            Err(ureq::Error::Status(code, _)) => return Err(Error::BadResponse(format!("HTTP {code}"))),
            Err(ureq::Error::Transport(t)) => return Err(transport(&t)),
        };
        let reply: Value = serde_json::from_str(&text).map_err(|_| Error::BadResponse("not an AnkiConnect reply".into()))?;
        let Value::Object(reply) = reply else {
            return Err(Error::BadResponse("not an AnkiConnect reply".into()));
        };
        if !reply.contains_key("result") && !reply.contains_key("error") {
            return Err(Error::BadResponse("not an AnkiConnect reply".into()));
        }
        if let Some(Value::String(message)) = reply.get("error") {
            if message.to_lowercase().contains("api key") {
                return Err(Error::WrongKey);
            }
            return Err(Error::Anki(message.clone()));
        }
        Ok(reply.get("result").cloned().unwrap_or(Value::Null))
    }
}

fn card(info: &Value) -> Option<Card> {
    Some(Card {
        id: info.get("cardId")?.as_i64()?,
        queue: info.get("queue")?.as_i64()?,
        due: info.get("due")?.as_i64()?,
        question_html: info.get("question")?.as_str()?.to_string(),
        answer_html: back_of_answer(info.get("answer")?.as_str()?).to_string(),
        next_reviews: info
            .get("nextReviews")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default(),
    })
}

fn is_timeout(e: &std::io::Error) -> bool {
    matches!(e.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock)
}

fn transport_error(e: &std::io::Error) -> Error {
    if is_timeout(e) { Error::TimedOut } else { Error::NotReachable(e.to_string()) }
}

fn transport(t: &ureq::Transport) -> Error {
    let mut source = std::error::Error::source(t);
    while let Some(s) = source {
        if let Some(io) = s.downcast_ref::<std::io::Error>() {
            if is_timeout(io) {
                return Error::TimedOut;
            }
        }
        source = s.source();
    }
    Error::NotReachable(t.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    /// The recorded AnkiConnect replies in testdata/ankiconnect, shared with
    /// the macOS tests.
    fn recorded(name: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../testdata/ankiconnect/{name}.json"));
        std::fs::read_to_string(path).unwrap()
    }

    fn reply(result: Value) -> String {
        json!({ "result": result, "error": null }).to_string()
    }

    enum Behaviour {
        Respond(HashMap<&'static str, String>),
        Silent,
        Status(u16),
    }

    /// A tiny HTTP server on 127.0.0.1 standing in for AnkiConnect. It
    /// records the JSON bodies it receives. Silent accepts connections and
    /// never answers, which is what a hung Anki looks like.
    struct Stub {
        url: String,
        requests: Arc<Mutex<Vec<Value>>>,
        _listener: Option<TcpListener>,
    }

    impl Stub {
        fn new(behaviour: Behaviour) -> Stub {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let requests = Arc::new(Mutex::new(Vec::new()));
            if let Behaviour::Silent = behaviour {
                return Stub { url, requests, _listener: Some(listener) };
            }
            let log = requests.clone();
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { return };
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut length = 0;
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                            break;
                        }
                        if let Some(v) = line.to_lowercase().strip_prefix("content-length:") {
                            length = v.trim().parse().unwrap_or(0);
                        }
                    }
                    let mut body = vec![0; length];
                    reader.read_exact(&mut body).unwrap();
                    let request: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                    let action = request["action"].as_str().unwrap_or("").to_string();
                    log.lock().unwrap().push(request);
                    let (status, payload) = match &behaviour {
                        Behaviour::Status(code) => (*code, String::new()),
                        Behaviour::Respond(replies) => (
                            200,
                            replies.get(action.as_str()).cloned().unwrap_or_else(|| {
                                json!({ "result": null, "error": "unsupported action" }).to_string()
                            }),
                        ),
                        Behaviour::Silent => unreachable!(),
                    };
                    let _ = write!(
                        stream,
                        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                        payload.len()
                    );
                }
            });
            Stub { url, requests, _listener: None }
        }

        fn actions(&self) -> Vec<String> {
            self.requests.lock().unwrap().iter().map(|r| r["action"].as_str().unwrap_or("").to_string()).collect()
        }

        fn request(&self, i: usize) -> Value {
            self.requests.lock().unwrap()[i].clone()
        }
    }

    fn recorded_stub() -> Stub {
        Stub::new(Behaviour::Respond(HashMap::from([
            ("findCards", recorded("findCards")),
            ("cardsInfo", recorded("cardsInfo")),
            ("answerCards", recorded("answerCards")),
            ("getActiveProfile", reply(json!("Slipcard Test"))),
        ])))
    }

    fn client(stub: &Stub, key: Option<&str>) -> Client {
        Client::new(&stub.url, key.map(String::from), DEFAULT_TIMEOUT)
    }

    #[test]
    fn next_due_card_reads_find_cards_then_cards_info() {
        let stub = recorded_stub();
        let card = client(&stub, None).next_due_card("deck:\"Slipcard Test\" is:due").unwrap().unwrap();
        assert_eq!(stub.actions(), ["findCards", "cardsInfo"]);
        assert_eq!(stub.request(0)["params"]["query"], "deck:\"Slipcard Test\" is:due");
        // The recorded cards are a learning, a review and a new card: the
        // learning card comes first.
        assert_eq!(card.queue, 1);
        assert!(card.question_html.ends_with("tedious"));
        assert_eq!(card.answer_html, "\n\nlong and boring<br><i>Filling in forms is tedious.</i>");
        assert_eq!(card.next_reviews.len(), 4);
    }

    #[test]
    fn no_due_card_is_none() {
        let stub = Stub::new(Behaviour::Respond(HashMap::from([("findCards", reply(json!([])))])));
        assert_eq!(client(&stub, None).next_due_card("is:due"), Ok(None));
        assert_eq!(stub.actions(), ["findCards"]);
    }

    #[test]
    fn cards_info_is_asked_in_chunks_of_100() {
        let ids: Vec<i64> = (1..=250).collect();
        let stub = Stub::new(Behaviour::Respond(HashMap::from([
            ("findCards", reply(json!(ids))),
            ("cardsInfo", reply(json!([]))),
        ])));
        client(&stub, None).next_due_card("is:due").unwrap();
        let sizes: Vec<usize> = (1..4).map(|i| stub.request(i)["params"]["cards"].as_array().unwrap().len()).collect();
        assert_eq!(sizes, [100, 100, 50]);
    }

    #[test]
    fn order_is_learning_then_most_overdue_then_new() {
        let card = |id, queue, due| Card {
            id,
            queue,
            due,
            question_html: String::new(),
            answer_html: String::new(),
            next_reviews: vec![],
        };
        let cards = vec![
            card(1, 0, 1),             // new
            card(2, 2, 40),            // review, due day 40
            card(3, 1, 1_800_000_100), // learning, later
            card(4, 2, 12),            // review, more overdue
            card(5, -1, 0),            // suspended
            card(6, 1, 1_800_000_000), // learning, sooner
            card(7, 3, 50),            // learning across days
            card(8, -2, 0),            // buried
        ];
        let ids: Vec<i64> = sorted(cards).iter().map(|c| c.id).collect();
        assert_eq!(ids, [6, 3, 7, 4, 2, 1]);
    }

    #[test]
    fn back_is_the_answer_after_the_marker() {
        assert_eq!(back_of_answer("front\n\n<hr id=answer>\n\nback"), "\n\nback");
        assert_eq!(back_of_answer("front<hr id=\"answer\">back"), "back");
        assert_eq!(back_of_answer("no marker"), "no marker");
    }

    #[test]
    fn answer_sends_card_id_and_ease() {
        let stub = recorded_stub();
        client(&stub, None).answer(1_794_264_260_702, Ease::Good).unwrap();
        assert_eq!(stub.actions(), ["answerCards"]);
        assert_eq!(stub.request(0)["params"]["answers"], json!([{ "cardId": 1_794_264_260_702_i64, "ease": 3 }]));
    }

    #[test]
    fn answer_that_anki_did_not_record_is_an_error() {
        let stub = Stub::new(Behaviour::Respond(HashMap::from([("answerCards", reply(json!([false])))])));
        assert_eq!(
            client(&stub, None).answer(1, Ease::Again),
            Err(Error::Anki("the card was not answered (it may no longer exist)".into()))
        );
    }

    #[test]
    fn ease_values_are_ankis() {
        assert_eq!([Ease::Again, Ease::Hard, Ease::Good, Ease::Easy].map(|e| e as i64), [1, 2, 3, 4]);
    }

    #[test]
    fn key_is_sent_when_set() {
        let stub = recorded_stub();
        client(&stub, Some("k")).active_profile().unwrap();
        client(&stub, None).active_profile().unwrap();
        assert_eq!(stub.request(0)["key"], "k");
        assert_eq!(stub.request(1).get("key"), None);
        assert_eq!(stub.request(0)["version"], 6);
    }

    #[test]
    fn client_from_settings() {
        let s = crate::settings::Settings { anki_connect_key: Some("k".into()), ..Default::default() };
        let c = Client::from_settings(&s);
        assert_eq!(c.url, "http://127.0.0.1:8765");
        assert_eq!(c.key.as_deref(), Some("k"));
    }

    #[test]
    fn wrong_key() {
        let stub = Stub::new(Behaviour::Respond(HashMap::from([("findCards", recorded("error-no-key"))])));
        assert_eq!(client(&stub, None).next_due_card("is:due"), Err(Error::WrongKey));
    }

    #[test]
    fn anki_error() {
        let stub = Stub::new(Behaviour::Respond(HashMap::from([("findCards", recorded("error-unsupported-action"))])));
        assert_eq!(client(&stub, None).next_due_card("is:due"), Err(Error::Anki("unsupported action".into())));
    }

    #[test]
    fn nothing_listening() {
        // Port 9 (discard) has no listener on a development machine.
        let c = Client::new("http://127.0.0.1:9", None, DEFAULT_TIMEOUT);
        assert!(matches!(c.next_due_card("is:due"), Err(Error::NotReachable(_))));
    }

    #[test]
    fn no_answer_times_out() {
        let stub = Stub::new(Behaviour::Silent);
        let c = Client::new(&stub.url, None, Duration::from_millis(500));
        let start = std::time::Instant::now();
        assert_eq!(c.next_due_card("is:due"), Err(Error::TimedOut));
        assert!(start.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn http_error_is_a_bad_response() {
        let stub = Stub::new(Behaviour::Status(500));
        assert_eq!(client(&stub, None).next_due_card("is:due"), Err(Error::BadResponse("HTTP 500".into())));
    }

    #[test]
    fn not_an_anki_connect_reply() {
        let stub = Stub::new(Behaviour::Respond(HashMap::from([("findCards", "{\"hello\": 1}".to_string())])));
        assert_eq!(
            client(&stub, None).next_due_card("is:due"),
            Err(Error::BadResponse("not an AnkiConnect reply".into()))
        );
    }

    #[test]
    fn states_read_for_the_tray() {
        assert_eq!(State::Connected.to_string(), "Connected to Anki");
        assert_eq!(State::Failed(Error::NotReachable("x".into())).to_string(), "Anki is not running or not reachable");
        assert_eq!(State::Failed(Error::TimedOut).to_string(), "Anki did not answer in time");
        assert_eq!(State::Failed(Error::WrongKey).to_string(), "AnkiConnect refused the API key");
        assert_eq!(State::Failed(Error::Anki("boom".into())).to_string(), "AnkiConnect: boom");
        assert_eq!(State::Failed(Error::BadResponse("x".into())).to_string(), "Unexpected answer from AnkiConnect");
    }

    /// Against a real test Anki, on demand only:
    ///
    ///   scripts/test-anki/test_deck.py reset
    ///   SLIPCARD_ANKI_URL=... SLIPCARD_ANKI_KEY=... cargo test -- --ignored
    ///
    /// Each test first checks that the active profile is "Slipcard Test" and
    /// stops before anything else if it is not.
    mod integration {
        use super::*;

        const DECK: &str = "deck:\"Slipcard Test\"";

        fn guarded() -> Client {
            let url = std::env::var("SLIPCARD_ANKI_URL").unwrap_or_else(|_| "http://127.0.0.1:8765".into());
            let c = Client::new(&url, std::env::var("SLIPCARD_ANKI_KEY").ok(), DEFAULT_TIMEOUT);
            let profile = c.active_profile().expect("the test Anki answers");
            assert_eq!(profile, "Slipcard Test", "refusing: the active profile is {profile}");
            c
        }

        fn reviews(c: &Client, id: i64) -> Vec<Value> {
            let r = c.call("getReviewsOfCards", json!({ "cards": [id.to_string()] })).unwrap();
            r[id.to_string()].as_array().cloned().unwrap_or_default()
        }

        #[test]
        #[ignore = "needs the test Anki"]
        fn next_due_card_is_from_the_test_deck_learning_first() {
            let c = guarded();
            let card = c.next_due_card(&format!("{DECK} is:due")).unwrap().expect("a due card");
            assert_eq!(card.queue, 1);
            assert!(!card.question_html.is_empty());
            assert!(!card.answer_html.contains("<hr id=answer>"));
        }

        #[test]
        #[ignore = "needs the test Anki"]
        fn answer_is_recorded_and_moves_the_card() {
            let c = guarded();
            let card = c.next_due_card(&format!("{DECK} is:review is:due")).unwrap().expect("a review card");
            let before = reviews(&c, card.id).len();
            c.answer(card.id, Ease::Good).unwrap();
            let after = reviews(&c, card.id);
            assert_eq!(after.len(), before + 1);
            assert_eq!(after.last().unwrap()["ease"], 3);
            assert_eq!(c.call("findCards", json!({ "query": format!("cid:{} is:due", card.id) })).unwrap(), json!([]));
        }

        #[test]
        #[ignore = "needs the test Anki"]
        fn wrong_key_is_refused() {
            let c = guarded();
            assert!(c.key.is_some(), "the test Anki has no API key");
            let wrong = Client::new(&c.url, Some("wrong".into()), DEFAULT_TIMEOUT);
            assert_eq!(wrong.active_profile(), Err(Error::WrongKey));
        }
    }
}
