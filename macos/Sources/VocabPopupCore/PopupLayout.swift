import CoreGraphics

public enum PopupLayout {
    /// The frame of a popup of `size` centred in `visibleFrame` (a screen's
    /// area without the menu bar and Dock), on whole points. A popup larger
    /// than the area is shrunk to fit it.
    public static func centeredFrame(size: CGSize, in visibleFrame: CGRect) -> CGRect {
        let width = min(size.width, visibleFrame.width)
        let height = min(size.height, visibleFrame.height)
        let x = visibleFrame.minX + ((visibleFrame.width - width) / 2).rounded(.down)
        let y = visibleFrame.minY + ((visibleFrame.height - height) / 2).rounded(.down)
        return CGRect(x: x, y: y, width: width, height: height)
    }
}
