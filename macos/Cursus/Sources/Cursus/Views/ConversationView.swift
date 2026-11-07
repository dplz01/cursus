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
            Hairline()
            ComposerView(chat: chat)
        }
        .navigationTitle(chat.title)
        .navigationSubtitle(subtitle)
        .toolbar {
            ToolbarItem {
                if !chat.isNotes && chat.pendingRequestId == nil {
                    Button { model.toggleInfo() } label: {
                        Image(systemName: model.showInfo ? "info.circle.fill" : "info.circle")
                    }
                    .help(chat.isGroup ? "Group info (⌘I)" : "Contact info (⌘I)")
                }
            }
        }
        .onDrop(of: [UTType.fileURL], isTargeted: $dropTargeted, perform: drop)
        .overlay {
            if dropTargeted {
                RoundedRectangle(cornerRadius: Radius.lg)
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
            IconDisc(icon: "user-plus-alt-1", size: 36)
            VStack(alignment: .leading, spacing: 1) {
                Text(chat.title).font(.cBodyBold).foregroundColor(Theme.primaryText)
                Text("wants to connect with you").font(.cMeta).foregroundColor(Theme.secondaryText)
            }
            Spacer()
            Button("Reject") { Task { await model.rejectRequest(chat) } }
                .buttonStyle(SecondaryButtonStyle(small: true))
            Button("Accept") { Task { await model.acceptRequest(chat) } }
                .buttonStyle(PrimaryButtonStyle(small: true))
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(Theme.elevated)
        .overlay(Hairline(), alignment: .bottom)
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
                .font(.cMeta)
                .foregroundColor(Theme.mutedText)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 10)
                .padding(.vertical, 3)
                .background(Capsule().fill(Theme.elevated.opacity(0.7)))
                .frame(maxWidth: .infinity)
                .padding(.vertical, 4)
        } else {
            HStack(alignment: .bottom, spacing: 6) {
                if message.outgoing {
                    Spacer(minLength: 80)
                } else if showSender {
                    Avatar(image: message.sender?.image, name: message.sender?.name ?? "?", size: 28, kind: .contact)
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
                        .font(.cMetaBold)
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
                        .font(.cBody)
                        .foregroundColor(Theme.primaryText)
                        .textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                }
                footer
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(
                RoundedRectangle(cornerRadius: Radius.bubble, style: .continuous)
                    .fill(message.outgoing ? Theme.bubbleOut : Theme.bubbleIn)
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
        .font(.system(size: 11))
        .foregroundColor(message.outgoing && !message.emojiOnly ? Theme.bubbleOutMeta : Theme.mutedText)
        .frame(maxWidth: .infinity, alignment: .trailing)
    }
}

struct DeliveryMark: View {
    let delivery: String

    var body: some View {
        switch delivery {
        case "sent":
            CIcon("check", size: 13)
        case "delivered":
            CIcon("check-read", size: 13, color: Theme.bubbleOutTick)
        case "error", "warning":
            Image(systemName: "exclamationmark.circle").foregroundColor(Theme.danger)
        default:
            CIcon("system-pending-line", size: 12)
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
                    .font(.cMetaBold)
                    .foregroundColor(Theme.accent)
                Text(quote.text.isEmpty ? "Attachment" : quote.text)
                    .font(.cMeta)
                    .foregroundColor(Theme.secondaryText)
                    .lineLimit(2)
            }
        }
        .padding(6)
        .background(RoundedRectangle(cornerRadius: Radius.sm).fill(Color.primary.opacity(0.05)))
    }
}

struct GroupInviteCard: View {
    @EnvironmentObject private var model: AppModel
    let invite: GroupInvite
    let outgoing: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 10) {
                IconDisc(icon: "user-group", size: 36)
                VStack(alignment: .leading, spacing: 1) {
                    Text(outgoing ? "You invited them to \(invite.name)" : "Invitation to join \(invite.name)")
                        .font(.cBody)
                        .foregroundColor(Theme.primaryText)
                    Text("Role: \(invite.role) · \(invite.status)")
                        .font(.cMeta)
                        .foregroundColor(Theme.secondaryText)
                }
            }
            if invite.pending {
                Button("Join group") { Task { await model.joinGroup(invite.groupId) } }
                    .buttonStyle(PrimaryButtonStyle(small: true))
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
                        .font(.cMeta)
                        .foregroundColor(reaction.mine ? Theme.accent : Theme.primaryText)
                        .padding(.horizontal, 8)
                        .padding(.vertical, 3)
                        .background(RoundedRectangle(cornerRadius: 14).fill(reaction.mine ? Theme.accentSoft : Theme.elevated))
                        .overlay(RoundedRectangle(cornerRadius: 14).strokeBorder(reaction.mine ? Theme.accent : Theme.border, lineWidth: 1))
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
                        .clipShape(RoundedRectangle(cornerRadius: Radius.md, style: .continuous))
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
            IconDisc(icon: symbol, size: 36)
            VStack(alignment: .leading, spacing: 2) {
                Text(file.name).font(.cBody).foregroundColor(Theme.primaryText).lineLimit(1).truncationMode(.middle)
                Text(detail).font(.cMeta).foregroundColor(Theme.secondaryText)
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
            IconTool(icon: "download", help: "Download") { Task { await model.download(file) } }
        case "complete":
            if file.isComplete {
                IconTool(icon: "sf:arrow.up.forward.app", help: "Open", size: 18) { Task { await model.open(file) } }
            }
        default:
            EmptyView()
        }
    }

    private var symbol: String {
        switch file.kind {
        case "image": return "image-square"
        case "video": return "sf:film"
        case "voice": return "mic"
        default: return "review-file"
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
                CIcon(player.playingId == messageId ? "stop" : "play", size: 16, color: Theme.accentOn)
                    .frame(width: 34, height: 34)
                    .background(Circle().fill(Theme.accent))
            }
            .buttonStyle(.plain)
            Image(systemName: "waveform")
                .foregroundColor(Theme.secondaryText)
            Text(file.duration.map(Format.duration) ?? "Voice message")
                .font(.cMeta.monospacedDigit())
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
                .font(.cBody)
                .foregroundColor(Theme.secondaryText)
                .frame(maxWidth: .infinity)
                .padding(14)
                .background(Theme.list)
        } else {
            VStack(spacing: 0) {
                contextBar
                HStack(spacing: 6) {
                    IconTool(icon: "sf:paperclip", help: "Send a file", action: attach)
                    TextField(placeholder, text: $model.draft)
                        .textFieldStyle(.plain)
                        .font(.cBody)
                        .focused($focused)
                        .onSubmit(send)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 9)
                        .background(RoundedRectangle(cornerRadius: Radius.md, style: .continuous).fill(Theme.fieldFill))
                        .overlay(
                            RoundedRectangle(cornerRadius: Radius.md, style: .continuous)
                                .strokeBorder(focused ? Theme.accent : Theme.border, lineWidth: 1))
                    Button(action: send) {
                        CIcon("send", size: 18, color: canSend ? Theme.accentOn : Theme.mutedText)
                            .frame(width: 34, height: 34)
                            .background(Circle().fill(canSend ? Theme.accent : Theme.disabledFill))
                    }
                    .buttonStyle(.plain)
                    .disabled(!canSend)
                    .help("Send (Return)")
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 8)
            }
            .background(Theme.list)
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
            bar(icon: "reply", title: "Replying", text: message.text)
        case .edit(let message):
            bar(icon: "edit-3", title: "Editing", text: message.text)
        }
    }

    private func bar(icon: String, title: String, text: String) -> some View {
        HStack(spacing: 8) {
            RoundedRectangle(cornerRadius: 1.5).fill(Theme.accent).frame(width: 3, height: 30)
            CIcon(icon, size: 18, color: Theme.accent)
            VStack(alignment: .leading, spacing: 1) {
                Text(title).font(.cMetaBold).foregroundColor(Theme.accent)
                Text(text.isEmpty ? "Attachment" : text).font(.cMeta).lineLimit(1)
                    .foregroundColor(Theme.secondaryText)
            }
            Spacer()
            IconTool(icon: "cancel", help: "Cancel (Esc)", size: 14) { model.cancelCompose() }
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
