import Foundation
import Testing
import SlipcardCore

/// Every test drives the scheduler with explicit times: midnight UTC of a
/// fixed day plus minutes, and a fixed UTC offset (0 unless a test says
/// otherwise). The names mirror the Linux tests.
@Suite struct SchedulerTests {
    static let midnight = Date(timeIntervalSince1970: 1_794_268_800)  // 00:00 UTC

    /// "HH:MM" on the test day (or `day` days later), UTC.
    func at(_ hhmm: String, day: Int = 0, plusSeconds: TimeInterval = 0) -> Date {
        let t = ClockTime(hhmm)!
        return Self.midnight.addingTimeInterval(
            TimeInterval(day * 86400 + (t.hour * 60 + t.minute) * 60) + plusSeconds)
    }

    func settings(interval: Int = 15, quiet: (String, String)? = nil) -> Settings {
        var s = Settings.defaults
        s.intervalMinutes = interval
        if let (start, end) = quiet {
            s.quietHoursEnabled = true
            s.quietHoursStart = ClockTime(start)!
            s.quietHoursEnd = ClockTime(end)!
        }
        return s
    }

    func scheduler(_ s: Settings, start: String, offset: Int = 0) -> Scheduler {
        Scheduler(settings: s, now: at(start), utcOffset: { _ in offset })
    }

    // MARK: AC-1 interval, no stacking

    @Test func firstPopupComesOneIntervalAfterStart() {
        var s = scheduler(settings(), start: "10:00")
        #expect(s.nextFire == at("10:15"))
        do {
            let ok = !s.tick(now: at("10:14", plusSeconds: 59))
            #expect(ok)
        }
        do {
            let ok = s.tick(now: at("10:15"))
            #expect(ok)
        }
        #expect(s.phase == .showing)
    }

    @Test func nextIntervalStartsWhenThePopupCloses() {
        var s = scheduler(settings(), start: "10:00")
        do {
            let ok = s.tick(now: at("10:15"))
            #expect(ok)
        }
        s.popupClosed(now: at("10:17"))
        #expect(s.nextFire == at("10:32"))
        do {
            let ok = !s.tick(now: at("10:31"))
            #expect(ok)
        }
        do {
            let ok = s.tick(now: at("10:32"))
            #expect(ok)
        }
    }

    @Test func popupsNeverStack() {
        var s = scheduler(settings(), start: "10:00")
        do {
            let ok = s.tick(now: at("10:15"))
            #expect(ok)
        }
        // Still on screen 40 minutes later: nothing else comes.
        #expect(s.nextFire == nil)
        do {
            let ok = !s.tick(now: at("10:30"))
            #expect(ok)
        }
        do {
            let ok = !s.tick(now: at("10:55"))
            #expect(ok)
        }
        do {
            let ok = !s.nextWordNow(now: at("10:56"))
            #expect(ok)
        }
    }

    @Test func settingsChangeRestartsTheWait() {
        var s = scheduler(settings(), start: "10:00")
        s.apply(settings(interval: 5), now: at("10:10"))
        #expect(s.nextFire == at("10:15"))
        #expect(s.interval == 300)
    }

    // MARK: AC-2 pause

    @Test func pauseForAnHourHoldsPopupsUntilTheHourIsOver() {
        var s = scheduler(settings(), start: "10:00")
        s.pauseForAnHour(now: at("10:05"))
        #expect(s.isPaused(now: at("11:04")))
        for minute in ["10:15", "10:30", "10:45", "11:00", "11:04"] {
            do {
                let ok = !s.tick(now: at(minute))
                #expect(ok, "\(minute)")
            }
        }
        #expect(!s.isPaused(now: at("11:05")))
    }

    @Test func afterThePauseTheNormalIntervalResumes() {
        var s = scheduler(settings(), start: "10:00")
        s.pauseForAnHour(now: at("10:05"))
        // The pause ends at 11:05; the first popup one interval later.
        #expect(s.nextFire == at("11:20"))
        do {
            let ok = !s.tick(now: at("11:19"))
            #expect(ok)
        }
        do {
            let ok = s.tick(now: at("11:20"))
            #expect(ok)
        }
    }

    @Test func resumeEndsThePauseAtOnce() {
        var s = scheduler(settings(), start: "10:00")
        s.pauseForAnHour(now: at("10:05"))
        s.resume(now: at("10:20"))
        #expect(!s.isPaused(now: at("10:20")))
        #expect(s.nextFire == at("10:35"))
        do {
            let ok = s.tick(now: at("10:35"))
            #expect(ok)
        }
    }

    @Test func pauseDuringAPopupKeepsItAndHoldsTheNextOne() {
        var s = scheduler(settings(), start: "10:00")
        do {
            let ok = s.tick(now: at("10:15"))
            #expect(ok)
        }
        s.pauseForAnHour(now: at("10:16"))
        s.popupClosed(now: at("10:17"))
        do {
            let ok = !s.tick(now: at("10:32"))
            #expect(ok)
        }
        do {
            let ok = !s.tick(now: at("11:15"))
            #expect(ok)
        }
        #expect(s.nextFire == at("11:16"))
    }

    // MARK: AC-3 quiet hours

    @Test func noPopupInsideQuietHours() {
        var s = scheduler(settings(quiet: ("12:00", "13:00")), start: "11:50")
        #expect(s.isQuiet(at("12:00")))
        #expect(s.isQuiet(at("12:59")))
        #expect(!s.isQuiet(at("13:00")))
        do {
            let ok = !s.tick(now: at("12:05"))
            #expect(ok)
        }
    }

    @Test func rangeAcrossMidnightIsQuietOnBothSides() {
        let s = scheduler(settings(quiet: ("22:00", "08:00")), start: "21:00")
        #expect(s.isQuiet(at("22:00")))
        #expect(s.isQuiet(at("23:30")))
        #expect(s.isQuiet(at("03:00", day: 1)))
        #expect(s.isQuiet(at("07:59", day: 1)))
        #expect(!s.isQuiet(at("08:00", day: 1)))
        #expect(!s.isQuiet(at("12:00")))
        #expect(!s.isQuiet(at("21:59")))
    }

    @Test func popupDueInsideQuietHoursMovesToTheirEnd() {
        var s = scheduler(settings(quiet: ("22:00", "08:00")), start: "21:50")
        // Due at 22:05, inside the range: moved to 08:00 the next day.
        #expect(s.nextFire == at("08:00", day: 1))
        do {
            let ok = !s.tick(now: at("22:05"))
            #expect(ok)
        }
        #expect(s.nextFire == at("08:00", day: 1))
        do {
            let ok = s.tick(now: at("08:00", day: 1))
            #expect(ok)
        }
    }

    @Test func dueAfterMidnightInsideTheRangeMovesToThatMorning() {
        let s = scheduler(settings(interval: 30, quiet: ("22:00", "08:00")), start: "02:00")
        #expect(s.nextFire == at("08:00"))
    }

    @Test func quietHoursOffMeansNeverQuiet() {
        var s = scheduler(settings(), start: "21:50")
        #expect(!s.isQuiet(at("23:00")))
        do {
            let ok = s.tick(now: at("22:05"))
            #expect(ok)
        }
    }

    @Test func equalStartAndEndIsNeverQuiet() {
        let s = scheduler(settings(quiet: ("08:00", "08:00")), start: "07:00")
        #expect(!s.isQuiet(at("08:00")))
        #expect(!s.isQuiet(at("20:00")))
    }

    @Test func quietHoursUseLocalTime() {
        // UTC+2: 20:30 UTC is 22:30 local, inside 22:00-08:00; the range
        // ends at 08:00 local = 06:00 UTC.
        let s = scheduler(settings(quiet: ("22:00", "08:00")), start: "20:15", offset: 7200)
        #expect(s.isQuiet(at("20:30")))
        #expect(!s.isQuiet(at("19:30")))
        #expect(s.nextFire == at("06:00", day: 1))
    }

    // MARK: AC-4 next word now

    @Test func nextWordNowShowsAtOnce() {
        var s = scheduler(settings(), start: "10:00")
        do {
            let ok = s.nextWordNow(now: at("10:01"))
            #expect(ok)
        }
        #expect(s.phase == .showing)
    }

    @Test func nextWordNowWorksDuringAPause() {
        var s = scheduler(settings(), start: "10:00")
        s.pauseForAnHour(now: at("10:01"))
        do {
            let ok = s.nextWordNow(now: at("10:02"))
            #expect(ok)
        }
        s.popupClosed(now: at("10:03"))
        // The pause still holds the regular popups.
        #expect(s.isPaused(now: at("10:20")))
        do {
            let ok = !s.tick(now: at("10:18"))
            #expect(ok)
        }
    }

    @Test func nextWordNowWorksDuringQuietHours() {
        var s = scheduler(settings(quiet: ("22:00", "08:00")), start: "22:30")
        do {
            let ok = s.nextWordNow(now: at("23:00"))
            #expect(ok)
        }
    }

    @Test func nextWordNowRestartsTheInterval() {
        var s = scheduler(settings(), start: "10:00")
        do {
            let ok = s.nextWordNow(now: at("10:10"))
            #expect(ok)
        }
        s.popupClosed(now: at("10:11"))
        #expect(s.nextFire == at("10:26"))
        do {
            let ok = !s.tick(now: at("10:15"))
            #expect(ok)
        }
    }

    // MARK: AC-5 sleep and absence

    @Test func afterSleepAtMostOnePopupOneIntervalAfterWake() {
        var s = scheduler(settings(), start: "10:00")
        // Asleep 10:05-13:00: five popups were missed; none is shown at wake.
        do {
            let ok = !s.tick(now: at("13:00"))
            #expect(ok)
        }
        #expect(s.nextFire == at("13:15"))
        do {
            let ok = !s.tick(now: at("13:10"))
            #expect(ok)
        }
        do {
            let ok = s.tick(now: at("13:15"))
            #expect(ok)
        }
        do {
            let ok = !s.tick(now: at("13:16"))
            #expect(ok)
        }
    }

    @Test func didWakeRestartsTheWait() {
        var s = scheduler(settings(), start: "10:00")
        s.didWake(now: at("10:12"))
        #expect(s.nextFire == at("10:27"))
    }

    @Test func slightlyLateTimerStillShows() {
        var s = scheduler(settings(), start: "10:00")
        do {
            let ok = s.tick(now: at("10:15", plusSeconds: 59))
            #expect(ok)
        }
    }

    @Test func lateTimerAfterQuietHoursWaitsOneInterval() {
        var s = scheduler(settings(quiet: ("22:00", "08:00")), start: "21:50")
        // The machine slept through the night and wakes at 09:30.
        do {
            let ok = !s.tick(now: at("09:30", day: 1))
            #expect(ok)
        }
        #expect(s.nextFire == at("09:45", day: 1))
    }
}
