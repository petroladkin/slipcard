import Foundation
import Testing
@testable import SlipcardCore

/// testdata/ankiconnect/card-text.json: cards recorded from the test Anki
/// with the text the popup must show; the Linux tests read the same file.
struct CardTextSample: Decodable {
    let note: String
    let question: String
    let answer: String
    let front: String
    let back: String

    static func all() throws -> [CardTextSample] {
        struct File: Decodable { let samples: [CardTextSample] }
        return try JSONDecoder().decode(File.self, from: try recorded("card-text")).samples
    }
}

@Suite struct CardTextTests {
    @Test func recordedCardsReadAsPlainText() throws {
        let samples = try CardTextSample.all()
        #expect(samples.count == 7)
        for s in samples {
            #expect(CardText.plain(s.question) == s.front, "\(s.note): front")
            #expect(CardText.plain(Card.back(ofAnswer: s.answer)) == s.back, "\(s.note): back")
        }
    }

    @Test func styleAndScriptBlocksAreDropped() {
        #expect(CardText.plain("<style>.card { color: red }</style>word") == "word")
        #expect(CardText.plain("<STYLE type=\"text/css\">x</STYLE>a<script>alert(1)</script>b") == "ab")
        #expect(CardText.plain("<style>never closed") == "")
    }

    @Test func breakingTagsMakeLinesAndEmptyLinesGo() {
        #expect(CardText.plain("a<br>b<br/>c<br />d") == "a\nb\nc\nd")
        #expect(CardText.plain("a<div>b</div><div>c</div>") == "a\nb\nc")
        #expect(CardText.plain("<p>a</p><p></p><p>b</p>") == "a\nb")
        #expect(CardText.plain("<ul><li>one</li><li>two</li></ul>") == "one\ntwo")
        #expect(CardText.plain("a<br><br><br>b") == "a\nb")
    }

    @Test func otherTagsAndImagesAreDropped() {
        #expect(CardText.plain("<b>bold</b> <i>it</i> <span style=\"x\">s</span>") == "bold it s")
        #expect(CardText.plain("x<img src=\"a.png\">y") == "xy")
        #expect(CardText.plain("<a href=\"https://example.com\">link</a>") == "link")
    }

    @Test func soundReferencesAreDropped() {
        #expect(CardText.plain("word [sound:word.mp3]") == "word")
        #expect(CardText.plain("[anki:play:q:0]word[anki:play:a:1]") == "word")
        #expect(CardText.plain("[not a sound] stays") == "[not a sound] stays")
    }

    @Test func entitiesAreDecoded() {
        #expect(CardText.plain("&quot;q&quot; &amp; &lt;t&gt; &apos;a&#39;") == "\"q\" & <t> 'a'")
        #expect(CardText.plain("caf&#233; &#x2014; na&iuml;ve") == "café — na&iuml;ve")
        #expect(CardText.plain("a&nbsp;&nbsp;b") == "a b")
        #expect(CardText.plain("AT&T and 5 < 6") == "AT&T and 5 < 6")
    }

    @Test func spacesCollapseAndSourceNewlinesAreSpaces() {
        #expect(CardText.plain("  a   b \n c  ") == "a b c")
        #expect(CardText.plain("\n\n<hr id=answer>\n\nback") == "back")
    }
}

@Suite struct PopupFlowTests {
    let start = Date(timeIntervalSince1970: 1_800_000_000)
    let card = Card(id: 7, queue: 2, due: 0, questionHTML: "<b>w</b>", answerHTML: "a")

    func flow() -> PopupFlow {
        PopupFlow(scheduler: Scheduler(settings: .defaults, now: start, utcOffset: { _ in 0 }))
    }

    @Test func timerAtTheIntervalFetches() {
        var f = flow()
        #expect(f.nextFire == start.addingTimeInterval(15 * 60))
        #expect(f.timerFired(now: start.addingTimeInterval(60)) == .wait)
        #expect(f.timerFired(now: start.addingTimeInterval(15 * 60)) == .fetch)
        #expect(f.nextFire == nil)
    }

    @Test func aDueCardIsPresented() {
        var f = flow()
        let t = start.addingTimeInterval(15 * 60)
        _ = f.timerFired(now: t)
        #expect(f.fetched(.success(card), now: t) == .present(card))
        #expect(f.anki == .connected)
        #expect(f.nextFire == nil)  // no timer while the popup is up
    }

    @Test func noDueCardMeansNoPopupAndOneMoreInterval() {
        var f = flow()
        let t = start.addingTimeInterval(15 * 60)
        _ = f.timerFired(now: t)
        #expect(f.fetched(.success(nil), now: t) == .wait)
        #expect(f.anki == .connected)
        #expect(f.nextFire == t.addingTimeInterval(15 * 60))
    }

    @Test func anErrorMeansNoPopupAndAState() {
        var f = flow()
        let t = start.addingTimeInterval(15 * 60)
        _ = f.timerFired(now: t)
        #expect(f.fetched(.failure(AnkiConnectError.wrongKey), now: t) == .wait)
        #expect(f.anki == .failed(.wrongKey))
        #expect(f.nextFire == t.addingTimeInterval(15 * 60))
    }

    @Test func closingThePopupStartsTheNextInterval() {
        var f = flow()
        let t = start.addingTimeInterval(15 * 60)
        _ = f.timerFired(now: t)
        _ = f.fetched(.success(card), now: t)
        f.popupClosed(now: t.addingTimeInterval(20))
        #expect(f.nextFire == t.addingTimeInterval(20 + 15 * 60))
    }

    @Test func nextWordNowFetchesUnlessAPopupIsUp() {
        var f = flow()
        #expect(f.nextWordNow(now: start) == .fetch)
        _ = f.fetched(.success(card), now: start)
        #expect(f.nextWordNow(now: start.addingTimeInterval(5)) == .wait)
    }

    @Test func wakeRestartsTheWait() {
        var f = flow()
        let t = start.addingTimeInterval(3 * 3600)
        f.didWake(now: t)
        #expect(f.nextFire == t.addingTimeInterval(15 * 60))
    }
}

@MainActor
@Suite struct CardPopupModelTests {
    let card = Card(
        id: 42, queue: 2, due: 0,
        questionHTML: "<style>.card{}</style><b>reluctant</b>",
        answerHTML: "\n\nnot willing<div><i>He was reluctant.</i></div>")

    final class Recorder: @unchecked Sendable {
        var answers: [(Int64, Ease)] = []
        var fail: Error?
    }

    func model(_ r: Recorder) -> CardPopupModel {
        CardPopupModel(card: card) { id, ease in
            r.answers.append((id, ease))
            if let e = r.fail { throw e }
        }
    }

    @Test func frontAndBackArePlainText() {
        let m = model(Recorder())
        #expect(m.front == "reluctant")
        #expect(m.back == "not willing\nHe was reluctant.")
        #expect(m.stage == .front)
        #expect(!m.showsAnswer)
        #expect(!m.showsGrades)
    }

    @Test func showRevealsTheBackAndTheGrades() {
        let m = model(Recorder())
        m.reveal()
        #expect(m.stage == .back)
        #expect(m.showsAnswer)
        #expect(m.showsGrades)
    }

    @Test func gradesBeforeShowAreIgnored() async {
        let r = Recorder()
        let m = model(r)
        await m.grade(.good)
        #expect(r.answers.isEmpty)
        #expect(m.stage == .front)
    }

    @Test(arguments: Ease.allCases)
    func aGradeIsSentAndClosesThePopup(ease: Ease) async {
        let r = Recorder()
        let m = model(r)
        var closed = 0
        m.onClose = { closed += 1 }
        m.reveal()
        await m.grade(ease)
        #expect(r.answers.count == 1)
        #expect(r.answers.first?.0 == 42)
        #expect(r.answers.first?.1 == ease)
        #expect(m.stage == .closed)
        #expect(closed == 1)
    }

    @Test func aFailedGradeKeepsThePopupAndSaysWhy() async {
        let r = Recorder()
        r.fail = AnkiConnectError.notReachable("down")
        let m = model(r)
        var closed = 0
        m.onClose = { closed += 1 }
        m.reveal()
        await m.grade(.good)
        #expect(closed == 0)
        #expect(m.failure == "Not saved: Anki is not running or not reachable. The card was not answered.")
        #expect(m.showsGrades)
        // Grading again is allowed and can succeed.
        r.fail = nil
        await m.grade(.hard)
        #expect(m.stage == .closed)
        #expect(closed == 1)
        #expect(r.answers.map(\.1) == [.good, .hard])
    }

    @Test func closeDoesNotAnswerAndFiresOnce() {
        let r = Recorder()
        let m = model(r)
        var closed = 0
        m.onClose = { closed += 1 }
        m.close()
        m.close()
        #expect(closed == 1)
        #expect(r.answers.isEmpty)
        #expect(m.stage == .closed)
    }

    @Test func everyStageChangeIsReported() async {
        let m = model(Recorder())
        var stages: [CardPopupModel.Stage] = []
        m.onChange = { stages.append(m.stage) }
        m.reveal()
        await m.grade(.easy)
        #expect(stages == [.back, .sending(.easy), .closed])
    }
}
