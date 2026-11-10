import Foundation
import Testing
@testable import SlipcardCore

@Suite struct TrayMenuTests {
    /// UTC+2, so a time in the menu reads two hours after UTC.
    let plusTwo: @Sendable (Date) -> Int = { _ in 7200 }
    /// 2026-11-10 10:00 UTC (12:00 at UTC+2).
    let ten = Date(timeIntervalSince1970: 1_794_304_800)

    func menu(_ state: TrayState) -> TrayMenu { TrayMenu(state, utcOffset: plusTwo) }

    func titles(_ m: TrayMenu) -> [String] { m.items.map(\.title) }

    func item(_ m: TrayMenu, _ title: String) -> TrayMenu.Item? { m.items.first { $0.title == title } }

    func interval(_ m: TrayMenu) -> [TrayMenu.Item] {
        guard case .submenu(let items)? = item(m, "Interval")?.kind else { return [] }
        return items
    }

    // MARK: controls (AC-1)

    @Test func menuOffersEveryControl() {
        let m = menu(TrayState(anki: .connected, dueCount: 3))
        for title in [
            "Next word now", "Next word in 5 s", "Pause for an hour", "Quiet hours 22:00–08:00", "Interval",
            "Open settings file", "Quit Slipcard",
        ] {
            #expect(item(m, title) != nil, "\(title)")
        }
        #expect(item(m, "Next word now")?.kind == .action(.nextWordNow))
        #expect(item(m, "Open settings file")?.kind == .action(.openSettings))
        #expect(item(m, "Quit Slipcard")?.kind == .action(.quit))
    }

    // MARK: start at login

    @Test func startAtLoginIsASwitchShowingItsState() {
        #expect(item(menu(TrayState(startAtLogin: .off)), "Start at login")?.kind
            == .toggle(.toggleStartAtLogin, checked: false))
        #expect(item(menu(TrayState(startAtLogin: .on)), "Start at login")?.kind
            == .toggle(.toggleStartAtLogin, checked: true))
        // It sits between Open settings file and Quit.
        let titles = menu(TrayState(startAtLogin: .on)).items.map(\.title)
        let i = titles.firstIndex(of: "Start at login")!
        #expect(titles[i - 1] == "Open settings file" && titles[i + 1] == "Quit Slipcard")
    }

    @Test func startAtLoginNeedingApprovalSaysSo() {
        #expect(item(menu(TrayState(startAtLogin: .needsApproval)), "Start at login: allow it in System Settings")?.kind
            == .toggle(.toggleStartAtLogin, checked: true))
    }

    @Test func startAtLoginOutsideTheInstalledAppIsOnlyALine() {
        let m = menu(TrayState())
        #expect(item(m, "Start at login: only from the installed app")?.kind == .info)
        #expect(item(m, "Start at login") == nil)
    }

    @Test func aLoginProblemIsAStateLine() {
        let m = menu(TrayState(anki: .connected, loginProblem: "Start at login: no"))
        #expect(m.stateLines.first == "Start at login: no")
        #expect(item(m, "Start at login: no")?.kind == .info)
    }

    @Test func loginItemFollowsTheServiceStatus() {
        // SMAppService.Status: notRegistered 0, enabled 1, requiresApproval 2, notFound 3.
        #expect([0, 1, 2, 3, 9].map(LoginItem.init(serviceStatus:)) == [.off, .on, .needsApproval, .unavailable, .unavailable])
    }

    @Test func startAtLoginIsNotASettingsChange() {
        #expect(TrayAction(commandLine: "start-at-login") == .toggleStartAtLogin)
        #expect(TrayAction.toggleStartAtLogin.applied(to: .defaults) == nil)
    }

    @Test func pauseBecomesResumeWhilePaused() {
        let paused = menu(TrayState(pausedUntil: ten))
        #expect(item(paused, "Resume")?.kind == .action(.resume))
        #expect(item(paused, "Pause for an hour") == nil)
        #expect(item(menu(TrayState()), "Pause for an hour")?.kind == .action(.pause))
    }

    @Test func intervalChoicesAreTheAgreedOnesWithTheCurrentChecked() {
        let items = interval(menu(TrayState()))
        #expect(items.map(\.title) == ["5 min", "10 min", "15 min", "30 min", "60 min"])
        #expect(items.filter { if case .toggle(_, true) = $0.kind { true } else { false } }.map(\.title) == ["15 min"])
        #expect(items[3].kind == .toggle(.setInterval(minutes: 30), checked: false))
    }

    @Test func aCustomIntervalFromTheFileIsListedAndChecked() {
        var s = Settings.defaults
        s.intervalMinutes = 20
        let items = interval(menu(TrayState(settings: s)))
        #expect(items.map(\.title) == ["5 min", "10 min", "15 min", "20 min", "30 min", "60 min"])
        #expect(items[3].kind == .toggle(.setInterval(minutes: 20), checked: true))
    }

    @Test func quietHoursSwitchShowsItsStateAndRange() {
        var s = Settings.defaults
        #expect(item(menu(TrayState(settings: s)), "Quiet hours 22:00–08:00")?.kind
            == .toggle(.toggleQuietHours, checked: false))
        s.quietHoursEnabled = true
        s.quietHoursStart = ClockTime(hour: 23, minute: 30)!
        s.quietHoursEnd = ClockTime(hour: 7, minute: 0)!
        #expect(item(menu(TrayState(settings: s)), "Quiet hours 23:30–07:00")?.kind
            == .toggle(.toggleQuietHours, checked: true))
    }

    // MARK: state

    @Test func connectedShowsTheDueCount() {
        #expect(menu(TrayState(anki: .connected, dueCount: 0)).stateLines == ["Connected to Anki", "No cards due"])
        #expect(menu(TrayState(anki: .connected, dueCount: 1)).stateLines == ["Connected to Anki", "1 card due"])
        #expect(menu(TrayState(anki: .connected, dueCount: 12)).stateLines == ["Connected to Anki", "12 cards due"])
    }

    @Test func anUnknownCountShowsNoCountLine() {
        #expect(menu(TrayState(anki: .connected, dueCount: nil)).stateLines == ["Connected to Anki"])
        #expect(menu(TrayState()).stateLines == ["Not connected yet"])
    }

    @Test func ankiNotReachableShowsTheStateAndDimsTheIcon() {
        let m = menu(TrayState(anki: .failed(.notReachable("x")), dueCount: 5))
        #expect(m.stateLines == ["Anki is not running or not reachable"])
        #expect(m.iconDimmed)
        #expect(menu(TrayState(anki: .failed(.wrongKey))).iconDimmed)
        #expect(menu(TrayState(anki: .failed(.timedOut))).iconDimmed)
    }

    @Test func connectedOrNotYetCheckedKeepsTheIcon() {
        #expect(!menu(TrayState(anki: .connected, dueCount: 0)).iconDimmed)
        #expect(!menu(TrayState()).iconDimmed)
    }

    @Test func aPauseShowsUntilWhenInLocalTime() {
        let m = menu(TrayState(anki: .connected, dueCount: 2, pausedUntil: ten))
        #expect(m.stateLines == ["Connected to Anki", "2 cards due", "Paused until 12:00"])
    }

    @Test func quietHoursShowUntilWhenInLocalTime() {
        let m = menu(TrayState(anki: .connected, quietUntil: ten.addingTimeInterval(-4 * 3600)))
        #expect(m.stateLines == ["Connected to Anki", "Quiet hours until 08:00"])
    }

    @Test func anInvalidSettingsFileIsTheFirstLine() {
        let load = SettingsStore.parse(Data("oops".utf8))
        let m = menu(TrayState(anki: .connected, dueCount: 1, settingsWarning: load.menuWarning))
        #expect(m.stateLines.first == "Settings file is invalid: using the defaults")
        #expect(m.items.first?.kind == .info)
    }

    @Test func aChangeThatCouldNotBeSavedIsShown() {
        let m = menu(TrayState(saveProblem: "Not saved: the settings file is invalid"))
        #expect(m.stateLines.first == "Not saved: the settings file is invalid")
    }

    @Test func stateLinesComeFirstAsInfoItems() {
        let m = menu(TrayState(anki: .connected, dueCount: 4, pausedUntil: ten))
        let infos = m.items.prefix { $0.kind == .info }.map(\.title)
        #expect(infos == m.stateLines)
        #expect(m.items[infos.count] == .separator)
    }

    // MARK: actions and settings (AC-3)

    @Test func traySettingsActionsChangeOnlyTheirSetting() {
        let s = Settings.defaults
        #expect(TrayAction.toggleQuietHours.applied(to: s)?.quietHoursEnabled == true)
        var thirty = s
        thirty.intervalMinutes = 30
        #expect(TrayAction.setInterval(minutes: 30).applied(to: s) == thirty)
        #expect(TrayAction.setInterval(minutes: 15).applied(to: s) == nil)  // already 15
        #expect(TrayAction.setInterval(minutes: 0).applied(to: s) == nil)
        for action in [TrayAction.nextWordNow, .nextWordSoon, .pause, .resume, .openSettings, .quit] {
            #expect(action.applied(to: s) == nil, "\(action)")
        }
    }

    @Test func commandLineNamesTheSettingsActions() {
        #expect(TrayAction(commandLine: "interval:30") == .setInterval(minutes: 30))
        #expect(TrayAction(commandLine: "quiet-hours") == .toggleQuietHours)
        #expect(TrayAction(commandLine: "pause") == nil)
        #expect(TrayAction(commandLine: "interval:soon") == nil)
    }

    @Test func aSavedChangeSurvivesARestartAndKeepsUnknownKeys() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("slipcard-tray-\(UUID())")
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("settings.json")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try Data(#"{"cardQuery": "deck:English is:due", "mine": [1]}"#.utf8).write(to: url)
        var s = SettingsStore.load(from: url).settings
        for action in [TrayAction.setInterval(minutes: 30), .toggleQuietHours] {
            s = try #require(action.applied(to: s))
            try SettingsStore.save(s, to: url)
        }
        let restarted = SettingsStore.load(from: url)
        #expect(restarted.settings.intervalMinutes == 30)
        #expect(restarted.settings.quietHoursEnabled)
        #expect(restarted.settings.cardQuery == "deck:English is:due")
        let raw = try JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any]
        #expect(raw?["mine"] as? [Int] == [1])
    }

    @Test func aChangeToAnInvalidFileAppliesButIsNotSaved() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("slipcard-tray-\(UUID())")
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("settings.json")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let broken = Data(#"{"intervalMinutes": 10,, oops"#.utf8)
        try broken.write(to: url)
        let r = try #require(TraySettingsChange.perform(.setInterval(minutes: 30), on: .defaults, file: url))
        #expect(r.settings.intervalMinutes == 30)
        #expect(r.saveProblem == "Not saved: the settings file is invalid")
        #expect(try Data(contentsOf: url) == broken)
        #expect(TraySettingsChange.perform(.pause, on: .defaults, file: url) == nil)
    }

    // MARK: the tray state from the popup flow

    @Test func trayStateFollowsThePauseAndResume() {
        var flow = PopupFlow(scheduler: Scheduler(settings: .defaults, now: ten, utcOffset: plusTwo))
        flow.pauseForAnHour(now: ten)
        let paused = flow.trayState(now: ten, dueCount: 3, settings: .defaults, settingsWarning: nil, saveProblem: nil)
        #expect(paused.pausedUntil == ten.addingTimeInterval(3600))
        #expect(menu(paused).stateLines.last == "Paused until 13:00")
        flow.resume(now: ten.addingTimeInterval(60))
        #expect(flow.trayState(now: ten, dueCount: 3, settings: .defaults, settingsWarning: nil, saveProblem: nil)
            .pausedUntil == nil)
    }

    @Test func trayStateShowsQuietHoursOnlyInsideThem() {
        var s = Settings.defaults
        s.quietHoursEnabled = true  // 22:00–08:00 local, UTC+2
        let flow = PopupFlow(scheduler: Scheduler(settings: s, now: ten, utcOffset: plusTwo))
        let night = Date(timeIntervalSince1970: 1_794_272_400)  // 01:00 UTC = 03:00 local
        let state = flow.trayState(now: night, dueCount: nil, settings: s, settingsWarning: nil, saveProblem: nil)
        #expect(menu(state).stateLines.last == "Quiet hours until 08:00")
        #expect(flow.trayState(now: ten, dueCount: nil, settings: s, settingsWarning: nil, saveProblem: nil)
            .quietUntil == nil)
    }

    @Test func applyingNewSettingsRestartsTheWait() {
        var flow = PopupFlow(scheduler: Scheduler(settings: .defaults, now: ten))
        var s = Settings.defaults
        s.intervalMinutes = 30
        flow.apply(s, now: ten.addingTimeInterval(600))
        #expect(flow.nextFire == ten.addingTimeInterval(600 + 1800))
    }

    @Test func aCountCheckSetsTheConnectionState() {
        var flow = PopupFlow(scheduler: Scheduler(settings: .defaults, now: ten))
        flow.connectionChecked(nil)
        #expect(flow.anki == .connected)
        flow.connectionChecked(AnkiConnectError.notReachable("x"))
        #expect(flow.anki == .failed(.notReachable("x")))
    }

    @Test func dueCardsGivesTheNextCardAndTheCount() async throws {
        let server = try StubAnkiServer(replies: [
            "findCards": try recorded("findCards"),
            "cardsInfo": try recorded("cardsInfo"),
        ])
        let client = AnkiConnectClient(url: server.url, key: nil)
        let due = try await client.dueCards(query: "is:due")
        #expect(due.count == 3)
        #expect(due.next?.queue == 1)
        #expect(try await client.dueCount(query: "is:due") == 3)
        #expect(server.actions == ["findCards", "cardsInfo", "findCards"])
    }
}
