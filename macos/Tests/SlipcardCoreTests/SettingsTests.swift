import Foundation
import Testing
import SlipcardCore

@Suite struct SettingsTests {
    /// A fresh directory per test, removed afterwards.
    final class TempDir {
        let url: URL
        init() throws {
            url = FileManager.default.temporaryDirectory
                .appendingPathComponent("slipcard-settings-\(UUID().uuidString)")
            try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        }
        deinit { try? FileManager.default.removeItem(at: url) }
        var settings: URL { url.appendingPathComponent("sub").appendingPathComponent("settings.json") }
    }

    func parse(_ json: String) -> SettingsLoad { SettingsStore.parse(Data(json.utf8)) }

    func object(at url: URL) throws -> [String: Any] {
        try JSONSerialization.jsonObject(with: Data(contentsOf: url)) as! [String: Any]
    }

    // MARK: defaults and the shared example

    @Test func missingFileGivesDefaults() throws {
        let dir = try TempDir()
        let load = SettingsStore.load(from: dir.settings)
        #expect(load == SettingsLoad(settings: .defaults, file: .missing, problems: []))
    }

    @Test func emptyObjectGivesDefaults() {
        #expect(parse("{}") == SettingsLoad(settings: .defaults, file: .valid, problems: []))
    }

    /// docs/settings.example.json lists every key at its default, and is
    /// read by the Linux tests too, so the platforms cannot drift apart.
    @Test func sharedExampleHasEveryKeyAtItsDefault() throws {
        let example = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().appendingPathComponent("docs/settings.example.json")
        let load = SettingsStore.load(from: example)
        #expect(load == SettingsLoad(settings: .defaults, file: .valid, problems: []))
        let keys = Set(try object(at: example).keys)
        #expect(keys == Set(Settings.Key.allCases.map(\.rawValue)))
    }

    // MARK: parsing

    @Test func readsEveryKey() {
        let load = parse(
            """
            {"ankiConnectUrl": "https://anki.example:443", "ankiConnectKey": "k",
             "cardQuery": "deck:English is:due", "intervalMinutes": 10, "countdownSeconds": 45,
             "quietHoursEnabled": true, "quietHoursStart": "23:30", "quietHoursEnd": "07:05"}
            """)
        #expect(load.file == .valid)
        #expect(load.problems.isEmpty)
        #expect(
            load.settings
                == Settings(
                    ankiConnectURL: "https://anki.example:443", ankiConnectKey: "k",
                    cardQuery: "deck:English is:due", intervalMinutes: 10, countdownSeconds: 45,
                    quietHoursEnabled: true, quietHoursStart: ClockTime(hour: 23, minute: 30)!,
                    quietHoursEnd: ClockTime(hour: 7, minute: 5)!))
    }

    @Test func missingKeysKeepTheirDefaults() {
        let load = parse(#"{"intervalMinutes": 5}"#)
        var expected = Settings.defaults
        expected.intervalMinutes = 5
        #expect(load.settings == expected)
        #expect(load.problems.isEmpty)
    }

    @Test func invalidValuesFallBackOneByOne() {
        let load = parse(
            """
            {"ankiConnectUrl": "ftp://x", "ankiConnectKey": 5, "cardQuery": "  ",
             "intervalMinutes": 0, "countdownSeconds": 2.5, "quietHoursEnabled": "yes",
             "quietHoursStart": "24:00", "quietHoursEnd": "7:00"}
            """)
        #expect(load.file == .valid)
        #expect(load.settings == .defaults)
        #expect(load.problems.count == 8)
        #expect(load.problems.contains("intervalMinutes: expected a whole number from 1 to 1440; using 15"))
    }

    @Test func booleansAreNotNumbersAndNumbersAreNotBooleans() {
        let load = parse(#"{"intervalMinutes": true, "quietHoursEnabled": 1}"#)
        #expect(load.settings == .defaults)
        #expect(load.problems.count == 2)
    }

    @Test func emptyOrNullKeyMeansNoKey() {
        #expect(parse(#"{"ankiConnectKey": ""}"#).settings.ankiConnectKey == nil)
        #expect(parse(#"{"ankiConnectKey": null}"#).settings.ankiConnectKey == nil)
        #expect(parse(#"{"ankiConnectKey": null}"#).problems.isEmpty)
    }

    @Test(arguments: ["not json", "[1, 2]", "\"text\"", ""])
    func notAnObjectIsAnInvalidFile(json: String) {
        let load = parse(json)
        #expect(load.settings == .defaults)
        #expect(load.file == .invalid(reason: "not a JSON object"))
    }

    @Test func onlyAnInvalidFileWarnsInTheMenu() {
        #expect(parse("oops").menuWarning == "Settings file is invalid: using the defaults")
        #expect(parse(#"{"intervalMinutes": 0}"#).menuWarning == nil)
        #expect(parse("{}").menuWarning == nil)
        #expect(SettingsLoad(settings: .defaults, file: .missing, problems: []).menuWarning == nil)
    }

    @Test func clockTimeNeedsTwoDigitsEach() {
        #expect(ClockTime("00:00") == ClockTime(hour: 0, minute: 0))
        #expect(ClockTime("23:59")?.description == "23:59")
        for bad in ["24:00", "12:60", "7:00", "07:5", "+1:00", "ab:cd", "0700", "07:00:00"] {
            #expect(ClockTime(bad) == nil, "\(bad)")
        }
    }

    // MARK: saving

    @Test func saveThenLoadRoundTrips() throws {
        let dir = try TempDir()
        var s = Settings.defaults
        s.intervalMinutes = 30
        s.ankiConnectKey = "secret"
        s.quietHoursEnabled = true
        try SettingsStore.save(s, to: dir.settings)
        let load = SettingsStore.load(from: dir.settings)
        #expect(load == SettingsLoad(settings: s, file: .valid, problems: []))
    }

    @Test func saveKeepsKeysItDoesNotKnow() throws {
        let dir = try TempDir()
        try FileManager.default.createDirectory(
            at: dir.settings.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data(#"{"intervalMinutes": 5, "futureOption": {"a": [1, 2]}, "note": "mine"}"#.utf8)
            .write(to: dir.settings)
        var s = SettingsStore.load(from: dir.settings).settings
        s.quietHoursEnabled = true
        try SettingsStore.save(s, to: dir.settings)
        let saved = try object(at: dir.settings)
        #expect(saved["note"] as? String == "mine")
        #expect((saved["futureOption"] as? [String: Any])?["a"] as? [Int] == [1, 2])
        #expect(saved["intervalMinutes"] as? Int == 5)
        #expect(saved["quietHoursEnabled"] as? Bool == true)
    }

    @Test func saveRefusesAnInvalidFileAndLeavesItAlone() throws {
        let dir = try TempDir()
        try FileManager.default.createDirectory(
            at: dir.settings.deletingLastPathComponent(), withIntermediateDirectories: true)
        let original = Data(#"{"intervalMinutes": 5,, oops"#.utf8)
        try original.write(to: dir.settings)
        #expect(throws: SettingsError.fileIsInvalid(reason: "not a readable JSON object")) {
            try SettingsStore.save(.defaults, to: dir.settings)
        }
        #expect(try Data(contentsOf: dir.settings) == original)
    }

    @Test func saveCreatesTheFolderAndWritesEveryKey() throws {
        let dir = try TempDir()
        try SettingsStore.save(.defaults, to: dir.settings)
        let keys = Set(try object(at: dir.settings).keys)
        #expect(keys == Set(Settings.Key.allCases.map(\.rawValue)))
    }

    @Test func savedFileIsLikeTheSharedExample() throws {
        let dir = try TempDir()
        try SettingsStore.save(.defaults, to: dir.settings)
        let saved = try object(at: dir.settings)
        #expect(saved["ankiConnectKey"] is NSNull)
        #expect(saved["quietHoursStart"] as? String == "22:00")
        #expect(saved["ankiConnectUrl"] as? String == "http://127.0.0.1:8765")
    }

    // MARK: location

    @Test func environmentOverridesThePath() {
        let url = SettingsStore.defaultURL(environment: ["SLIPCARD_SETTINGS": "/tmp/x/s.json"])
        #expect(url.path == "/tmp/x/s.json")
    }

    @Test func defaultPathIsInApplicationSupport() {
        let path = SettingsStore.defaultURL(environment: [:]).path
        #expect(path.hasSuffix("/Library/Application Support/Slipcard/settings.json"))
    }
}
