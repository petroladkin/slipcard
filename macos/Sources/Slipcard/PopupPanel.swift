import AppKit
import SlipcardCore

/// The card popup: a borderless, non-activating panel that floats over every
/// Space, full-screen apps included, and leaves keyboard focus where it was.
@MainActor
final class PopupPanel: NSPanel {
    static let size = CGSize(width: 420, height: 220)

    var onClose: (() -> Void)?

    private let closeButton = NSButton()
    private let answerLabel: NSTextField
    private let showButton = NSButton()

    init(word: String, answer: String) {
        answerLabel = NSTextField(wrappingLabelWithString: answer)
        super.init(
            contentRect: CGRect(origin: .zero, size: Self.size),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false)

        // Over normal windows, and with fullScreenAuxiliary allowed onto a
        // full-screen app's Space; canJoinAllSpaces keeps it on whichever
        // Space is current instead of switching to the one it was made on.
        level = .floating
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle]
        hidesOnDeactivate = false
        isReleasedWhenClosed = false
        isMovableByWindowBackground = true
        backgroundColor = .clear
        isOpaque = false
        hasShadow = true

        contentView = makeContent(word: word)
    }

    // A non-activating panel can still become key when clicked, which lets
    // its buttons work; it never becomes main, so the app behind keeps its
    // place. Showing it does not make it key (see present()).
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    func present() {
        let screen = NSScreen.screens.first { $0.frame.contains(NSEvent.mouseLocation) }
            ?? NSScreen.main
        if let screen {
            setFrame(PopupLayout.centeredFrame(size: Self.size, in: screen.visibleFrame), display: false)
        }
        // orderFrontRegardless, not makeKeyAndOrderFront: shown without
        // becoming key and without activating this app.
        orderFrontRegardless()
    }

    override func close() {
        super.close()
        onClose?()
        onClose = nil
    }

    private func makeContent(word: String) -> NSView {
        let background = NSVisualEffectView()
        background.material = .hudWindow
        background.state = .active
        background.wantsLayer = true
        background.layer?.cornerRadius = 14
        background.layer?.masksToBounds = true

        let wordLabel = NSTextField(labelWithString: word)
        wordLabel.font = .systemFont(ofSize: 30, weight: .semibold)
        wordLabel.alignment = .center

        answerLabel.alignment = .center
        answerLabel.isHidden = true

        showButton.title = "Show"
        showButton.bezelStyle = .rounded
        showButton.target = self
        showButton.action = #selector(reveal)

        closeButton.title = "Close"
        closeButton.bezelStyle = .rounded
        closeButton.target = self
        closeButton.action = #selector(closeClicked)

        let buttons = NSStackView(views: [showButton, closeButton])
        let stack = NSStackView(views: [wordLabel, answerLabel, buttons])
        stack.orientation = .vertical
        stack.spacing = 14
        stack.translatesAutoresizingMaskIntoConstraints = false
        background.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.centerXAnchor.constraint(equalTo: background.centerXAnchor),
            stack.centerYAnchor.constraint(equalTo: background.centerYAnchor),
            stack.widthAnchor.constraint(lessThanOrEqualTo: background.widthAnchor, constant: -40),
        ])
        return background
    }

    @objc private func reveal() {
        answerLabel.isHidden = false
        showButton.isHidden = true
    }

    @objc private func closeClicked() { close() }
}
