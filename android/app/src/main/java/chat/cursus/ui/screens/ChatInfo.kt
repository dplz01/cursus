package chat.cursus.ui.screens

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import chat.cursus.AppModel
import chat.cursus.R
import chat.cursus.core.Chat
import chat.cursus.core.ConnLink
import chat.cursus.core.Member
import chat.cursus.ui.components.ActionTile
import chat.cursus.ui.components.Avatar
import chat.cursus.ui.components.CardTitle
import chat.cursus.ui.components.ChoiceChip
import chat.cursus.ui.components.ChoiceMenu
import chat.cursus.ui.components.CursusCard
import chat.cursus.ui.components.DangerButton
import chat.cursus.ui.components.EditField
import chat.cursus.ui.components.EditableLine
import chat.cursus.ui.components.FieldEditor
import chat.cursus.ui.components.HeaderIcon
import chat.cursus.ui.components.IconDisc
import chat.cursus.ui.components.LineStyle
import chat.cursus.ui.components.LinkBox
import chat.cursus.ui.components.NavRow
import chat.cursus.ui.components.PageColumn
import chat.cursus.ui.components.PrefRow
import chat.cursus.ui.components.PrimaryButton
import chat.cursus.ui.components.ProfileIdentity
import chat.cursus.ui.components.PhotoActions
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import chat.cursus.ui.components.ProfileName
import chat.cursus.ui.components.SecondaryButton
import chat.cursus.ui.components.SectionLabel
import chat.cursus.ui.components.SwitchRow
import chat.cursus.ui.theme.Cursus
import chat.cursus.ui.theme.Radius
import kotlinx.coroutines.launch

/** Contact, group or channel page for the open chat. */
@Composable
fun ChatInfoPage(model: AppModel, chat: Chat, onClose: () -> Unit, onOpenChat: (String) -> Unit) {
    when {
        chat.isGroup -> GroupPage(model, chat, onClose, onOpenChat)
        chat.isContact -> ContactPage(model, chat, onClose)
        else -> NotesPage(model, chat, onClose)
    }
}

// ── shared pieces ─────────────────────────────────────────────────────

@Composable
private fun PageScaffold(title: String, onBack: () -> Unit, actions: @Composable () -> Unit = {}, body: @Composable ColumnScope.() -> Unit) {
    Column(Modifier.fillMaxSize().background(Cursus.colors.bgPanel)) {
        PaneHeader(title, onBack = onBack) { actions() }
        Column(Modifier.verticalScroll(rememberScrollState())) { PageColumn(content = body) }
    }
}

/** Linux `button_card`: rows span the card edge to edge. */
@Composable
private fun ButtonCard(content: @Composable ColumnScope.() -> Unit) {
    val c = Cursus.colors
    Column(
        Modifier.fillMaxWidth().clip(Radius.lg).background(c.bgElevated).border(1.dp, c.border, Radius.lg).padding(vertical = 2.dp),
        content = content,
    )
}

/** Linux pane confirm (icon, sentence, Cancel / solid action). */
@Composable
private fun ConfirmPane(icon: Int, danger: Boolean, text: String, action: String, onCancel: () -> Unit, onConfirm: () -> Unit) {
    Column(
        Modifier.fillMaxWidth().padding(top = 24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Icon(painterResource(icon), null, Modifier.size(28.dp), tint = if (danger) Cursus.colors.danger else Cursus.colors.textSecondary)
        Text(text, style = Cursus.type.body, color = Cursus.colors.textPrimary, textAlign = TextAlign.Center)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            SecondaryButton("Cancel", onCancel)
            DangerButton(action, onConfirm)
        }
    }
}

private fun allowLabel(v: String) = when (v) {
    "yes" -> "on"
    "always" -> "always"
    else -> "off"
}

private fun receiptsLabel(v: Boolean?) = when (v) {
    null -> "Default"
    true -> "On"
    false -> "Off"
}

/** Disappearing-message times offered in the pickers. */
private val TTLS = listOf(
    3600L to "1 hour", 8 * 3600L to "8 hours", 86400L to "1 day", 7 * 86400L to "1 week", 30 * 86400L to "1 month",
)

private fun ttlLabel(secs: Long) = TTLS.firstOrNull { it.first == secs }?.second ?: when {
    secs % 86400 == 0L -> "${secs / 86400} days"
    secs % 3600 == 0L -> "${secs / 3600} hours"
    else -> "${secs / 60} min"
}

// ── contact ───────────────────────────────────────────────────────────

@Composable
private fun ContactPage(model: AppModel, chat: Chat, onClose: () -> Unit) {
    val c = Cursus.colors
    var prefs by remember { mutableStateOf(false) }
    var editing by remember { mutableStateOf<EditField?>(null) }
    var confirm by remember { mutableStateOf<String?>(null) }
    if (prefs) {
        BackHandler { prefs = false }
        ContactPreferencesPage(model, chat) { prefs = false }
        return
    }
    PageScaffold("Contact", onClose, actions = {
        HeaderIcon(if (chat.favorite) R.drawable.ic_star_filled else R.drawable.ic_star, "Favourite", chat.favorite) { model.setFavorite(chat, !chat.favorite) }
        HeaderIcon(if (chat.muted) R.drawable.ic_notifications_off else R.drawable.ic_notifications_off_outline, "Mute", chat.muted) { model.setMuted(chat, !chat.muted) }
        HeaderIcon(R.drawable.ic_settings, "Chat preferences") { prefs = true }
    }) {
        CursusCard {
            ProfileIdentity(chat.name, chat.image) {
                ProfileName(chat.name)
                EditableLine(chat.alias, "Add a local name", LineStyle.Sub) { editing = EditField.ContactAlias }
            }
            if (chat.shortDescr.isNotBlank()) {
                SectionLabel("Bio")
                Text(chat.shortDescr, style = Cursus.type.body, color = c.textSecondary, modifier = Modifier.padding(horizontal = 8.dp))
            }
            if (chat.description.isNotBlank()) {
                SectionLabel("About")
                Text(chat.description, style = Cursus.type.body, color = c.textSecondary, modifier = Modifier.padding(horizontal = 8.dp))
            }
        }
        ButtonCard {
            NavRow(R.drawable.ic_settings, "Chat preferences") { prefs = true }
        }
        ButtonCard {
            ActionTile(R.drawable.ic_clear_all, "Clear chat", { confirm = "clear" })
            ActionTile(R.drawable.ic_trash_delete, "Delete contact", { confirm = "delete" }, danger = true)
        }
    }
    editing?.let { f ->
        FieldEditor(f, chat.alias, { editing = null }) { model.setAlias(chat, it); editing = null }
    }
    when (confirm) {
        "clear" -> ConfirmDialog("Clear chat?", "All messages are deleted from this phone.", "Clear",
            { confirm = null; model.clear(chat) }, { confirm = null })
        "delete" -> ConfirmDialog("Delete ${chat.title}?", "The contact and your chat are deleted. This cannot be undone.", "Delete",
            { confirm = null; onClose(); model.delete(chat) }, { confirm = null })
    }
}

private val CONTACT_FEATURES = listOf(
    "timedMessages" to "Disappearing messages",
    "fullDelete" to "Full deletion",
    "reactions" to "Message reactions",
    "voice" to "Voice messages",
    "calls" to "Audio/video calls",
)

private val FEATURE_ICONS = mapOf(
    "timedMessages" to R.drawable.ic_timer,
    "directMessages" to R.drawable.ic_user_member,
    "fullDelete" to R.drawable.ic_trash_delete,
    "reactions" to R.drawable.ic_reaction_emoji_add,
    "voice" to R.drawable.ic_mic,
    "calls" to R.drawable.ic_calls,
    "files" to R.drawable.ic_image_square,
    "simplexLinks" to R.drawable.ic_link_minimalistic_2,
    "history" to R.drawable.ic_history,
    "support" to R.drawable.ic_admin,
)

@Composable
private fun ContactPreferencesPage(model: AppModel, chat: Chat, onBack: () -> Unit) {
    val c = Cursus.colors
    var menu by remember { mutableStateOf<String?>(null) }
    val prefs = chat.contactPrefs
    PageScaffold("Preferences", onBack) {
        CursusCard {
            CardTitle(R.drawable.ic_settings, "Chat features", "Peer icon: red / green / gold. Changes save immediately.")
            for ((key, label) in CONTACT_FEATURES) {
                val f = prefs?.features?.get(key)
                PrefRow(label, FEATURE_ICONS[key]) {
                    ChoiceChip(allowLabel(f?.mine ?: "no"), { menu = key })
                    // What the contact allows: red no, green yes, gold always.
                    Icon(
                        painterResource(R.drawable.ic_user_alt_1), "Contact: ${allowLabel(f?.contact ?: "no")}",
                        Modifier.size(18.dp),
                        tint = when (f?.contact) {
                            "yes" -> c.success
                            "always" -> c.gold
                            else -> c.danger
                        },
                    )
                }
                if (key == "timedMessages" && f?.mine != null && f.mine != "no") {
                    PrefRow("Delete after") { ChoiceChip(ttlLabel(prefs.timedTtl), { menu = "ttl" }) }
                }
            }
        }
        CursusCard {
            PrefRow("Send receipts", R.drawable.ic_check_read) { ChoiceChip(receiptsLabel(chat.sendReceipts), { menu = "receipts" }) }
        }
    }
    when (val m = menu) {
        null -> Unit
        "ttl" -> ChoiceMenu("Delete after", TTLS, prefs?.timedTtl, { menu = null }) { model.setContactTimedTtl(chat, it); menu = null }
        "receipts" -> ChoiceMenu("Send receipts", listOf<Pair<Boolean?, String>>(null to "Default", true to "On", false to "Off"), chat.sendReceipts, { menu = null }) {
            model.setSendReceipts(chat, it); menu = null
        }
        else -> ChoiceMenu(
            CONTACT_FEATURES.first { it.first == m }.second,
            listOf("always" to "always", "yes" to "on", "no" to "off"),
            prefs?.features?.get(m)?.mine,
            { menu = null },
        ) { model.setContactFeature(chat, m, it); menu = null }
    }
}

@Composable
private fun NotesPage(model: AppModel, chat: Chat, onClose: () -> Unit) {
    var confirm by remember { mutableStateOf(false) }
    PageScaffold("Notes", onClose) {
        CursusCard { Text("Notes to yourself, kept only on this phone.", style = Cursus.type.body, color = Cursus.colors.textSecondary) }
        ButtonCard { ActionTile(R.drawable.ic_clear_all, "Clear chat", { confirm = true }) }
    }
    if (confirm) ConfirmDialog("Clear notes?", "All notes are deleted from this phone.", "Clear", { confirm = false; model.clear(chat) }, { confirm = false })
}

// ── group / channel ───────────────────────────────────────────────────

private enum class GroupPane { Overview, AddMember, Preferences, MemberInfo, ChannelMembers, SupportList, ConfirmLeave, ConfirmDelete }

@Composable
private fun GroupPage(model: AppModel, chat: Chat, onClose: () -> Unit, onOpenChat: (String) -> Unit) {
    var pane by remember { mutableStateOf(GroupPane.Overview) }
    var memberId by remember { mutableStateOf<Long?>(null) }
    LaunchedEffect(chat.ref) { model.loadMembers(chat) }
    val back = { pane = GroupPane.Overview }
    if (pane != GroupPane.Overview) BackHandler(onBack = back)
    val noun = if (chat.isChannel) "channel" else "group"
    when (pane) {
        GroupPane.Overview -> GroupOverview(model, chat, onClose, onOpenPane = { pane = it }, onMember = { memberId = it; pane = GroupPane.MemberInfo })
        GroupPane.AddMember -> PageScaffold("Invite", back) { AddMemberPane(model, chat) }
        GroupPane.Preferences -> PageScaffold("Group preferences", back) { GroupPreferencesPane(model, chat) }
        GroupPane.MemberInfo -> PageScaffold("Member", back) {
            val m = model.members.firstOrNull { it.id == memberId }
            if (m == null) Text("Member not found.", style = Cursus.type.body, color = Cursus.colors.textMuted)
            else MemberInfoPane(model, chat, m, onOpenChat)
        }
        GroupPane.ChannelMembers -> PageScaffold(if (chat.canEditProfile) "Subscribers" else "Owners", back) {
            CursusCard {
                MembersList(model.members.filter { chat.canEditProfile || it.role == "owner" }) { memberId = it; pane = GroupPane.MemberInfo }
            }
        }
        GroupPane.SupportList -> PageScaffold("Chats with members", back) {
            // Members waiting for review or with a support thread (Linux support list).
            val list = model.members.filter { !it.isMe && (it.pending || it.support != null) }
            CursusCard {
                if (list.isEmpty()) {
                    Text("No join requests or member chats.", style = Cursus.type.body, color = Cursus.colors.textMuted, modifier = Modifier.padding(8.dp))
                } else {
                    MembersList(list) { id -> onOpenChat("${chat.ref}(_support:$id)") }
                }
            }
        }
        GroupPane.ConfirmLeave -> PageScaffold("Leave $noun", back) {
            ConfirmPane(R.drawable.ic_session_leave, false, "Leave this $noun? You will need a new invite to rejoin.", "Leave", back) {
                model.leaveGroup(chat); back()
            }
        }
        GroupPane.ConfirmDelete -> PageScaffold("Delete $noun", back) {
            ConfirmPane(R.drawable.ic_trash_delete, true, "Delete this $noun for yourself? This cannot be undone.", "Delete", back) {
                onClose(); model.delete(chat)
            }
        }
    }
}

@Composable
private fun GroupOverview(model: AppModel, chat: Chat, onClose: () -> Unit, onOpenPane: (GroupPane) -> Unit, onMember: (Long) -> Unit) {
    val c = Cursus.colors
    val owner = chat.canEditProfile
    val admin = chat.canManageMembers
    val moderate = chat.canModerate
    val supportOk = chat.groupPrefs?.features?.get("support")?.on == true
    var editing by remember { mutableStateOf<EditField?>(null) }
    var confirmClear by remember { mutableStateOf(false) }
    PageScaffold(if (chat.isChannel) "Channel" else "Group", onClose, actions = {
        HeaderIcon(if (chat.favorite) R.drawable.ic_star_filled else R.drawable.ic_star, "Favourite", chat.favorite) { model.setFavorite(chat, !chat.favorite) }
        HeaderIcon(if (chat.muted) R.drawable.ic_notifications_off else R.drawable.ic_notifications_off_outline, "Mute", chat.muted) { model.setMuted(chat, !chat.muted) }
        HeaderIcon(R.drawable.ic_settings, "Group preferences") { onOpenPane(GroupPane.Preferences) }
    }) {
        // Profile card
        CursusCard {
            val pickPhoto = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri ->
                uri?.let { model.withProfileImage(it) { url -> model.setGroupImage(chat, url) } }
            }
            val photo = if (owner) PhotoActions(
                hasImage = chat.image != null,
                onPick = { pickPhoto.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) },
                onClear = { model.setGroupImage(chat, "") },
            ) else null
            ProfileIdentity(chat.name, chat.image, group = true, photo = photo) {
                if (owner) EditableLine(chat.name, "Name", LineStyle.Name) { editing = EditField.GroupName }
                else ProfileName(chat.name)
                EditableLine(chat.alias, "Add a local name", LineStyle.Sub) { editing = EditField.GroupAlias }
            }
            if (owner) {
                SectionLabel("Short description")
                EditableLine(chat.shortDescr, "Add a short description", LineStyle.Body) { editing = EditField.GroupShortDescr }
            } else {
                for ((label, value) in listOf("About" to chat.shortDescr, "Welcome message" to chat.description)) {
                    if (value.isNotBlank()) {
                        SectionLabel(label)
                        Text(value, style = Cursus.type.body, color = c.textSecondary, modifier = Modifier.padding(horizontal = 8.dp))
                    }
                }
            }
        }
        if (chat.isChannel) ChannelLinkCard(model, chat)
        // Support / invite / subscribers
        if (owner || supportOk || moderate || admin || chat.isChannel) {
            ButtonCard {
                if (owner) {
                    val preview = chat.description.split(Regex("\\s+")).filter { it.isNotEmpty() }.joinToString(" ")
                    NavRow(R.drawable.ic_scripture_scroll, "Welcome message", preview) {
                        editing = EditField.GroupWelcome
                    }
                }
                if (supportOk || moderate) {
                    if (moderate) {
                        NavRow(R.drawable.ic_admin, "Chats with members", badge = chat.membersRequireAttention) { onOpenPane(GroupPane.SupportList) }
                    } else {
                        NavRow(R.drawable.ic_admin, "Chat with admins", badge = chat.supportUnread) {
                            model.select("${chat.ref}(_support)"); onClose()
                        }
                    }
                }
                if (admin && !chat.isChannel) {
                    NavRow(R.drawable.ic_user_plus_alt_1, "Invite") { onOpenPane(GroupPane.AddMember) }
                }
                if (chat.isChannel) {
                    NavRow(
                        R.drawable.ic_user_group,
                        if (owner) "Subscribers" else "Owners",
                        if (owner) "${model.members.size} people" else "Who runs this channel",
                    ) { onOpenPane(GroupPane.ChannelMembers) }
                }
            }
        }
        // Members (private groups)
        if (!chat.isChannel) {
            CursusCard {
                val n = model.members.size
                CardTitle(R.drawable.ic_user_group, "Members", if (n == 1) "1 member" else "$n members")
                MembersList(model.members, onMember)
            }
        }
        // Chat actions — a channel's only owner can delete it but not leave it.
        val soleChannelOwner = chat.isChannel && chat.role == "owner" && model.members.none { !it.isMe && it.role == "owner" }
        ButtonCard {
            ActionTile(R.drawable.ic_clear_all, "Clear chat", { confirmClear = true })
            if (!soleChannelOwner) {
                ActionTile(R.drawable.ic_session_leave, if (chat.isChannel) "Leave channel" else "Leave group", { onOpenPane(GroupPane.ConfirmLeave) })
            }
            ActionTile(R.drawable.ic_trash_delete, if (chat.isChannel) "Delete channel" else "Delete group", { onOpenPane(GroupPane.ConfirmDelete) }, danger = true)
        }
    }
    editing?.let { f ->
        val initial = when (f) {
            EditField.GroupName -> chat.name
            EditField.GroupAlias -> chat.alias
            EditField.GroupShortDescr -> chat.shortDescr
            EditField.GroupWelcome -> chat.description
            else -> ""
        }
        FieldEditor(f, initial, { editing = null }) { v ->
            when (f) {
                EditField.GroupName -> model.updateGroupProfile(chat, name = v)
                EditField.GroupAlias -> model.setAlias(chat, v)
                EditField.GroupShortDescr -> model.updateGroupProfile(chat, shortDescr = v)
                EditField.GroupWelcome -> model.updateGroupProfile(chat, welcome = v)
                else -> Unit
            }
            editing = null
        }
    }
    if (confirmClear) {
        ConfirmDialog("Clear chat?", "All messages are deleted from this phone.", "Clear", { confirmClear = false; model.clear(chat) }, { confirmClear = false })
    }
}

@Composable
private fun ChannelLinkCard(model: AppModel, chat: Chat) {
    val c = Cursus.colors
    var link by remember { mutableStateOf<ConnLink?>(null) }
    LaunchedEffect(chat.ref) { if (chat.canManageMembers) link = model.groupLink(chat, quiet = true) }
    CursusCard {
        CardTitle(R.drawable.ic_link_minimalistic_2, "Channel link", "Anyone with this link can join via chat relays.")
        Column(Modifier.padding(top = 8.dp)) {
            val l = link
            when {
                l != null -> LinkBox(l.short ?: l.full) { model.notice = "Link copied" }
                !chat.canManageMembers -> Text("Only the owner can share the channel link.", style = Cursus.type.meta, color = c.textSecondary)
            }
        }
    }
}

/** Linux `group_members_list`: 40px avatar, name (you: YOU (name)), bio, pending clock, role. */
@Composable
private fun MembersList(members: List<Member>, onMember: (Long) -> Unit) {
    val c = Cursus.colors
    Column {
        for (m in members) {
            Row(
                Modifier.fillMaxWidth().clip(Radius.lg).clickable { onMember(m.id) }.padding(horizontal = 4.dp, vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Avatar(m.name, m.image, 40.dp)
                Column(Modifier.weight(1f)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(
                            if (m.isMe) "YOU (${m.publicName.ifBlank { m.name }})" else m.name,
                            style = Cursus.type.body, color = if (m.blocked) c.textMuted else c.textPrimary,
                            maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false),
                        )
                        if (m.invited || m.pending) {
                            Icon(painterResource(R.drawable.ic_system_pending_line), "Pending", Modifier.padding(start = 4.dp).size(16.dp), tint = c.gold)
                        }
                    }
                    if (m.shortDescr.isNotBlank()) {
                        Text(m.shortDescr, style = Cursus.type.meta, color = c.textSecondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                }
                Text(m.role, style = Cursus.type.meta, color = c.textMuted)
            }
        }
    }
}

@Composable
private fun MemberInfoPane(model: AppModel, chat: Chat, m: Member, onOpenChat: (String) -> Unit) {
    val c = Cursus.colors
    var roles by remember { mutableStateOf(false) }
    CursusCard {
        ProfileIdentity(m.name, m.image) {
            ProfileName(if (m.isMe) "YOU (${m.publicName.ifBlank { m.name }})" else m.name)
            if (m.shortDescr.isNotBlank()) {
                Text(m.shortDescr, style = Cursus.type.meta, color = c.textSecondary, modifier = Modifier.padding(horizontal = 8.dp))
            }
        }
        Column(Modifier.padding(top = 8.dp, start = 8.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text("Role: ${m.role}", style = Cursus.type.meta, color = c.textSecondary)
            Text("Status: ${m.status}", style = Cursus.type.meta, color = c.textSecondary)
        }
    }
    if (m.isMe) return
    val outranked = chat.assignableRoles.contains(m.role) || m.pending
    ButtonCard {
        NavRow(R.drawable.ic_chats, "Open direct chat") { model.messageMember(chat, m) { onOpenChat(it) } }
        if (m.pending && chat.canModerate) {
            ActionTile(R.drawable.ic_check, "Accept as member", { model.acceptMember(chat, m, "member") })
            ActionTile(R.drawable.ic_eye_observer, "Accept as observer", { model.acceptMember(chat, m, "observer") })
            ActionTile(R.drawable.ic_cancel, "Reject", { model.removeMember(chat, m, false) }, danger = true)
        }
        if (chat.canManageMembers && outranked && !m.pending && chat.assignableRoles.isNotEmpty()) {
            ActionTile(R.drawable.ic_admin_role, "Change role", { roles = true })
        }
        ActionTile(
            R.drawable.ic_eye_observer,
            if (m.showMessages) "Hide messages (for me)" else "Show messages (for me)",
            { model.setMemberShowMessages(chat, m, !m.showMessages) },
        )
        if (chat.canModerate && outranked) {
            ActionTile(R.drawable.ic_block_prohibited, if (m.blocked) "Unblock for all" else "Block for all", { model.blockMember(chat, m, !m.blocked) })
        }
        if (chat.canManageMembers && outranked && !m.pending) {
            ActionTile(R.drawable.ic_remove_profile, "Remove member", { model.removeMember(chat, m, false) }, danger = true)
            ActionTile(R.drawable.ic_trash_delete, "Remove + delete messages", { model.removeMember(chat, m, true) }, danger = true)
        }
    }
    if (roles) {
        ChoiceMenu("Change role", chat.assignableRoles.map { it to "Set $it" }, m.role, { roles = false }) {
            model.setMemberRole(chat, m, it); roles = false
        }
    }
}

@Composable
private fun AddMemberPane(model: AppModel, chat: Chat) {
    val c = Cursus.colors
    val scope = rememberCoroutineScope()
    var link by remember { mutableStateOf<ConnLink?>(null) }
    var linkRole by remember { mutableStateOf("member") }
    var inviteRole by remember { mutableStateOf("member") }
    var menu by remember { mutableStateOf<String?>(null) }
    if (!chat.canManageMembers) {
        CursusCard { Text("Only admins can manage the invite link.", style = Cursus.type.body, color = c.textSecondary) }
        return
    }
    CursusCard {
        CardTitle(R.drawable.ic_link_minimalistic_2, "Group link", "Anyone with this link can ask to join.")
        Column(Modifier.padding(top = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            val l = link
            if (l == null) {
                PrimaryButton("Create link", { scope.launch { link = model.groupLink(chat, linkRole) } }, Modifier.fillMaxWidth())
            } else {
                PrefRow("Join as") { ChoiceChip(linkRole, { menu = "link" }) }
                LinkBox(l.short ?: l.full) { model.notice = "Link copied" }
                ActionTile(R.drawable.ic_trash_delete, "Delete link", { model.deleteGroupLink(chat); link = null }, danger = true)
            }
        }
    }
    val inGroup = model.members.mapNotNull { it.contactId }.toSet()
    val contacts = model.chats.filter { it.isContact && !it.isRequest && it.apiId !in inGroup }
    CursusCard {
        CardTitle(R.drawable.ic_user_plus_alt_1, "Invite contacts", "They receive an invitation in your direct chat.")
        PrefRow("Invite as") { ChoiceChip(inviteRole, { menu = "invite" }) }
        if (contacts.isEmpty()) {
            Text("Everyone you know is already here", style = Cursus.type.body, color = c.textPrimary, modifier = Modifier.padding(8.dp))
            Text("Connect with someone in Chats to invite them.", style = Cursus.type.meta, color = c.textSecondary, modifier = Modifier.padding(horizontal = 8.dp))
        } else {
            for (contact in contacts) {
                Row(
                    Modifier.fillMaxWidth().clip(Radius.lg).clickable { model.addMember(chat, contact, inviteRole) }.padding(4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    Avatar(contact.title, contact.image, 40.dp)
                    Text(contact.title, style = Cursus.type.body, color = c.textPrimary, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
                    IconDisc(R.drawable.ic_plus)
                }
            }
        }
    }
    when (menu) {
        "link" -> ChoiceMenu("Join as", listOf("observer", "member").map { it to it }, linkRole, { menu = null }) {
            linkRole = it; menu = null; model.setGroupLinkRole(chat, it)
        }
        "invite" -> ChoiceMenu("Invite as", chat.assignableRoles.map { it to it }, inviteRole, { menu = null }) { inviteRole = it; menu = null }
    }
}

private val GROUP_FEATURES = listOf(
    "timedMessages" to "Disappearing messages",
    "directMessages" to "Direct messages",
    "fullDelete" to "Full deletion",
    "reactions" to "Message reactions",
    "voice" to "Voice messages",
    "files" to "Files and media",
    "simplexLinks" to "SimpleX links",
    "history" to "Recent history",
    "support" to "Chat with admins",
)

@Composable
private fun GroupPreferencesPane(model: AppModel, chat: Chat) {
    val owner = chat.canEditProfile
    val prefs = chat.groupPrefs
    var menu by remember { mutableStateOf<String?>(null) }
    CursusCard {
        CardTitle(
            R.drawable.ic_settings, "Chat features",
            if (owner) "Owners change these for everyone. Changes save immediately." else "View only — only owners can change preferences.",
        )
        for ((key, label) in GROUP_FEATURES) {
            val on = prefs?.features?.get(key)?.on == true
            PrefRow(label, FEATURE_ICONS[key]) {
                androidx.compose.material3.Switch(
                    checked = on, enabled = owner,
                    onCheckedChange = { model.setGroupFeature(chat, key, it) },
                    colors = androidx.compose.material3.SwitchDefaults.colors(
                        checkedTrackColor = Cursus.colors.accent, checkedThumbColor = Cursus.colors.accentOn,
                        uncheckedTrackColor = Cursus.colors.bgField, uncheckedBorderColor = Cursus.colors.border,
                        uncheckedThumbColor = Cursus.colors.textMuted,
                    ),
                )
            }
            if (key == "timedMessages" && on) {
                PrefRow("Delete after") { ChoiceChip(ttlLabel(prefs?.timedTtl ?: 86400), { menu = "ttl" }, enabled = owner) }
            }
        }
    }
    if (owner && !chat.isChannel) {
        CursusCard {
            CardTitle(R.drawable.ic_user_question_alt_1, "Joining", "How new members get in.")
            SwitchRow("Review members before join", chat.memberReview, { model.setMemberReview(chat, it) })
        }
    }
    CursusCard {
        CardTitle(R.drawable.ic_user_alt_1, "Your settings", "Only for you — other members don't see these.")
        PrefRow("Send receipts") { ChoiceChip(receiptsLabel(chat.sendReceipts), { menu = "receipts" }) }
    }
    when (menu) {
        "ttl" -> ChoiceMenu("Delete after", TTLS, prefs?.timedTtl, { menu = null }) { model.setGroupFeature(chat, "timedMessages", true, it); menu = null }
        "receipts" -> ChoiceMenu("Send receipts", listOf<Pair<Boolean?, String>>(null to "Default", true to "On", false to "Off"), chat.sendReceipts, { menu = null }) {
            model.setSendReceipts(chat, it); menu = null
        }
    }
}
