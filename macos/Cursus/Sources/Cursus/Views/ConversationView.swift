import AppKit
import AVFoundation
import SwiftUI
import UniformTypeIdentifiers

struct ConversationView: View {
    @EnvironmentObject private var model: AppModel
    let chat: Chat
    @State private var dropTargeted = false
    @State private var shownCount = 0

    var body: some View {
        VStack(spacing: 0) {
            if chat.pendingRequestId != nil {
                RequestBanner(chat: chat)
            }
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(spacing: 6) {
                        if model.isLoadingMessages && model.messages.isEmpty {
                            ProgressView().padding(30)
                        }
                        ForEach(model.messages) { message in
                            MessageRow(message: message, showSender: chat.isGroup && !message.outgoing,
                                       inNotes: chat.isNotes)
                                .id(message.id)
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 12)
                }
                .background(Theme.panel)
                .onChange(of: model.messages.last?.id) { _ in
                    // First load jumps; new messages after that animate.
                    let animate = shownCount > 0
                    shownCount = model.messages.count
                    scrollToBottom(proxy, animated: animate)
                }
                .onChange(of: model.composeMode) { mode in
                    if mode != .new { scrollToBottom(proxy, animated: true) }
                }
            }
            Divider()
            ComposerView(chat: chat)
        }
        .navigationTitle(chat.title)
        .navigationSubtitle(subtitle)
        .onDrop(of: [UTType.fileURL], isTargeted: $dropTargeted, perform: drop)
        .overlay {
            if dropTargeted {
                RoundedRectangle(cornerRadius: 12)
                    .strokeBorder(Theme.accent, style: StrokeStyle(lineWidth: 2, dash: [8, 5]))
                    .padding(8)
                    .allowsHitTesting(false)
            }
        }
    }

    private var subtitle: String {
        if chat.isNotes { return "Private notes" }
        if chat.isChannel { return "Channel" }
        if chat.isGroup { return chat.role.map { "Group · \($0)" } ?? "Group" }
        return chat.fullName.isEmpty ? "" : chat.fullName
    }

    private func scrollToBottom(_ proxy: ScrollViewProxy, animated: Bool) {
        guard let last = model.messages.last?.id else { return }
        if animated {
            withAnimation(.easeOut(duration: 0.2)) { proxy.scrollTo(last, anchor: .bottom) }
        } else {
            proxy.scrollTo(last, anchor: .bottom)
        }
    }

    private func drop(_ providers: [NSItemProvider]) -> Bool {
        guard chat.pendingRequestId == nil else { return false }
        let group = DispatchGroup()
        var urls: [URL] = []
        let lock = NSLock()
        for provider in providers where provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) {
            group.enter()
            provider.loadItem(forTypeIdentifier: UTType.fileURL.identifier, options: nil) { item, _ in
                defer { group.leave() }
                guard let data = item as? Data, let url = URL(dataRepresentation: data, relativeTo: nil) else { return }
                lock.lock()
                urls.append(url)
                lock.unlock()
            }
        }
        group.notify(queue: .main) {
            guard !urls.isEmpty else { return }
            Task { await model.sendFiles(urls) }
        }
        return true
    }
}

struct RequestBanner: View {
    @EnvironmentObject private var model: AppModel
    let chat: Chat

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "person.badge.plus").font(.title3).foregroundColor(Theme.accent)
            Text("**\(chat.title)** wants to connect with you.")
            Spacer()
            Button("Reject") { Task { await model.rejectRequest(chat) } }
            Button("Accept") { Task { await model.acceptRequest(chat) } }
                .buttonStyle(.borderedProminent)
        }
        .padding(12)
        .background(Theme.accentSoft)
    }
}

// MARK: - Message bubble

struct MessageRow: View {
    @EnvironmentObject private var model: AppModel
    let message: Message
    let showSender: Bool
    /// Private notes: no reactions, no delivery state.
    let inNotes: Bool

    var body: some View {
        if message.announcement {
            Text(message.text)
                .font(.caption)
                .foregroundColor(Theme.mutedText)
                .multilineTextAlignment(.center)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 4)
        } else {
            HStack(alignment: .bottom, spacing: 6) {
                if message.outgoing {
                    Spacer(minLength: 80)
                } else if showSender {
                    Avatar(image: message.sender?.image, name: message.sender?.name ?? "?", size: 26)
                }
                VStack(alignment: message.outgoing ? .trailing : .leading, spacing: 3) {
                    bubble
                        .contextMenu { MessageMenu(message: message, canReact: !inNotes) }
                    if !message.reactions.isEmpty {
                        ReactionsBar(message: message)
                    }
                }
                if !message.outgoing {
                    Spacer(minLength: 80)
                }
            }
        }
    }

    @ViewBuilder private var bubble: some View {
        if message.emojiOnly {
            VStack(alignment: message.outgoing ? .trailing : .leading, spacing: 0) {
                Text(message.text).font(.system(size: 42))
                footer
            }
        } else {
            VStack(alignment: .leading, spacing: 5) {
                if showSender, let sender = message.sender {
                    Text(sender.name)
                        .font(.caption.weight(.semibold))
                        .foregroundColor(Theme.accent)
                }
                if let quote = message.quote {
                    QuoteView(quote: quote)
                }
                if let file = message.file, message.deleted == nil {
                    AttachmentView(file: file, messageId: message.id)
                }
                if let invite = message.groupInvitation {
                    GroupInviteCard(invite: invite, outgoing: message.outgoing)
                }
                if let deleted = message.deleted {
                    Text(deleted == "marked" ? "Marked deleted" : "Message deleted")
                        .italic()
                        .foregroundColor(Theme.secondaryText)
                } else if !message.text.isEmpty && message.groupInvitation == nil {
                    Text(RichText.attributed(message))
                        .textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                }
                footer
            }
            .padding(.horizontal, 11)
            .padding(.vertical, 7)
            .background(
                RoundedRectangle(cornerRadius: 14, style: .continuous)
                    .fill(message.outgoing ? Theme.accentSoft : Theme.elevated)
            )
        }
    }

    private var footer: some View {
        HStack(spacing: 4) {
            if message.edited {
                Text("edited")
            }
            if let date = message.date {
                Text(Dates.clock(date))
            }
            if message.outgoing && !inNotes {
                DeliveryMark(delivery: message.delivery)
            }
        }
        .font(.caption2)
        .foregroundColor(Theme.mutedText)
        .frame(maxWidth: .infinity, alignment: .trailing)
    }
}

struct DeliveryMark: View {
    let delivery: String

    var body: some View {
        switch delivery {
        case "sent":
            Text("✓")
        case "delivered":
            Text("✓✓").foregroundColor(Theme.accent)
        case "error", "warning":
            Image(systemName: "exclamationmark.circle").foregroundColor(.red)
        default:
            Image(systemName: "clock")
        }
    }
}

struct QuoteView: View {
    let quote: Quote

    var body: some View {
        HStack(spacing: 6) {
            RoundedRectangle(cornerRadius: 1.5)
                .fill(Theme.accent)
                .frame(width: 3)
            VStack(alignment: .leading, spacing: 1) {
                Text(quote.outgoing ? "You" : "Reply")
                    .font(.caption.weight(.semibold))
                    .foregroundColor(Theme.accent)
                Text(quote.text.isEmpty ? "Attachment" : quote.text)
                    .font(.callout)
                    .foregroundColor(Theme.secondaryText)
                    .lineLimit(2)
            }
        }
        .padding(6)
        .background(RoundedRectangle(cornerRadius: 8).fill(Color.primary.opacity(0.05)))
    }
}

struct GroupInviteCard: View {
    @EnvironmentObject private var model: AppModel
    let invite: GroupInvite
    let outgoing: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Label(outgoing ? "You invited them to \(invite.name)" : "Invitation to join \(invite.name)",
                  systemImage: "person.3")
            Text("Role: \(invite.role) · \(invite.status)")
                .font(.caption)
                .foregroundColor(Theme.secondaryText)
            if invite.pending {
                Button("Join group") { Task { await model.joinGroup(invite.groupId) } }
                    .buttonStyle(.borderedProminent)
            }
        }
    }
}

struct ReactionsBar: View {
    @EnvironmentObject private var model: AppModel
    let message: Message

    var body: some View {
        HStack(spacing: 4) {
            ForEach(message.reactions, id: \.emoji) { reaction in
                Button {
                    Task { await model.toggleReaction(message, emoji: reaction.emoji) }
                } label: {
                    Text("\(reaction.glyph) \(reaction.total)")
                        .font(.caption)
                        .padding(.horizontal, 7)
                        .padding(.vertical, 2)
                        .background(Capsule().fill(reaction.mine ? Theme.accentSoft : Theme.elevated))
                }
                .buttonStyle(.plain)
            }
        }
    }
}

struct MessageMenu: View {
    @EnvironmentObject private var model: AppModel
    let message: Message
    let canReact: Bool

    var body: some View {
        if message.deleted == nil {
            Button("Reply") { model.startReply(message) }
            if !message.text.isEmpty {
                Button("Copy") { Clipboard.copy(message.text) }
            }
            if canReact {
                Menu("React") {
                    ForEach(knownReactions, id: \.emoji) { reaction in
                        Button(reaction.glyph) {
                            Task { await model.toggleReaction(message, emoji: reaction.emoji) }
                        }
                    }
                }
            }
            if message.canEdit {
                Button("Edit") { model.startEdit(message) }
            }
            if let file = message.file, file.isComplete {
                Divider()
                Button("Open") { Task { await model.open(file) } }
                Button("Save As…") { Task { await model.saveAs(file) } }
            }
            Divider()
        }
        Button("Delete for me") { Task { await model.deleteMessage(message, forEveryone: false) } }
        if message.outgoing && message.deleted == nil {
            Button("Delete for everyone") { Task { await model.deleteMessage(message, forEveryone: true) } }
        }
    }
}

// MARK: - Rich text

enum RichText {
    static func attributed(_ message: Message) -> AttributedString {
        guard !message.segments.isEmpty else { return AttributedString(message.text) }
        var out = AttributedString()
        for seg in message.segments {
            var text = seg.text
            if seg.style == "secret" {
                text = String(repeating: "•", count: max(3, min(seg.text.count, 24)))
            }
            var part = AttributedString(text)
            switch seg.style {
            case "bold":
                part.inlinePresentationIntent = .stronglyEmphasized
            case "italic":
                part.inlinePresentationIntent = .emphasized
            case "strike":
                part.inlinePresentationIntent = .strikethrough
            case "snippet":
                part.inlinePresentationIntent = .code
            case "small":
                part[AttributeScopes.SwiftUIAttributes.FontAttribute.self] = .caption
            case "secret":
                part[AttributeScopes.SwiftUIAttributes.ForegroundColorAttribute.self] = Theme.mutedText
            case "colored":
                if let color = Theme.formatColor(seg.color) { part[AttributeScopes.SwiftUIAttributes.ForegroundColorAttribute.self] = color }
            case "mention":
                part.inlinePresentationIntent = .stronglyEmphasized
                part[AttributeScopes.SwiftUIAttributes.ForegroundColorAttribute.self] = Theme.accent
            case "command":
                part.inlinePresentationIntent = .code
                part[AttributeScopes.SwiftUIAttributes.ForegroundColorAttribute.self] = Theme.accent
            case "uri", "email", "phone":
                if let url = linkURL(seg) {
                    part.link = url
                    part[AttributeScopes.SwiftUIAttributes.ForegroundColorAttribute.self] = Theme.accent
                }
            default:
                if let url = seg.link.flatMap(URL.init(string:)) {
                    part.link = url
                    part[AttributeScopes.SwiftUIAttributes.ForegroundColorAttribute.self] = Theme.accent
                }
            }
            out.append(part)
        }
        return out
    }

    private static func linkURL(_ seg: Segment) -> URL? {
        if let link = seg.link, let url = URL(string: link) { return url }
        let raw = seg.text.trimmingCharacters(in: .whitespaces)
        switch seg.style {
        case "email": return URL(string: "mailto:\(raw)")
        case "phone": return URL(string: "tel:\(raw.filter { !$0.isWhitespace })")
        default: return URL(string: raw.contains("://") ? raw : "https://\(raw)")
        }
    }
}

// MARK: - Attachments

struct AttachmentView: View {
    @EnvironmentObject private var model: AppModel
    let file: FileAttachment
    let messageId: Int64
    @State private var image: NSImage?

    var body: some View {
        if file.kind == "image" && file.isComplete {
            Group {
                if let image = image {
                    Image(nsImage: image)
                        .resizable()
                        .scaledToFit()
                        .frame(maxWidth: 300, maxHeight: 320)
                        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
                        .onTapGesture { Task { await model.open(file) } }
                        .help("Open in Preview")
                } else {
                    RoundedRectangle(cornerRadius: 10)
                        .fill(Color.primary.opacity(0.06))
                        .frame(width: 200, height: 140)
                        .overlay(ProgressView())
                }
            }
            .task(id: file.path) {
                if let url = await model.readablePath(file) {
                    image = NSImage(contentsOf: url)
                }
            }
        } else if file.kind == "voice" && file.isComplete {
            VoiceView(file: file, messageId: messageId)
        } else {
            FileRow(file: file, progress: model.fileProgress[file.id] ?? file.progress)
        }
    }
}

struct FileRow: View {
    @EnvironmentObject private var model: AppModel
    let file: FileAttachment
    let progress: Double?

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: symbol)
                .font(.title2)
                .foregroundColor(Theme.accent)
                .frame(width: 30)
            VStack(alignment: .leading, spacing: 2) {
                Text(file.name).lineLimit(1).truncationMode(.middle)
                Text(detail).font(.caption).foregroundColor(Theme.secondaryText)
                if file.state == "inProgress" {
                    if let progress = progress {
                        ProgressView(value: progress).frame(width: 160)
                    } else {
                        ProgressView().progressViewStyle(.linear).frame(width: 160)
                    }
                }
            }
            Spacer(minLength: 0)
            action
        }
        .frame(minWidth: 220)
    }

    @ViewBuilder private var action: some View {
        switch file.state {
        case "offer":
            Button { Task { await model.download(file) } } label: { Image(systemName: "arrow.down.circle") }
                .buttonStyle(.borderless)
                .font(.title2)
                .help("Download")
        case "complete":
            if file.isComplete {
                Button { Task { await model.open(file) } } label: { Image(systemName: "arrow.up.forward.app") }
                    .buttonStyle(.borderless)
                    .font(.title3)
                    .help("Open")
            }
        default:
            EmptyView()
        }
    }

    private var symbol: String {
        switch file.kind {
        case "image": return "photo"
        case "video": return "film"
        case "voice": return "waveform"
        default: return "doc"
        }
    }

    private var detail: String {
        var parts = [Format.bytes(file.size)]
        if let d = file.duration { parts.append(Format.duration(d)) }
        switch file.state {
        case "offer": parts.append("tap to download")
        case "failed": parts.append(file.error ?? "failed")
        default: break
        }
        return parts.joined(separator: " · ")
    }
}

/// Plays one voice message at a time.
final class VoicePlayer: NSObject, ObservableObject, AVAudioPlayerDelegate {
    static let shared = VoicePlayer()
    @Published private(set) var playingId: Int64?
    private var player: AVAudioPlayer?

    func toggle(id: Int64, url: URL) {
        if playingId == id {
            stop()
            return
        }
        stop()
        guard let player = try? AVAudioPlayer(contentsOf: url) else { return }
        player.delegate = self
        player.play()
        self.player = player
        playingId = id
    }

    func stop() {
        player?.stop()
        player = nil
        playingId = nil
    }

    func audioPlayerDidFinishPlaying(_ player: AVAudioPlayer, successfully flag: Bool) {
        DispatchQueue.main.async { [weak self] in
            if self?.player === player { self?.stop() }
        }
    }
}

struct VoiceView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject private var player = VoicePlayer.shared
    let file: FileAttachment
    let messageId: Int64

    var body: some View {
        HStack(spacing: 10) {
            Button {
                Task {
                    if let url = await model.readablePath(file) {
                        player.toggle(id: messageId, url: url)
                    }
                }
            } label: {
                Image(systemName: player.playingId == messageId ? "pause.circle.fill" : "play.circle.fill")
                    .font(.system(size: 28))
                    .foregroundColor(Theme.accent)
            }
            .buttonStyle(.plain)
            Image(systemName: "waveform")
                .foregroundColor(Theme.secondaryText)
            Text(file.duration.map(Format.duration) ?? "Voice message")
                .font(.callout.monospacedDigit())
                .foregroundColor(Theme.secondaryText)
        }
    }
}

// MARK: - Composer

struct ComposerView: View {
    @EnvironmentObject private var model: AppModel
    let chat: Chat
    @FocusState private var focused: Bool

    private var canSend: Bool {
        !model.draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    var body: some View {
        if chat.pendingRequestId != nil {
            Text("Accept the request to start chatting.")
                .foregroundColor(Theme.secondaryText)
                .frame(maxWidth: .infinity)
                .padding(14)
        } else {
            VStack(spacing: 0) {
                contextBar
                HStack(spacing: 8) {
                    Button(action: attach) {
                        Image(systemName: "paperclip").font(.title3)
                    }
                    .buttonStyle(.borderless)
                    .help("Send a file")
                    TextField(placeholder, text: $model.draft)
                        .textFieldStyle(.plain)
                        .focused($focused)
                        .onSubmit(send)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 8)
                        .background(RoundedRectangle(cornerRadius: 18, style: .continuous).fill(Theme.fieldFill))
                    Button(action: send) {
                        Image(systemName: "arrow.up.circle.fill")
                            .font(.system(size: 26))
                            .foregroundColor(canSend ? Theme.accent : Theme.mutedText)
                    }
                    .buttonStyle(.plain)
                    .disabled(!canSend)
                    .help("Send (Return)")
                }
                .padding(10)
            }
            .onAppear { focused = true }
            .onChange(of: model.composeMode) { _ in focused = true }
            .onExitCommand { model.cancelCompose() }
        }
    }

    private var placeholder: String {
        if chat.isNotes { return "Note to self" }
        if case .edit = model.composeMode { return "Edit message" }
        return "Message"
    }

    @ViewBuilder private var contextBar: some View {
        switch model.composeMode {
        case .new:
            EmptyView()
        case .reply(let message):
            bar(symbol: "arrowshape.turn.up.left", title: "Replying", text: message.text)
        case .edit(let message):
            bar(symbol: "pencil", title: "Editing", text: message.text)
        }
    }

    private func bar(symbol: String, title: String, text: String) -> some View {
        HStack(spacing: 8) {
            Image(systemName: symbol).foregroundColor(Theme.accent)
            VStack(alignment: .leading, spacing: 1) {
                Text(title).font(.caption.weight(.semibold)).foregroundColor(Theme.accent)
                Text(text.isEmpty ? "Attachment" : text).font(.callout).lineLimit(1)
                    .foregroundColor(Theme.secondaryText)
            }
            Spacer()
            Button { model.cancelCompose() } label: { Image(systemName: "xmark.circle.fill") }
                .buttonStyle(.borderless)
                .foregroundColor(Theme.mutedText)
                .help("Cancel (Esc)")
        }
        .padding(.horizontal, 14)
        .padding(.top, 8)
    }

    private func send() {
        guard canSend else { return }
        Task { await model.send() }
    }

    private func attach() {
        let panel = NSOpenPanel()
        panel.allowsMultipleSelection = true
        panel.canChooseDirectories = false
        panel.prompt = "Send"
        guard panel.runModal() == .OK else { return }
        let urls = panel.urls
        Task { await model.sendFiles(urls) }
    }
}
