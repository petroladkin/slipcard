import CoreGraphics
import Testing
import VocabPopupCore

@Suite struct PopupLayoutTests {
    @Test func centresInVisibleFrame() {
        let frame = PopupLayout.centeredFrame(
            size: CGSize(width: 400, height: 200),
            in: CGRect(x: 0, y: 25, width: 1440, height: 875))
        #expect(frame == CGRect(x: 520, y: 362, width: 400, height: 200))
    }

    @Test func respectsScreenOrigin() {
        // A second monitor to the left of the main one has a negative origin.
        let frame = PopupLayout.centeredFrame(
            size: CGSize(width: 400, height: 200),
            in: CGRect(x: -1920, y: 0, width: 1920, height: 1080))
        #expect(frame == CGRect(x: -1160, y: 440, width: 400, height: 200))
    }

    @Test func shrinksToFitSmallScreen() {
        let frame = PopupLayout.centeredFrame(
            size: CGSize(width: 800, height: 600),
            in: CGRect(x: 0, y: 0, width: 640, height: 480))
        #expect(frame == CGRect(x: 0, y: 0, width: 640, height: 480))
    }
}
