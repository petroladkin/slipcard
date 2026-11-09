import Foundation
import Testing
import SlipcardCore

@Suite struct CountdownTests {
    let t0 = Date(timeIntervalSince1970: 1_800_000_000)

    @Test func countdownStartsAtTheConfiguredSeconds() {
        #expect(Countdown(seconds: 30, started: t0).label(at: t0) == "Close (30)")
        #expect(Countdown(seconds: 45, started: t0).label(at: t0) == "Close (45)")
    }

    @Test func countdownTicksDownOnWholeSeconds() {
        let c = Countdown(seconds: 30, started: t0)
        #expect(c.label(at: t0 + 0.5) == "Close (30)")
        #expect(c.label(at: t0 + 1) == "Close (29)")
        #expect(c.label(at: t0 + 10) == "Close (20)")
        #expect(c.label(at: t0 + 29.5) == "Close (1)")
        #expect(c.label(at: t0 + 30) == "Close (0)")
    }

    @Test func countdownExpiresAtZeroNotBefore() {
        let c = Countdown(seconds: 30, started: t0)
        #expect(!c.isExpired(at: t0 + 29.9))
        #expect(c.isExpired(at: t0 + 30))
        #expect(c.isExpired(at: t0 + 300))
    }

    @Test func aStoppedCountdownReadsCloseAndNeverExpires() {
        var c = Countdown(seconds: 30, started: t0)
        c.stop()
        #expect(c.label(at: t0 + 5) == "Close")
        #expect(c.remaining(at: t0 + 5) == nil)
        #expect(!c.isExpired(at: t0 + 3600))
    }
}

@MainActor
@Suite struct PopupCountdownTests {
    let t0 = Date(timeIntervalSince1970: 1_800_000_000)
    let card = Card(id: 42, queue: 2, due: 0, questionHTML: "word", answerHTML: "answer")

    final class Recorder: @unchecked Sendable {
        var answers: [(Int64, Ease)] = []
    }

    func model(_ r: Recorder, seconds: Int = 30) -> CardPopupModel {
        CardPopupModel(card: card, countdownSeconds: seconds, shownAt: t0) { id, ease in
            r.answers.append((id, ease))
        }
    }

    @Test func theDefaultCountdownIsTheSettingsDefault() {
        let m = CardPopupModel(card: card, shownAt: t0) { _, _ in }
        #expect(m.closeLabel(at: t0) == "Close (30)")
        #expect(Settings.defaults.countdownSeconds == 30)
    }

    @Test func thePopupClosesAtZero() {
        let r = Recorder()
        let m = model(r)
        var closed = 0
        m.onClose = { closed += 1 }
        m.tick(now: t0 + 10)
        #expect(m.stage == .front)
        m.tick(now: t0 + 30)
        #expect(m.stage == .closed)
        #expect(closed == 1)
    }

    @Test func autoCloseIsNotAnAnswer() {
        let r = Recorder()
        let m = model(r, seconds: 5)
        m.tick(now: t0 + 5)
        #expect(m.stage == .closed)
        #expect(r.answers.isEmpty)
    }

    @Test func closeIsNotAnAnswer() {
        let r = Recorder()
        let m = model(r)
        m.close()
        #expect(m.stage == .closed)
        #expect(r.answers.isEmpty)
    }

    @Test func pointerOverThePopupStopsTheCountdownForGood() {
        let m = model(Recorder())
        var closed = 0
        m.onClose = { closed += 1 }
        m.pointerMoved()
        #expect(m.closeLabel(at: t0 + 1) == "Close")
        m.tick(now: t0 + 30)
        m.tick(now: t0 + 3600)
        #expect(m.stage == .front)
        #expect(closed == 0)
        // Moving again changes nothing: it never restarts.
        m.pointerMoved()
        #expect(m.closeLabel(at: t0 + 3600) == "Close")
    }

    @Test func showStopsTheCountdownForGood() {
        let m = model(Recorder())
        m.tick(now: t0 + 3)
        m.reveal()
        #expect(m.closeLabel(at: t0 + 4) == "Close")
        m.tick(now: t0 + 60)
        #expect(m.stage == .back)
    }

    @Test func aStopIsReportedSoTheButtonRedraws() {
        let m = model(Recorder())
        var changes = 0
        m.onChange = { changes += 1 }
        m.pointerMoved()
        m.pointerMoved()
        #expect(changes == 1)
    }

    @Test func aTickAfterCloseDoesNothing() {
        let m = model(Recorder())
        var closed = 0
        m.onClose = { closed += 1 }
        m.close()
        m.tick(now: t0 + 30)
        #expect(closed == 1)
    }

    @Test func aGradeAfterShowStillCloses() async {
        let r = Recorder()
        let m = model(r, seconds: 5)
        m.reveal()
        m.tick(now: t0 + 10)
        #expect(m.stage == .back)
        await m.grade(.good)
        #expect(m.stage == .closed)
        #expect(r.answers.count == 1)
    }
}
