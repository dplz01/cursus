import AppKit
import SwiftUI

/// The message field: multi-line, grows up to six lines, then scrolls.
/// Return sends or starts a new line depending on Settings → Appearance →
/// Typing ("Enter sends messages"); Shift+Return / ⌘Return do the other.
/// Esc cancels a reply or edit. An NSTextView, because SwiftUI on macOS 12
/// has no multi-line field with key handling.
struct ComposerField: NSViewRepresentable {
    @Binding var text: String
    @Binding var height: CGFloat
    let enterSends: Bool
    /// Changes when the field should take the keyboard focus.
    let focusToken: Int
    let onSend: () -> Void
    let onEscape: () -> Void

    static let maxLines: CGFloat = 6

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> NSScrollView {
        let textView = KeyTextView()
        textView.isRichText = false
        textView.importsGraphics = false
        textView.allowsUndo = true
        textView.drawsBackground = false
        textView.isAutomaticQuoteSubstitutionEnabled = false
        textView.isAutomaticDashSubstitutionEnabled = false
        textView.isAutomaticTextReplacementEnabled = false
        textView.font = Self.font
        textView.textColor = .labelColor
        textView.textContainerInset = NSSize(width: 0, height: 2)
        textView.textContainer?.widthTracksTextView = true
        textView.textContainer?.containerSize = NSSize(width: 0, height: CGFloat.greatestFiniteMagnitude)
        textView.minSize = .zero
        textView.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]
        textView.delegate = context.coordinator
        textView.onKey = { [weak coordinator = context.coordinator] key in
            coordinator?.handle(key) ?? false
        }

        let scroll = NSScrollView()
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.borderType = .noBorder
        scroll.documentView = textView
        context.coordinator.textView = textView
        textView.string = text
        DispatchQueue.main.async { context.coordinator.updateHeight() }
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        let coordinator = context.coordinator
        coordinator.parent = self
        guard let textView = coordinator.textView else { return }
        if textView.string != text {
            textView.string = text
            DispatchQueue.main.async { coordinator.updateHeight() }
        }
        if coordinator.focusToken != focusToken {
            coordinator.focusToken = focusToken
            DispatchQueue.main.async { textView.window?.makeFirstResponder(textView) }
        }
    }

    static var font: NSFont { .systemFont(ofSize: 14 * Prefs.currentTextSize.scale) }

    /// One line of text, for the empty field's height.
    static var lineHeight: CGFloat {
        let font = Self.font
        return ceil(font.ascender - font.descender + font.leading) + 4
    }

    final class Coordinator: NSObject, NSTextViewDelegate {
        var parent: ComposerField
        weak var textView: KeyTextView?
        var focusToken = -1

        init(_ parent: ComposerField) {
            self.parent = parent
        }

        func textDidChange(_ notification: Notification) {
            guard let textView = textView else { return }
            parent.text = textView.string
            updateHeight()
        }

        /// Return / Esc. `true` = handled here.
        func handle(_ key: KeyTextView.Key) -> Bool {
            switch key {
            case .escape:
                parent.onEscape()
                return true
            case .returnKey(let modifiers):
                let send: Bool
                if parent.enterSends {
                    send = !modifiers.contains(.shift) && !modifiers.contains(.option)
                } else {
                    send = modifiers.contains(.command)
                }
                if send {
                    parent.onSend()
                    return true
                }
                textView?.insertNewlineIgnoringFieldEditor(nil)
                return true
            }
        }

        func updateHeight() {
            guard let textView = textView, let container = textView.textContainer,
                  let layout = textView.layoutManager else { return }
            layout.ensureLayout(for: container)
            let used = layout.usedRect(for: container).height + textView.textContainerInset.height * 2
            let line = ComposerField.lineHeight
            let height = min(max(ceil(used), line), line * ComposerField.maxLines)
            if abs(parent.height - height) > 0.5 {
                parent.height = height
            }
        }
    }
}

/// NSTextView that reports Return (with its modifiers) and Esc first.
final class KeyTextView: NSTextView {
    enum Key {
        case returnKey(NSEvent.ModifierFlags)
        case escape
    }

    var onKey: ((Key) -> Bool)?

    override func keyDown(with event: NSEvent) {
        // While an input method is composing (accents, CJK), keys are its.
        if hasMarkedText() {
            super.keyDown(with: event)
            return
        }
        let modifiers = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        switch event.keyCode {
        case 36, 76: // Return, keypad Enter
            if onKey?(.returnKey(modifiers)) == true { return }
        case 53: // Esc
            if onKey?(.escape) == true { return }
        default:
            break
        }
        super.keyDown(with: event)
    }
}
