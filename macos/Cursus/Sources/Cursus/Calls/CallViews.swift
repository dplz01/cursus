import AppKit
import SwiftUI

struct CallBannerView: View {
    @ObservedObject var calls: MacCalls
    var body: some View {
        if let call = calls.state.active {
            Button { calls.reveal() } label: {
                HStack(spacing: 12) {
                    CIcon("calls", size: 16, color: Theme.accent)
                    Text(call.name).font(.cBodyBold).lineLimit(1)
                    Spacer()
                    Text(call.label).font(.cMeta)
                    Text("Show call").font(.cMeta).foregroundColor(Theme.accent)
                }.padding(.horizontal, 16).padding(.vertical, 9)
                    .frame(maxWidth: .infinity).background(Theme.accent.opacity(0.12))
            }.buttonStyle(.plain)
        }
    }
}

/// The presentation observes the nested call object directly, so updates do
/// not depend on unrelated chat changes propagating through AppModel.
struct CallPresentation: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var calls: MacCalls
    var body: some View {
        Color.clear.frame(width: 0, height: 0)
            .sheet(isPresented: $calls.expanded) {
                VStack(spacing: 0) {
                    CallControlsView(calls: calls).environmentObject(model)
                    Button("Back to chats") { calls.expanded = false }
                        .keyboardShortcut(.cancelAction).buttonStyle(SecondaryButtonStyle())
                        .padding(.bottom, 20)
                }.frame(width: 420)
                    .onExitCommand { calls.expanded = false }
            }
    }
}

struct CallControlsView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var calls: MacCalls
    private var call: NativeCall? { calls.state.active }
    private var contact: Chat? { model.chats.first { $0.isDirect && $0.apiId == call?.contactId } }
    var body: some View {
        ScrollView {
            VStack(spacing: 18) {
                Avatar(image: contact?.image, name: call?.name ?? "Audio call", size: 76, kind: .contact)
                Text(call?.name ?? "Audio call").font(.cTitle).foregroundColor(Theme.primaryText)
                    .lineLimit(2).multilineTextAlignment(.center)
                Text(call?.label ?? "Starting audio call…").font(.cBody).foregroundColor(Theme.secondaryText)
                if let call = call, ["connecting", "connected"].contains(call.phase) {
                    Text(call.encrypted ? "End-to-end encrypted audio" : "Secure audio connection")
                        .font(.cMeta).foregroundColor(Theme.mutedText)
                }
                if call?.phase == "incoming" {
                    HStack(spacing: 12) {
                        Button { calls.end(reject: true) } label: {
                            Label { Text("Decline") } icon: { CIcon("call_rejected", size: 16) }
                        }.buttonStyle(SecondaryButtonStyle())
                        Button { calls.accept() } label: {
                            Label { Text("Answer") } icon: { CIcon("call_incoming", size: 16) }
                        }.buttonStyle(PrimaryButtonStyle(stretch: true)).disabled(calls.busy)
                    }
                } else {
                    HStack(spacing: 12) {
                        Button { calls.mute() } label: {
                            Label(call?.muted == true ? "Unmute" : "Mute",
                                  systemImage: call?.muted == true ? "mic.slash" : "mic")
                        }
                            .buttonStyle(SecondaryButtonStyle()).disabled(call == nil || calls.busy)
                        Button {
                            if let url = URL(string: "x-apple.systempreferences:com.apple.preference.sound") { NSWorkspace.shared.open(url) }
                        } label: {
                            Label("Audio settings…", systemImage: "speaker.wave.2")
                        }.buttonStyle(SecondaryButtonStyle())
                    }
                    Button { calls.end() } label: {
                        Label { Text("Hang up") } icon: { CIcon("call_rejected", size: 16) }
                    }.buttonStyle(DangerButtonStyle())
                }
                Text("Back returns to your chats. The call stays active.")
                    .font(.cMeta).foregroundColor(Theme.mutedText).multilineTextAlignment(.center)
            }.padding(24).frame(maxWidth: .infinity)
        }.frame(maxHeight: 480).background(Theme.panel)
    }
}

struct CallDetailView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var calls: MacCalls
    var body: some View {
        if calls.state.active != nil || calls.busy {
            CallControlsView(calls: calls).environmentObject(model)
                .frame(maxWidth: .infinity, maxHeight: .infinity).background(Theme.panel)
        } else {
            Placeholder(icon: "calls", text: "Choose a contact to start an audio call.")
        }
    }
}

struct CallsView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var calls: MacCalls
    @State private var showContacts = false
    @State private var pendingContact: Chat?
    var body: some View {
        let contacts = Dictionary(uniqueKeysWithValues: model.chats.filter { $0.isDirect }.map { ($0.apiId, $0) })
        return VStack(spacing: 0) {
            PaneHeader(title: "Calls", background: Theme.list) {
                CreationButton(help: "New audio call") { showContacts = true }
                    .disabled(calls.state.active != nil || calls.busy)
            }
            if calls.loadingHistory && calls.history.isEmpty {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if calls.history.isEmpty {
                Placeholder(icon: "calls", text: "Your recent calls will appear here.")
            } else {
                ScrollView {
                    LazyVStack(spacing: 2) {
                        ForEach(calls.history) { call in
                            let contact = contacts[call.contactId]
                            Button {
                                if let contact = contact { model.destination = .chats; model.selection = contact.ref }
                            } label: {
                                HStack(spacing: 10) {
                                    Avatar(image: contact?.image, name: contact?.title ?? call.peerName, size: 40, kind: .contact)
                                    VStack(alignment: .leading, spacing: 4) {
                                        Text(contact?.title ?? call.peerName).font(.cBodyBold).foregroundColor(call.missed ? Theme.danger : Theme.primaryText).lineLimit(1)
                                        HStack(spacing: 5) {
                                            CIcon(call.icon, size: 14, color: call.color)
                                            Text(call.durationSecs > 0 ? "\(call.label) · \(Format.duration(call.durationSecs))" : call.label)
                                                .font(.cMeta).foregroundColor(call.color)
                                        }
                                    }
                                    Spacer(minLength: 4)
                                    if let date = call.ts.flatMap(Dates.parse) { Text(Dates.listStamp(date)).font(.cMeta).foregroundColor(Theme.mutedText) }
                                }.padding(10).frame(maxWidth: .infinity, alignment: .leading)
                            }.buttonStyle(GhostTileStyle()).disabled(contact == nil)
                                .padding(.trailing, 38)
                                .overlay(alignment: .trailing) {
                                    if let contact = contact {
                                        IconTool(icon: "calls", help: "Call \(contact.title)", size: 18) { calls.start(contact) }
                                            .disabled(calls.state.active != nil || calls.busy)
                                    }
                                }
                        }
                    }.padding(6)
                        .padding(.bottom, NavDock.clearance)
                }
            }
        }.background(Theme.list)
            .task { await calls.loadHistory() }
            .sheet(isPresented: $showContacts, onDismiss: {
                if let contact = pendingContact { pendingContact = nil; calls.start(contact) }
            }) { NewCallSheet { pendingContact = $0 }.environmentObject(model) }
    }
}

private struct NewCallSheet: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    let choose: (Chat) -> Void
    @State private var search = ""
    private var contacts: [Chat] {
        model.chats.filter { $0.isDirect && $0.pendingRequestId == nil &&
            (search.isEmpty || $0.title.localizedCaseInsensitiveContains(search)) }.sorted { $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending }
    }
    var body: some View {
        VStack(spacing: 14) {
            Text("New audio call").font(.cTitle)
            CursusField(placeholder: "Search contacts", text: $search)
            ScrollView {
                LazyVStack(spacing: 3) {
                    ForEach(contacts) { contact in
                        Button {
                            choose(contact)
                            dismiss()
                        } label: {
                            HStack(spacing: 12) {
                                Avatar(image: contact.image, name: contact.title, size: 36, kind: .contact)
                                Text(contact.title).font(.cBody).foregroundColor(Theme.primaryText)
                                Spacer()
                            }.padding(10)
                        }.buttonStyle(GhostTileStyle())
                    }
                    if contacts.isEmpty { Text("No contacts found.").font(.cMeta).foregroundColor(Theme.secondaryText).padding() }
                }
            }.frame(height: 280)
            Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction).buttonStyle(SecondaryButtonStyle())
        }.padding(22).frame(width: 380).background(Theme.panel)
    }
}
