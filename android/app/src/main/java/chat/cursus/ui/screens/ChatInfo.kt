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
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
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
import chat.cursus.ui.components.ProfileText
import chat.cursus.AppModel
import chat.cursus.R
import chat.cursus.core.Chat
import chat.cursus.core.Core
import chat.cursus.ui.components.RoundedField
import chat.cursus.core.ConnLink
import chat.cursus.core.Member
import chat.cursus.ui.components.IconTool
import chat.cursus.ui.components.CursusSwitch
import chat.cursus.ui.components.AboutBlock
import chat.cursus.ui.components.HeroLine
import chat.cursus.ui.components.QuickAction
import chat.cursus.ui.components.QuickActions
import chat.cursus.ui.components.QuickActionTools
import chat.cursus.ui.components.isParagraph
import chat.cursus.ui.components.ActionTile
import chat.cursus.ui.components.Avatar
import chat.cursus.ui.components.CardTitle
import chat.cursus.ui.components.ChoiceChip
import chat.cursus.ui.components.ChoiceMenu
import chat.cursus.ui.components.CursusCard
import chat.cursus.ui.components.Panel
import chat.cursus.ui.components.ChoiceRow
import chat.cursus.ui.components.SectionHeading
import chat.cursus.ui.components.SectionNote
import chat.cursus.ui.components.SectionInset
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
import chat.cursus.ui.components.PreferenceGroup
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
private fun PageScaffold(
    title: String,
    onBack: () -> Unit,
    actions: @Composable () -> Unit = {},
    scroll: androidx.compose.foundation.ScrollState = rememberScrollState(),
    body: @Composable ColumnScope.() -> Unit,
) {
    Column(Modifier.fillMaxSize().background(Cursus.colors.bgPanel)) {
        PaneHeader(title, onBack = onBack) { actions() }
        Column(Modifier.verticalScroll(scroll)) { PageColumn(content = body) }
    }
}

/** A section made only of rows. */
@Composable
private fun ButtonCard(content: @Composable ColumnScope.() -> Unit) = Panel(content = content)

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

/** List + detail shell: profile actions go to the header as small tools. */
@Composable
private fun wideProfile() = PaneLayout.isSplit(androidx.compose.ui.platform.LocalConfiguration.current.screenWidthDp.toFloat())

private fun allowLabel(v: String) = when (v) {
    "yes" -> "On"
    "always" -> "Always"
    else -> "Off"
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

/** "Delete after (this device only)": the Linux choices; null = never. */
private val LOCAL_TTLS = listOf<Pair<Long?, String>>(
    null to "Never", 30L to "30 seconds", 300L to "5 minutes", 3600L to "1 hour",
    86400L to "1 day", 7 * 86400L to "1 week", 30 * 86400L to "1 month",
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
    val quick = listOf(
        QuickAction(if (chat.favorite) R.drawable.ic_star_filled else R.drawable.ic_star, if (chat.favorite) "Starred" else "Star", chat.favorite) {
            model.setFavorite(chat, !chat.favorite)
        },
        QuickAction(
            if (chat.muted) R.drawable.ic_notifications_off else R.drawable.ic_notifications_off_outline,
            if (chat.muted) "Muted" else "Mute", chat.muted,
        ) { model.setMuted(chat, !chat.muted) },
        QuickAction(R.drawable.ic_settings, "Settings") { prefs = true },
    )
    val wide = wideProfile()
    PageScaffold("Contact", onClose, actions = { if (wide) QuickActionTools(quick) }) {
        ProfileIdentity(chat.name, chat.image) {
            ProfileName(chat.name)
            EditableLine(chat.alias, "Add a local name", LineStyle.Sub, rich = false) { editing = EditField.ContactAlias }
            if (chat.shortDescr.isNotBlank()) HeroLine(chat.shortDescr)
        }
        if (!wide) QuickActions(*quick.toTypedArray())
        if (chat.description.isNotBlank()) AboutBlock("About", chat.description)
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
    "fullDelete" to "Delete for everyone",
    "reactions" to "Message reactions",
    "voice" to "Voice messages",
    "calls" to "Audio and video calls",
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
    PageScaffold("Settings", onBack) {
        PreferenceGroup("Privacy") {
            ChoiceRow("Delivery receipts", receiptsLabel(chat.sendReceipts), R.drawable.ic_check_read) { menu = "receipts" }
        }
        PreferenceGroup("Chat permissions") {
            for ((key, label) in CONTACT_FEATURES) {
                val f = prefs?.features?.get(key)
                // One centred line per row; what the contact allows is in the
                // choice sheet (shared wording from cursus_core via the bridge).
                ChoiceRow(label, f?.mineLabel ?: allowLabel(f?.mine ?: "no"), FEATURE_ICONS[key]) { menu = key }
                // "on": the time both sides agreed on. "always": your own
                // one-way timer, deleting from this phone only (as on Linux).
                if (key == "timedMessages" && f?.mine == "yes") {
                    ChoiceRow("Delete after", ttlLabel(prefs.timedTtl), R.drawable.ic_system_pending_line) { menu = "ttl" }
                }
                if (key == "timedMessages" && f?.mine == "always") {
                    val local = chat.chatItemTtl?.takeIf { it > 0 }
                    ChoiceRow("Delete after (this device only)", local?.let { ttlLabel(it) } ?: "Never", R.drawable.ic_system_pending_line) { menu = "localTtl" }
                }
            }
        }
    }
    when (val m = menu) {
        null -> Unit
        "ttl" -> ChoiceMenu("Delete after", TTLS, prefs?.timedTtl, { menu = null }) { model.setContactTimedTtl(chat, it); menu = null }
        "localTtl" -> ChoiceMenu("Delete after (this device only)", LOCAL_TTLS, chat.chatItemTtl?.takeIf { it > 0 }, { menu = null }) {
            model.setChatTtl(chat, it); menu = null
        }
        "receipts" -> ChoiceMenu("Delivery receipts", listOf<Pair<Boolean?, String>>(null to "Default", true to "On", false to "Off"), chat.sendReceipts, { menu = null },
            note = "Let this contact know their messages arrived.") {
            model.setSendReceipts(chat, it); menu = null
        }
        else -> ChoiceMenu(
            CONTACT_FEATURES.first { it.first == m }.second,
            listOf("always" to "Always", "yes" to "On", "no" to "Off"),
            prefs?.features?.get(m)?.mine,
            { menu = null },
            note = prefs?.features?.get(m)?.let { it.contactNote ?: it.contactSummary },
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

private enum class GroupPane { Overview, Link, AddMember, Preferences, MemberInfo, ChannelMembers, SupportList, ConfirmLeave, ConfirmDelete }

@Composable
private fun GroupPage(model: AppModel, chat: Chat, onClose: () -> Unit, onOpenChat: (String) -> Unit) {
    var pane by remember { mutableStateOf(GroupPane.Overview) }
    var memberId by remember { mutableStateOf<Long?>(null) }
    // Held here, not in the overview: sub-pages replace the overview, and
    // coming back must land where you left it (Linux keeps its offset too).
    val overviewScroll = rememberScrollState()
    LaunchedEffect(chat.ref) { model.loadMembers(chat) }
    val back = { pane = GroupPane.Overview }
    if (pane != GroupPane.Overview) BackHandler(onBack = back)
    val noun = if (chat.isChannel) "channel" else "group"
    when (pane) {
        GroupPane.Overview -> GroupOverview(model, chat, onClose, overviewScroll, onOpenPane = { pane = it }, onMember = { memberId = it; pane = GroupPane.MemberInfo })
        GroupPane.Link -> PageScaffold(if (chat.isChannel) "Share this channel" else "Share this group", back) {
            GroupSharePane(model, chat)
        }
        GroupPane.AddMember -> PageScaffold("Invite a contact", back) { AddMemberPane(model, chat) }
        GroupPane.Preferences -> PageScaffold("Group settings", back) { GroupPreferencesPane(model, chat) }
        GroupPane.MemberInfo -> PageScaffold("Member", back) {
            val m = model.members.firstOrNull { it.id == memberId }
            if (m == null) Text("Member not found.", style = Cursus.type.body, color = Cursus.colors.textMuted)
            else MemberInfoPane(model, chat, m, onOpenChat)
        }
        GroupPane.ChannelMembers -> PageScaffold(if (chat.canEditProfile) "Subscribers" else "Owners", back) {
            Panel {
                MembersList(model.members.filter { chat.canEditProfile || it.role == "owner" }) { memberId = it; pane = GroupPane.MemberInfo }
            }
        }
        GroupPane.SupportList -> PageScaffold("Chats with members", back) {
            // Members waiting for review or with a support thread (Linux support list).
            val list = model.members.filter { !it.isMe && (it.pending || it.support != null) }
            Panel {
                if (list.isEmpty()) {
                    Text("No join requests or member chats.", style = Cursus.type.body, color = Cursus.colors.textMuted, modifier = Modifier.padding(16.dp))
                } else {
                    var closing by remember { mutableStateOf<Member?>(null) }
                    // Moderators can close a member's thread (not a join request).
                    MembersList(list, trailing = { m ->
                        if (chat.canModerate && m.support != null && !m.pending) {
                            IconTool(R.drawable.ic_trash_delete, "Delete chat", { closing = m }, tint = Cursus.colors.danger)
                        }
                    }) { id -> onOpenChat("${chat.ref}(_support:$id)") }
                    closing?.let { m ->
                        ConfirmDialog("Delete the chat with ${m.name}?", "The messages you and the other admins exchanged with this member are deleted.", "Delete",
                            { closing = null; model.deleteSupportChat(chat, m) }, { closing = null })
                    }
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
private fun GroupOverview(model: AppModel, chat: Chat, onClose: () -> Unit, scroll: androidx.compose.foundation.ScrollState, onOpenPane: (GroupPane) -> Unit, onMember: (Long) -> Unit) {
    val c = Cursus.colors
    val owner = chat.canEditProfile
    val admin = chat.canManageMembers
    val moderate = chat.canModerate
    val supportOk = chat.groupPrefs?.features?.get("support")?.on == true
    var editing by remember { mutableStateOf<EditField?>(null) }
    var confirmClear by remember { mutableStateOf(false) }
    val quick = listOf(
        QuickAction(if (chat.favorite) R.drawable.ic_star_filled else R.drawable.ic_star, if (chat.favorite) "Starred" else "Star", chat.favorite) {
            model.setFavorite(chat, !chat.favorite)
        },
        QuickAction(
            if (chat.muted) R.drawable.ic_notifications_off else R.drawable.ic_notifications_off_outline,
            if (chat.muted) "Muted" else "Mute", chat.muted,
        ) { model.setMuted(chat, !chat.muted) },
        QuickAction(R.drawable.ic_settings, "Settings") { onOpenPane(GroupPane.Preferences) },
    )
    val wide = wideProfile()
    val paragraph = isParagraph(chat.shortDescr)
    PageScaffold(if (chat.isChannel) "Channel" else "Group", onClose, actions = { if (wide) QuickActionTools(quick) }, scroll = scroll) {
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
                EditableLine(chat.alias, "Add a local name", LineStyle.Sub, rich = false) { editing = EditField.GroupAlias }
                // The short description is the line under the name, for everyone
                // (a paragraph-long one gets its own block under the actions).
                if (owner && !paragraph) EditableLine(chat.shortDescr, "Add a short description", LineStyle.Sub) { editing = EditField.GroupShortDescr }
                else if (!paragraph && chat.shortDescr.isNotBlank()) HeroLine(chat.shortDescr)
            }
        }
        if (!wide) QuickActions(*quick.toTypedArray())
        if (paragraph) {
            if (owner) Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                SectionHeading("Description")
                Panel {
                    Row(
                        Modifier.fillMaxWidth().clip(Radius.insetControl).clickable { editing = EditField.GroupShortDescr }
                            .padding(horizontal = 16.dp, vertical = 12.dp),
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        ProfileText(chat.shortDescr, style = Cursus.type.body, color = c.textSecondary, modifier = Modifier.weight(1f))
                        Icon(painterResource(R.drawable.ic_edit_3), "Edit", Modifier.size(16.dp), tint = c.textMuted)
                    }
                }
            } else AboutBlock("Description", chat.shortDescr)
        }
        if (!owner && chat.description.isNotBlank()) AboutBlock("Welcome message", chat.description)
        // Support / invite / subscribers
        if (owner || supportOk || moderate || admin || chat.isChannel) {
            ButtonCard {
                if (owner) {
                    val preview = chat.description.split(Regex("\\s+")).filter { it.isNotEmpty() }.joinToString(" ")
                    NavRow(R.drawable.ic_scripture_scroll, "Welcome message", preview, richSubtitle = true) {
                        editing = EditField.GroupWelcome
                    }
                }
                if (supportOk || moderate) {
                    if (moderate) {
                        NavRow(R.drawable.ic_flag_outline, "Chats with members", badge = chat.membersRequireAttention) { onOpenPane(GroupPane.SupportList) }
                    } else {
                        NavRow(R.drawable.ic_flag_outline, "Chat with admins", badge = chat.supportUnread) {
                            model.select("${chat.ref}(_support)"); onClose()
                        }
                    }
                }
                if (admin || chat.isChannel) {
                    NavRow(R.drawable.ic_share_03, if (chat.isChannel) "Share this channel" else "Share this group") { onOpenPane(GroupPane.Link) }
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
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                val n = model.members.size
                SectionHeading(if (n == 1) "1 member" else "$n members")
                Panel {
                    if (admin) NavRow(R.drawable.ic_user_plus_alt_1, "Invite a contact") { onOpenPane(GroupPane.AddMember) }
                    MembersList(model.members, onMember = onMember)
                }
            }
        }
        // Chat actions — a channel's only owner can delete it but not leave it.
        val soleChannelOwner = chat.isChannel && chat.role == "owner" && model.members.none { !it.isMe && it.role == "owner" }
        ButtonCard {
            ActionTile(R.drawable.ic_clear_all, "Clear chat", { confirmClear = true })
            if (!soleChannelOwner) {
                ActionTile(R.drawable.ic_session_leave, if (chat.isChannel) "Leave channel" else "Leave group", { onOpenPane(GroupPane.ConfirmLeave) }, danger = true)
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

/** Same hierarchy as the Mac share page: explanation, link, share, join role. */
@Composable
private fun GroupSharePane(model: AppModel, chat: Chat) {
    val c = Cursus.colors
    val scope = rememberCoroutineScope()
    val context = androidx.compose.ui.platform.LocalContext.current
    var link by remember(chat.ref) { mutableStateOf<ConnLink?>(null) }
    var loading by remember(chat.ref) { mutableStateOf(true) }
    var role by remember(chat.ref) { mutableStateOf("member") }
    var roles by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }
    var deleting by remember { mutableStateOf(false) }
    suspend fun load() {
        loading = true
        try { link = model.groupLink(chat, role, quiet = !chat.canManageMembers) }
        finally { loading = false }
    }
    LaunchedEffect(chat.ref) { load() }
    // The page title already names it: a note, the link, then its panel.
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        SectionNote(if (chat.isChannel) "Anyone with this link can subscribe." else "Anyone with this link can join the group.")
        when {
            loading -> CircularProgressIndicator(Modifier.padding(12.dp).size(24.dp), color = c.accent)
            link != null -> {
                val value = link!!.short ?: link!!.full
                LinkBox(value) { model.notice = "Link copied" }
            }
            chat.canManageMembers && !chat.isChannel -> PrimaryButton("Create link", {
                if (!loading) { loading = true; scope.launch { load() } }
            }, enabled = !loading)
            chat.isChannel -> Text("No channel link available. Create a new channel to invite people.",
                style = Cursus.type.meta, color = c.textMuted, modifier = Modifier.padding(12.dp))
            else -> Text("No link yet — ask an admin.", style = Cursus.type.meta, color = c.textMuted,
                modifier = Modifier.padding(12.dp))
        }
        link?.let { l ->
            val value = l.short ?: l.full
            Panel(Modifier.padding(top = 4.dp)) {
                // Android shares through the system sheet (Linux: clipboard).
                ActionTile(R.drawable.ic_share_03, "Share link", {
                    val intent = android.content.Intent(android.content.Intent.ACTION_SEND)
                        .setType("text/plain").putExtra(android.content.Intent.EXTRA_TEXT, value)
                    context.startActivity(android.content.Intent.createChooser(intent, "Share link"))
                })
                if (!chat.isChannel && chat.canManageMembers) {
                    NavRow(roleIcon(role), "People joining become", value = role.replaceFirstChar { it.titlecase() }) { roles = true }
                    ActionTile(R.drawable.ic_trash_delete, if (deleting) "Deleting…" else "Delete link", {
                        if (!deleting) confirmDelete = true
                    }, danger = true)
                }
            }
        }
    }
    if (roles) ChoiceMenu("People joining become", listOf("member" to "Member", "observer" to "Observer"), role, { roles = false }) {
        role = it; roles = false; model.setGroupLinkRole(chat, it)
    }
    if (confirmDelete) ConfirmDialog("Delete the group link?", "Nobody can join with it any more. Members stay in the group.",
        "Delete link", {
            confirmDelete = false; deleting = true
            scope.launch {
                try { if (model.deleteGroupLink(chat)) link = null }
                finally { deleting = false }
            }
        }, { confirmDelete = false })
}

/** Same glyph per role on every platform (Linux `icons::role_icon`). */
private fun roleIcon(role: String) = when (role.lowercase()) {
    "observer" -> R.drawable.ic_eye_observer
    "moderator" -> R.drawable.ic_shield_check
    "admin" -> R.drawable.ic_admin_role
    "owner" -> R.drawable.ic_owner
    else -> R.drawable.ic_user_member
}

/** Linux `group_members_list`: 40px avatar, name (you: YOU (name)), bio, pending clock, role. */
@Composable
private fun MembersList(members: List<Member>, trailing: @Composable (Member) -> Unit = {}, onMember: (Long) -> Unit) {
    val c = Cursus.colors
    Column {
        for (m in members) {
            Row(
                Modifier.fillMaxWidth().heightIn(min = 72.dp).clip(Radius.insetControl).clickable { onMember(m.id) }
                    .padding(horizontal = 16.dp, vertical = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Avatar(m.name, m.image, 48.dp)
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
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
                        ProfileText(m.shortDescr, style = Cursus.type.meta, color = c.textSecondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                }
                // The role as a quiet sign, not a control (Linux `role_icon`).
                Icon(painterResource(roleIcon(m.role)), m.role.replaceFirstChar { it.titlecase() }, Modifier.size(18.dp), tint = c.textSecondary)
                trailing(m)
            }
        }
    }
}

@Composable
private fun MemberInfoPane(model: AppModel, chat: Chat, m: Member, onOpenChat: (String) -> Unit) {
    val c = Cursus.colors
    var roles by remember { mutableStateOf(false) }
    // Same hierarchy as Linux / Mac: avatar beside name and role · status.
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
        modifier = Modifier.padding(horizontal = 4.dp)) {
        Avatar(m.name, m.image, 64.dp)
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(if (m.isMe) "YOU (${m.publicName.ifBlank { m.name }})" else m.name, style = Cursus.type.title,
                fontWeight = FontWeight.SemiBold, color = c.textPrimary)
            Text("${m.role.replaceFirstChar { it.titlecase() }} · ${m.status}", style = Cursus.type.meta, color = c.textSecondary)
        }
    }
    if (m.shortDescr.isNotBlank()) Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        SectionHeading("Bio")
        ProfileText(m.shortDescr, style = Cursus.type.body, color = c.textSecondary, modifier = Modifier.padding(horizontal = SectionInset))
    }
    if (m.isMe) return
    // You can act on members up to your own rank (core `can_act_on`), as on Linux.
    val outranked = chat.roleChoices.contains(m.role) || m.pending
    ButtonCard {
        NavRow(R.drawable.ic_chats, "Direct message") { model.messageMember(chat, m) { onOpenChat(it) } }
        if (chat.canModerate && !m.pending && (m.support != null || (chat.groupPrefs?.features?.get("support")?.on == true && m.role !in listOf("owner", "admin", "moderator")))) {
            NavRow(R.drawable.ic_flag_outline, if (m.role in listOf("owner", "admin", "moderator")) "Chat with admin" else "Chat with member") {
                onOpenChat("${chat.ref}(_support:${m.id})")
            }
        }
        if (chat.canManageMembers && outranked && !m.pending) {
            NavRow(roleIcon(m.role), "Role", value = m.role.replaceFirstChar { it.titlecase() }) { roles = true }
        }
        if (chat.canModerate && outranked) {
            ActionTile(R.drawable.ic_block_prohibited, if (m.blocked) "Unblock for all" else "Block for all", { model.blockMember(chat, m, !m.blocked) })
        }
    }
    if (m.pending && chat.canModerate) Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        SectionHeading("Waiting for review")
        Panel {
            ActionTile(R.drawable.ic_check, "Accept as member", { model.acceptMember(chat, m, "member") })
            ActionTile(R.drawable.ic_eye_observer, "Accept as observer", { model.acceptMember(chat, m, "observer") })
            ActionTile(R.drawable.ic_cancel, "Reject", { model.removeMember(chat, m, false) }, danger = true)
        }
    }
    if (chat.canManageMembers && outranked && !m.pending) Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        SectionHeading("Manage member")
        Panel {
            ActionTile(R.drawable.ic_remove_profile, "Remove member", { model.removeMember(chat, m, false) }, danger = true)
            ActionTile(R.drawable.ic_trash_delete, "Remove and delete messages", { model.removeMember(chat, m, true) }, danger = true)
        }
    }
    if (roles) {
        ChoiceMenu("Role", chat.roleChoices.map { it to it.replaceFirstChar { c -> c.titlecase() } }, m.role, { roles = false }) {
            model.setMemberRole(chat, m, it); roles = false
        }
    }
}

@Composable
private fun AddMemberPane(model: AppModel, chat: Chat) {
    val c = Cursus.colors
    var inviteRole by remember { mutableStateOf("member") }
    var menu by remember { mutableStateOf<String?>(null) }
    var search by remember { mutableStateOf("") }
    if (!chat.canManageMembers) {
        Text("Only admins can invite contacts.", style = Cursus.type.body, color = c.textSecondary)
        return
    }
    val inGroup = model.members.mapNotNull { it.contactId }.toSet()
    val contacts = model.chats.filter { it.isContact && !it.isRequest && it.apiId !in inGroup }
    val filtered = contacts.filter { it.title.contains(search, ignoreCase = true) }
    // Same order as Linux: note, who to invite as, search, the contacts.
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        SectionNote("They get the invitation in your direct chat.")
        Panel { NavRow(roleIcon(inviteRole), "Invite as", value = inviteRole.replaceFirstChar { it.titlecase() }) { menu = "invite" } }
        RoundedField(search, { search = it }, "Search contacts", modifier = Modifier.fillMaxWidth().padding(top = 4.dp))
        Panel {
            if (filtered.isEmpty()) Text(if (contacts.isEmpty()) "Everyone you know is already here." else "No matching contacts.",
                style = Cursus.type.meta, color = c.textSecondary, modifier = Modifier.padding(16.dp))
            for (contact in filtered) Row(
                Modifier.fillMaxWidth().heightIn(min = 64.dp).clip(Radius.insetControl).clickable { model.addMember(chat, contact, inviteRole) }
                    .padding(horizontal = 16.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Avatar(contact.title, contact.image, 40.dp)
                Text(contact.title, style = Cursus.type.body, color = c.textPrimary, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
                Icon(painterResource(R.drawable.ic_user_plus_alt_1), null, Modifier.size(24.dp), tint = c.textSecondary)
            }
        }
    }
    when (menu) {
        "invite" -> ChoiceMenu("Invite as", chat.assignableRoles.map { it to it.replaceFirstChar { c -> c.titlecase() } }, inviteRole, { menu = null }) { inviteRole = it; menu = null }
    }
}

private val GROUP_FEATURES = listOf(
    "timedMessages" to "Disappearing messages",
    "directMessages" to "Direct messages",
    "fullDelete" to "Delete for everyone",
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
    val sections = listOf(
        "Messages" to listOf("timedMessages", "directMessages", "fullDelete", "history"),
        "Media & interaction" to listOf("reactions", "voice", "files", "simplexLinks"),
        "Members" to listOf("support"),
    )
    for ((title, keys) in sections) {
        PreferenceGroup(title, if (title == "Messages") {
            if (owner) "Changes apply to everyone in this ${if (chat.isChannel) "channel" else "group"}."
            else "Only owners can change these preferences."
        } else null) {
            for (key in keys) {
                val label = GROUP_FEATURES.first { it.first == key }.second
                val on = prefs?.features?.get(key)?.on ?: (key == "support")
                SwitchRow(label, on, { value -> model.setGroupFeature(chat, key, value) },
                    note = groupFeatureSummary(key), icon = FEATURE_ICONS[key], enabled = owner)
                if (key == "timedMessages" && on) {
                    ChoiceRow("Delete after", ttlLabel(prefs?.timedTtl ?: 86400), R.drawable.ic_system_pending_line, enabled = owner) { menu = "ttl" }
                }
            }
            if (title == "Members" && owner && !chat.isChannel) {
                SwitchRow("Review new members", chat.memberReview, { model.setMemberReview(chat, it) },
                    note = "Approve requests before people join.", icon = R.drawable.ic_user_question_alt_1)
            }
        }
    }
    if (!chat.isChannel) PreferenceGroup("Just for you", "These choices only affect your account.") {
        ChoiceRow("Delivery receipts", receiptsLabel(chat.sendReceipts), R.drawable.ic_check_read) { menu = "receipts" }
    }
    when (menu) {
        "ttl" -> ChoiceMenu("Delete after", TTLS, prefs?.timedTtl, { menu = null }) { model.setGroupFeature(chat, "timedMessages", true, it); menu = null }
        "receipts" -> ChoiceMenu("Delivery receipts", listOf<Pair<Boolean?, String>>(null to "Default", true to "On", false to "Off"), chat.sendReceipts, { menu = null },
            note = "Let members know their messages arrived.") {
            model.setSendReceipts(chat, it); menu = null
        }
    }
}

private fun groupFeatureSummary(key: String): String = when (key) {
    "timedMessages" -> "Automatically remove messages after a set time."
    "directMessages" -> "Let members connect with each other."
    "fullDelete" -> "Allow senders to remove messages for everyone."
    "history" -> "Share recent messages with new members."
    "reactions" -> "Respond with a quick emoji."
    "voice" -> "Record and share audio messages."
    "files" -> "Share photos, videos and documents."
    "simplexLinks" -> "Share SimpleX invitations in messages."
    else -> "Give members a private way to reach admins."
}
