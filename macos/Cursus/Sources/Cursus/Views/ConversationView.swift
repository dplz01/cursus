import AppKit
import AVFoundation
import SwiftUI
import UniformTypeIdentifiers

struct ConversationView: View {
    @EnvironmentObject private var model: AppModel
    let chat: Chat
    @State private var dropTargeted = false
    @State private var shownCount = 0
    /// The end of the history is on screen (new incoming messages follow).
    @State private var atBottom = true
    @State private var viewportHeight: CGFloat = 0

    var body: some View {
        VStack(spacing: 0) {
            if let thread = model.support {
                // "Chat with admins" thread: its title, the group, Back (Esc).
                PaneHeader(title: thread.title, subtitle: chat.title, back: { model.closeSupport() },
                           background: Theme.sidebar) {
                    if let member = thread.member {
                        Avatar(image: member.image, name: member.name, size: 36, kind: .contact)
                    } else {
                        Avatar(image: chat.image, name: chat.title, size: 36, kind: chat.avatarKind)
                    }
                } actions: {
                    EmptyView()
                }
                if let member = thread.member, member.awaitingReview, chat.isModerator {
                    ReviewBanner(chat: chat, member: member)
                }
            } else {
                PaneHeader(title: chat.title, subtitle: subtitle, background: Theme.sidebar,
                           open: openProfile,
                           openHelp: chat.isGroup ? "Group info (⌘I)" : "Contact info (⌘I)") {
                    Avatar(image: chat.image, name: chat.title, size: 36, kind: chat.avatarKind)
                } actions: {
                    // Unread support messages: the yellow counter (Linux header).
                    if chat.isGroup && !chat.isChannel {
                        let unread = chat.isModerator ? chat.membersRequireAttention : chat.supportUnread
                        if unread > 0 {
                            AttentionButton(icon: "admin", count: unread,
                                            help: chat.isModerator ? "Chats with members" : "Chat with admins") {
                                if chat.isModerator { model.toggleInfo() } else { model.openSupport(chat) }
                            }
                        }
                    }
                    if canOpenProfile {
                        IconTool(icon: "info-circle", help: chat.isGroup ? "Group info (⌘I)" : "Contact info (⌘I)",
                                 size: 20) { model.toggleInfo() }
                    }
                }
            }
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
                            if message.id == model.newMessagesFrom {
                                NewMessagesDivider()
                            }
                            MessageRow(message: message, showSender: chat.isGroup && !message.outgoing,
                                       inNotes: chat.isNotes)
                                .id(message.id)
                        }
                        // Where the end of the history is (see `atBottom`).
                        Color.clear
                            .frame(height: 1)
                            .background(GeometryReader { geo in
                                Color.clear.preference(key: HistoryEndKey.self,
                                                       value: geo.frame(in: .named("history")).minY)
                            })
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 12)
                }
                .coordinateSpace(name: "history")
                .background(GeometryReader { geo in
                    Color.clear
                        .onAppear { viewportHeight = geo.size.height }
                        .onChange(of: geo.size.height) { viewportHeight = $0 }
                })
                .onPreferenceChange(HistoryEndKey.self) { endY in
                    atBottom = endY <= viewportHeight + 80
                }
                .background(Theme.panel)
                .onChange(of: model.messages.last?.id) { _ in
                    let firstLoad = shownCount == 0
                    shownCount = model.messages.count
                    if firstLoad {
                        // Opened with unread messages: start at the first one.
                        if let from = model.newMessagesFrom {
                            proxy.scrollTo(from, anchor: .top)
                        } else {
                            scrollToBottom(proxy, animated: false)
                        }
                    } else if model.messages.last?.outgoing == true || atBottom {
                        // Your message, or you're reading the end: follow it.
                        // Otherwise stay put; the divider marks what's new.
                        scrollToBottom(proxy, animated: true)
                    }
                }
                .onChange(of: model.composeMode) { mode in
                    if mode != .new { scrollToBottom(proxy, animated: true) }
                }
                .onChange(of: model.scrollTarget) { target in
                    guard let target = target else { return }
                    withAnimation(.easeOut(duration: 0.25)) { proxy.scrollTo(target, anchor: .center) }
                    model.scrollTarget = nil
                }
            }
            Hairline()
            if model.selectingMessages {
                SelectionBar()
            } else {
                ComposerView(chat: chat, recorder: model.recorder)
            }
        }
        .sheet(isPresented: Binding(get: { model.forwarding != nil },
                                    set: { if !$0 { model.forwarding = nil } })) {
            ForwardSheet(ids: model.forwarding ?? []).environmentObject(model)
        }
        // Own anchors: two .sheet modifiers on one view clash on macOS 12.
        .background(Color.clear.sheet(item: $model.infoMessage) { message in
            MessageInfoSheet(message: message).environmentObject(model)
        })
        .overlay(Color.clear.allowsHitTesting(false).sheet(item: $model.gallery) { state in
            MediaGallery(items: state.items, index: state.index).environmentObject(model)
        })
        // Message links are untrusted: checked by the core and confirmed.
        .environment(\.openURL, OpenURLAction { url in
            model.openLink(url)
            return .handled
        })
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

    private var canOpenProfile: Bool { !chat.isNotes && chat.pendingRequestId == nil }

    private var openProfile: (() -> Void)? {
        guard canOpenProfile else { return nil }
        return { model.toggleInfo() }
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

/// Bottom edge of the history in its scroll view (∞ when not laid out).
private struct HistoryEndKey: PreferenceKey {
    static var defaultValue: CGFloat = .infinity
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = min(value, nextValue())
    }
}

/// "New messages" line + pill above the first message that just arrived.
struct NewMessagesDivider: View {
    var body: some View {
        HStack(spacing: 10) {
            Rectangle().fill(Theme.accent.opacity(0.45)).frame(height: 1)
            Text("New messages")
                .font(.cMetaBold)
                .foregroundColor(Theme.accent)
                .padding(.horizontal, 10)
                .padding(.vertical, 3)
                .background(Capsule().fill(Theme.accentSoft))
            Rectangle().fill(Theme.accent.opacity(0.45)).frame(height: 1)
        }
        .padding(.top, 8)
    }
}

/// A join request in its support thread: accept (as member / observer) or remove.
struct ReviewBanner: View {
    @EnvironmentObject private var model: AppModel
    let chat: Chat
    let member: Member

    var body: some View {
        HStack(spacing: 8) {
            CIcon("review-file", size: 18, color: Theme.gold)
            Text("\(member.name) wants to join.").font(.cBody).foregroundColor(Theme.primaryText)
            Spacer()
            Button("Remove") {
                Task {
                    await model.removeMember(chat, member, withMessages: false)
                    model.closeSupport()
                }
            }
            .buttonStyle(SecondaryButtonStyle())
            Button("Accept as observer") { Task { await model.acceptMember(chat, member, role: "observer") } }
                .buttonStyle(SecondaryButtonStyle())
            Button("Accept") { Task { await model.acceptMember(chat, member) } }
                .buttonStyle(PrimaryButtonStyle())
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .background(Theme.list)
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
            // Top: a group sender's avatar sits at the top of the message.
            HStack(alignment: .top, spacing: 6) {
                if model.selectingMessages {
                    SelectionMark(selected: model.selectedMessages.contains(message.id))
                        .padding(.top, 8)
                }
                if message.outgoing {
                    Spacer(minLength: 80)
                } else if showSender {
                    Avatar(image: message.sender?.image, name: message.sender?.name ?? "?", size: 28, kind: .contact)
                }
                VStack(alignment: message.outgoing ? .trailing : .leading, spacing: 3) {
                    bubble
                        .overlay(
                            bubbleShape
                                .stroke(Theme.accent, lineWidth: model.highlightedMessage == message.id ? 2 : 0))
                        .contextMenu { MessageMenu(message: message, canReact: !inNotes) }
                    if !message.reactions.isEmpty {
                        ReactionsBar(message: message, inGroup: model.selectedChat?.isGroup == true)
                    }
                }
                if !message.outgoing {
                    Spacer(minLength: 80)
                }
            }
            .overlay {
                // While selecting, a click anywhere on the row (de)selects it.
                if model.selectingMessages {
                    Color.clear
                        .contentShape(Rectangle())
                        .onTapGesture { model.toggleSelected(message) }
                }
            }
        }
    }

    private var revealed: Bool { model.revealedMessages.contains(message.id) }

    private var bubbleShape: BubbleShape {
        BubbleShape(outgoing: message.outgoing, runContinues: message.runContinues)
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
                    // Name, then the role as its icon only (owner / admin / moderator).
                    HStack(spacing: 4) {
                        Text(sender.name)
                            .font(.cMetaBold)
                            .foregroundColor(Theme.accent)
                        if groupRoleRank(sender.role) > groupRoleRank("member") {
                            CIcon(MemberRoles.icon(sender.role), size: 13, color: Theme.mutedText)
                                .help(sender.role.capitalized)
                        }
                    }
                }
                if let quote = message.quote {
                    QuoteView(quote: quote)
                        .onTapGesture { model.jumpTo(quote.itemId) }
                        .help("Show the original message")
                }
                if let file = message.file, message.deleted == nil {
                    AttachmentView(file: file, messageId: message.id, outgoing: message.outgoing)
                }
                if let invite = message.groupInvitation {
                    GroupInviteCard(invite: invite, outgoing: message.outgoing)
                }
                if let deleted = message.deleted {
                    Text(deleted == "marked" ? "Marked deleted" : "Message deleted")
                        .italic()
                        .foregroundColor(Theme.secondaryText)
                    if deleted == "marked" && revealed && !message.text.isEmpty {
                        Text(RichText.attributed(message, reveal: true))
                            .font(.cBody)
                            .foregroundColor(Theme.secondaryText)
                            .textSelection(.enabled)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                } else if message.file != nil {
                    // Under an attachment: only the sender's own caption.
                    if !message.caption.isEmpty {
                        Text(message.caption)
                            .font(.cBody)
                            .foregroundColor(Theme.primaryText)
                            .textSelection(.enabled)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                } else if !message.text.isEmpty && message.groupInvitation == nil {
                    if message.hasSecret {
                        // Secret text: click to reveal / hide (Linux: tap).
                        Text(RichText.attributed(message, reveal: revealed))
                            .font(.cBody)
                            .foregroundColor(Theme.primaryText)
                            .fixedSize(horizontal: false, vertical: true)
                            .onTapGesture { model.toggleRevealed(message) }
                            .help(revealed ? "Hide secret text" : "Show secret text")
                    } else {
                        Text(RichText.attributed(message))
                            .font(.cBody)
                            .foregroundColor(Theme.primaryText)
                            .textSelection(.enabled)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                footer
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(
                bubbleShape.fill(message.outgoing ? Theme.bubbleOut : Theme.bubbleIn)
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
    let message: Message
    let inGroup: Bool

    var body: some View {
        HStack(spacing: 4) {
            ForEach(message.reactions, id: \.emoji) { reaction in
                ReactionChip(message: message, reaction: reaction, inGroup: inGroup)
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
            Button("Forward…") { model.forwarding = [message.id] }
            if message.hasSecret {
                Button(model.revealedMessages.contains(message.id) ? "Hide secret text" : "Show secret text") {
                    model.toggleRevealed(message)
                }
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
        if message.deleted == "marked" && !message.text.isEmpty {
            Button(model.revealedMessages.contains(message.id) ? "Hide original" : "Show original") {
                model.toggleRevealed(message)
            }
        }
        Button("Info") { model.infoMessage = message }
        Button("Select") { model.startSelecting(message) }
        Divider()
        Button("Delete for me") { Task { await model.deleteMessage(message, forEveryone: false) } }
        if message.outgoing && message.deleted == nil {
            Button("Delete for everyone") { Task { await model.deleteMessage(message, forEveryone: true) } }
        }
    }
}

// MARK: - Rich text

enum RichText {
    /// `reveal`: show secret spans (highlighted) instead of dots.
    static func attributed(_ message: Message, reveal: Bool = false) -> AttributedString {
        guard !message.segments.isEmpty else { return AttributedString(message.text) }
        var out = AttributedString()
        for seg in message.segments {
            var text = seg.text
            if seg.style == "secret" && !reveal {
                text = String(repeating: "•", count: max(3, min(seg.text.count, 24)))
            }
            var part = AttributedString(text)
            switch seg.style {
            case "bold":
                part.inlinePresentationIntent = .stronglyEmphasized
            case "italic":
                part.inlinePresentationIntent = .emphasized
                // SimpleX links: the italic "(via …)" part is the same link.
                if let link = seg.link, let url = URL(string: link) {
                    part.link = url
                    part[AttributeScopes.SwiftUIAttributes.ForegroundColorAttribute.self] = Theme.accent
                }
            case "strike":
                part.inlinePresentationIntent = .strikethrough
            case "snippet":
                part.inlinePresentationIntent = .code
            case "small":
                part[AttributeScopes.SwiftUIAttributes.FontAttribute.self] = .caption
            case "secret":
                if reveal {
                    part[AttributeScopes.SwiftUIAttributes.BackgroundColorAttribute.self] = Theme.accentSoft
                } else {
                    part[AttributeScopes.SwiftUIAttributes.ForegroundColorAttribute.self] = Theme.mutedText
                }
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
    var outgoing = false
    @State private var image: NSImage?
    @AppStorage(Prefs.blurMedia) private var blurMedia = false

    /// Blurred until clicked when "Blur photos and videos" is on.
    private var hidden: Bool { blurMedia && !model.revealedMessages.contains(messageId) }

    var body: some View {
        if file.kind == "image" && file.isComplete {
            Group {
                if let image = image {
                    Image(nsImage: image)
                        .resizable()
                        .scaledToFit()
                        .frame(maxWidth: 300, maxHeight: 320)
                        .blur(radius: hidden ? 24 : 0)
                        .clipShape(RoundedRectangle(cornerRadius: Radius.md, style: .continuous))
                        .overlay {
                            if hidden {
                                Text("Click to show")
                                    .font(.cMetaBold)
                                    .foregroundColor(.white)
                                    .padding(.horizontal, 10)
                                    .padding(.vertical, 5)
                                    .background(Capsule().fill(Color.black.opacity(0.45)))
                            }
                        }
                        .onTapGesture {
                            if hidden {
                                model.revealedMessages.insert(messageId)
                            } else {
                                model.openGallery(at: messageId)
                            }
                        }
                        .help(hidden ? "Show photo" : "Open (← → for the others)")
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
        } else if file.kind == "video" && file.isComplete {
            VideoThumbnail(file: file, messageId: messageId, hidden: hidden)
                .onTapGesture {
                    if hidden {
                        model.revealedMessages.insert(messageId)
                    } else {
                        model.openGallery(at: messageId)
                    }
                }
                .help(hidden ? "Show video" : "Play (← → for the others)")
        } else if file.kind == "voice" && file.isComplete {
            VoiceView(file: file, messageId: messageId, outgoing: outgoing)
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
    /// 0…1 of the message playing (waveform fill).
    @Published private(set) var progress: Double = 0
    private var player: AVAudioPlayer?
    private var timer: Timer?

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
        progress = 0
        timer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] _ in
            guard let self = self, let p = self.player, p.duration > 0 else { return }
            self.progress = p.currentTime / p.duration
        }
    }

    func stop() {
        timer?.invalidate()
        timer = nil
        player?.stop()
        player = nil
        playingId = nil
        progress = 0
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
    var outgoing = false
    @State private var peaks: [Float]?

    var body: some View {
        HStack(spacing: 10) {
            Button {
                Task {
                    if let url = await model.readablePath(file) {
                        player.toggle(id: messageId, url: url)
                    }
                }
            } label: {
                CIcon(player.playingId == messageId ? "stop" : "play", size: 18, color: Theme.accentOn)
                    .frame(width: 40, height: 40)
                    // The nav rail buttons' shape (Linux too).
                    .background(RoundedRectangle(cornerRadius: Radius.lg, style: .continuous).fill(Theme.accent))
            }
            .buttonStyle(.plain)
            .help(player.playingId == messageId ? "Stop" : "Play")
            WaveformView(peaks: peaks ?? Array(repeating: 0.25, count: 48),
                         progress: player.playingId == messageId ? player.progress : 0,
                         rest: outgoing ? Theme.voiceRestOut : Theme.voiceRest)
                .frame(width: 180, height: 28)
        }
        .task(id: file.path) {
            guard let url = await model.readablePath(file) else { return }
            peaks = await Task.detached(priority: .utility) { Waveform.peaks(url) }.value
        }
    }
}

// MARK: - Composer

struct ComposerView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject private var recorder: VoiceRecorder
    let chat: Chat

    init(chat: Chat, recorder: VoiceRecorder) {
        self.chat = chat
        _recorder = ObservedObject(wrappedValue: recorder)
    }
    @AppStorage(Prefs.enterSends) private var enterSends = true
    @State private var fieldHeight = ComposerField.lineHeight
    /// Bumped to give the field the keyboard focus.
    @State private var focusToken = 0
    /// What's typed, kept here: writing every keystroke into the app-wide
    /// draft re-rendered the whole window (typing lagged). The draft gets it
    /// after a short pause, and right away when sending.
    @State private var text = ""
    @State private var pushedDraft = ""
    @State private var draftSync: Task<Void, Never>?
    @State private var editing = false
    /// `:shortcode` suggestions for what's typed (core rules).
    @State private var suggestions: [EmojiHit] = []
    @State private var suggestIndex = 0
    @State private var emojiTask: Task<Void, Never>?

    private var canSend: Bool {
        !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    var body: some View {
        if chat.pendingRequestId != nil {
            Text("Accept the request to start chatting.")
                .font(.cBody)
                .foregroundColor(Theme.secondaryText)
                .frame(maxWidth: .infinity)
                .padding(14)
                .background(Theme.list)
        } else if let draft = model.voiceDraft {
            VoiceDraftBar(draft: draft)
        } else if recorder.recording {
            RecordingBar(recorder: recorder)
        } else {
            VStack(spacing: 0) {
                contextBar
                if !suggestions.isEmpty { suggestionStrip }
                HStack(spacing: 6) {
                    IconTool(icon: "sf:paperclip", help: "Send a file", action: attach)
                    ZStack(alignment: .topLeading) {
                        ComposerField(text: $text, height: $fieldHeight, enterSends: enterSends,
                                      focusToken: focusToken, onSend: send,
                                      onEscape: escape,
                                      onAccept: acceptHighlighted,
                                      onTab: cycleSuggestion,
                                      onUp: { model.editLastOutgoing() })
                            .frame(height: fieldHeight)
                        if text.isEmpty {
                            Text(placeholder)
                                .font(.cBody)
                                .foregroundColor(Theme.mutedText)
                                .padding(.top, 2)
                                .padding(.leading, 5) // NSTextView line-fragment padding
                                .allowsHitTesting(false)
                        }
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 7)
                    .background(RoundedRectangle(cornerRadius: Radius.md, style: .continuous).fill(Theme.fieldFill))
                    .overlay(
                        RoundedRectangle(cornerRadius: Radius.md, style: .continuous)
                            .strokeBorder(Theme.border, lineWidth: 1))
                    IconTool(icon: "mic", help: "Record a voice message", size: 20) {
                        Task { await model.startRecording() }
                    }
                    Button(action: send) {
                        CIcon("send", size: 18, color: canSend ? Theme.accentOn : Theme.mutedText)
                            .frame(width: 34, height: 34)
                            .background(Circle().fill(canSend ? Theme.accent : Theme.disabledFill))
                    }
                    .buttonStyle(.plain)
                    .disabled(!canSend)
                    .help(enterSends ? "Send (Return)" : "Send (⌘Return)")
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 8)
            }
            .background(Theme.list)
            .onAppear {
                text = model.draft
                pushedDraft = model.draft
                focusToken += 1
            }
            .onChange(of: model.composeMode) { mode in
                focusToken += 1
                if case .edit = mode { editing = true } else { editing = false }
            }
            .onChange(of: text) { new in
                scheduleDraftSync(new)
                updateEmoji(new)
            }
            // The model changed the draft (edit starts, send clears it).
            .onChange(of: model.draft) { new in
                guard new != pushedDraft else { return }
                pushedDraft = new
                text = new
            }
            // Switching chats: keep what was typed for this chat (an edit
            // in progress isn't a draft).
            .onDisappear { model.saveDraft(editing ? "" : text, for: chat.ref) }
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
        pushDraft(text)
        Task { await model.send() }
    }

    // MARK: Emoji shortcodes

    private var suggestionStrip: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 4) {
                ForEach(Array(suggestions.enumerated()), id: \.element) { index, hit in
                    Button { accept(index) } label: {
                        HStack(spacing: 4) {
                            Text(hit.glyph).font(.system(size: 18))
                            Text(":\(hit.shortcode):").font(.cMeta).foregroundColor(Theme.secondaryText)
                        }
                        .padding(.horizontal, 8)
                        .padding(.vertical, 4)
                        .background(RoundedRectangle(cornerRadius: Radius.sm, style: .continuous)
                            .fill(index == suggestIndex ? Theme.accentSoft : Color.clear))
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .help("Insert (Return) · next (Tab)")
                }
            }
            .padding(.horizontal, 12)
            .padding(.top, 6)
        }
    }

    /// Completed `:shortcodes:` turn into emoji as you type; an open
    /// `:query` shows suggestions. Both decided by the core.
    private func updateEmoji(_ value: String) {
        emojiTask?.cancel()
        guard value.contains(":") else {
            suggestions = []
            return
        }
        emojiTask = Task { @MainActor in
            let expanded = await model.expandShortcodes(value)
            guard !Task.isCancelled, value == text else { return }
            if expanded != value {
                suggestions = []
                text = expanded
                return
            }
            let hits = await model.emojiSuggestions(for: value)
            guard !Task.isCancelled, value == text else { return }
            if hits != suggestions { suggestIndex = 0 }
            suggestions = hits
        }
    }

    private func accept(_ index: Int) {
        guard suggestions.indices.contains(index) else { return }
        let glyph = suggestions[index].glyph
        let current = text
        suggestions = []
        Task { @MainActor in
            if let done = await model.acceptShortcode(current, glyph: glyph), current == text {
                text = done
            }
            focusToken += 1
        }
    }

    private func acceptHighlighted() -> Bool {
        guard !suggestions.isEmpty else { return false }
        accept(suggestIndex)
        return true
    }

    private func cycleSuggestion(_ backwards: Bool) -> Bool {
        guard !suggestions.isEmpty else { return false }
        let n = suggestions.count
        suggestIndex = (suggestIndex + (backwards ? n - 1 : 1)) % n
        return true
    }

    /// Esc closes the suggestions first, then a reply / edit.
    private func escape() {
        if !suggestions.isEmpty {
            suggestions = []
        } else {
            model.cancelCompose()
        }
    }

    private func scheduleDraftSync(_ value: String) {
        draftSync?.cancel()
        draftSync = Task { @MainActor in
            try? await Task.sleep(nanoseconds: 400_000_000)
            guard !Task.isCancelled else { return }
            pushDraft(value)
        }
    }

    private func pushDraft(_ value: String) {
        draftSync?.cancel()
        guard model.draft != value else { return }
        pushedDraft = value
        model.draft = value
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
