import AppKit
import SlipcardCore

/// The card popup: a borderless, non-activating panel that floats over every
/// Space, full-screen apps included, and leaves keyboard focus where it was.
/// It renders a CardPopupModel and forwards its buttons to it.
@MainActor
final class PopupPanel: NSPanel {
    static let width: CGFloat = 460
    static let minHeight: CGFloat = 220
    static let textWidth: CGFloat = 400

    var onClose: (() -> Void)?

    private let model: CardPopupModel
    private let wordLabel: NSTextField
    private let answerLabel: NSTextField
    private let errorLabel = NSTextField(wrappingLabelWithString: "")
    private let showButton = NSButton()
    private let closeButton = NSButton()
    private var gradeButtons: [NSButton] = []
    private let gradeRow = NSStackView()
    private let stack = NSStackView()
    private var screenFrame: CGRect?
    /// Ticks the Close button's countdown once a second while it runs.
    private var countdownTimer: Timer?

    init(model: CardPopupModel) {
        self.model = model
        wordLabel = NSTextField(wrappingLabelWithString: model.front)
        answerLabel = NSTextField(wrappingLabelWithString: model.back)
        super.init(
            contentRect: CGRect(origin: .zero, size: CGSize(width: Self.width, height: Self.minHeight)),
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

        contentView = makeContent()
        // The pointer moving over the popup is a reaction: it stops the
        // countdown. .activeAlways because this app is never active.
        contentView?.addTrackingArea(NSTrackingArea(
            rect: .zero, options: [.mouseMoved, .mouseEnteredAndExited, .activeAlways, .inVisibleRect],
            owner: self, userInfo: nil))
        model.onChange = { [weak self] in self?.render() }
        model.onClose = { [weak self] in self?.close() }
        render()
    }

    // A non-activating panel can still become key when clicked, which lets
    // its buttons work; it never becomes main, so the app behind keeps its
    // place. Showing it does not make it key (see present()).
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    func present() {
        let screen = NSScreen.screens.first { $0.frame.contains(NSEvent.mouseLocation) }
            ?? NSScreen.main
        screenFrame = screen?.visibleFrame
        fit()
        // orderFrontRegardless, not makeKeyAndOrderFront: shown without
        // becoming key and without activating this app.
        orderFrontRegardless()
        startCountdown()
    }

    override func close() {
        countdownTimer?.invalidate()
        countdownTimer = nil
        super.close()
        onClose?()
        onClose = nil
    }

    /// One tick a second from the moment the countdown started, so the
    /// title changes on whole seconds; at zero the model closes the popup.
    private func startCountdown() {
        let first = model.countdown.started.addingTimeInterval(1)
        let timer = Timer(fire: first, interval: 1, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self else { return }
                self.model.tick(now: Date())
                if self.model.countdown.isStopped || self.model.stage == .closed {
                    self.countdownTimer?.invalidate()
                    self.countdownTimer = nil
                }
                self.render()
            }
        }
        RunLoop.main.add(timer, forMode: .common)
        countdownTimer = timer
    }

    // Only movement counts: a pointer resting where the popup appears sends
    // no mouseMoved, so it does not stop the countdown.
    override func mouseMoved(with event: NSEvent) { model.pointerMoved() }
    override func mouseEntered(with event: NSEvent) { model.pointerMoved() }

    /// Sizes the panel to its content (at least the PoC size) and centres
    /// it on its screen; PopupLayout shrinks it to the screen if needed.
    private func fit() {
        contentView?.layoutSubtreeIfNeeded()
        let height = max(Self.minHeight, stack.fittingSize.height + 48)
        let size = CGSize(width: Self.width, height: height)
        if let screenFrame {
            setFrame(PopupLayout.centeredFrame(size: size, in: screenFrame), display: true)
        } else {
            setContentSize(size)
        }
    }

    private func render() {
        showButton.isHidden = model.showsAnswer
        answerLabel.isHidden = !model.showsAnswer
        gradeRow.isHidden = !model.showsGrades
        for button in gradeButtons { button.isEnabled = !model.isSending }
        errorLabel.stringValue = model.failure ?? ""
        errorLabel.isHidden = model.failure == nil
        closeButton.title = model.closeLabel(at: Date())
        if isVisible { fit() }
    }

    private func label(_ field: NSTextField, size: CGFloat, weight: NSFont.Weight, lines: Int) {
        field.font = .systemFont(ofSize: size, weight: weight)
        field.alignment = .center
        field.preferredMaxLayoutWidth = Self.textWidth
        field.maximumNumberOfLines = lines
        field.lineBreakMode = .byWordWrapping
        field.cell?.truncatesLastVisibleLine = true
    }

    private func button(_ title: String, _ action: Selector, tag: Int = 0) -> NSButton {
        let b = NSButton(title: title, target: self, action: action)
        b.bezelStyle = .rounded
        b.tag = tag
        return b
    }

    private func makeContent() -> NSView {
        let background = NSVisualEffectView()
        background.material = .hudWindow
        background.state = .active
        background.wantsLayer = true
        background.layer?.cornerRadius = 14
        background.layer?.masksToBounds = true

        label(wordLabel, size: 30, weight: .semibold, lines: 4)
        label(answerLabel, size: 15, weight: .regular, lines: 12)
        label(errorLabel, size: 12, weight: .regular, lines: 3)
        errorLabel.textColor = .systemRed

        showButton.title = "Show"
        showButton.bezelStyle = .rounded
        showButton.target = self
        showButton.action = #selector(reveal)
        closeButton.title = model.closeLabel(at: Date())
        closeButton.bezelStyle = .rounded
        closeButton.target = self
        closeButton.action = #selector(closeClicked)

        gradeButtons = [("Again", Ease.again), ("Hard", .hard), ("Good", .good), ("Easy", .easy)]
            .map { button($0.0, #selector(gradeClicked(_:)), tag: $0.1.rawValue) }
        gradeRow.setViews(gradeButtons, in: .center)
        gradeRow.spacing = 8

        let buttons = NSStackView(views: [showButton, gradeRow, closeButton])
        buttons.spacing = 12
        stack.setViews([wordLabel, answerLabel, errorLabel, buttons], in: .center)
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

    @objc private func reveal() { model.reveal() }

    @objc private func gradeClicked(_ sender: NSButton) {
        guard let ease = Ease(rawValue: sender.tag) else { return }
        Task { await model.grade(ease) }
    }

    @objc private func closeClicked() { model.close() }
}
