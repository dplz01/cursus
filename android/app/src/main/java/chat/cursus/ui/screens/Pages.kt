package chat.cursus.ui.screens

import androidx.compose.foundation.border
import chat.cursus.ui.components.PreferenceGroup
import chat.cursus.ui.components.PrefRow
import chat.cursus.ui.components.ChoiceChip
import chat.cursus.ui.components.ChoiceMenu
import chat.cursus.ui.components.ConfirmSheet
import chat.cursus.ui.components.InfoSwitchRow
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.draw.rotate
import androidx.compose.material3.Icon
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.LaunchedEffect
import chat.cursus.core.Invitation
import android.content.Intent
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import chat.cursus.AppModel
import chat.cursus.R
import chat.cursus.ThemeMode
import chat.cursus.core.Chat
import chat.cursus.core.ConnLink
import chat.cursus.ui.components.ActionTile
import chat.cursus.ui.components.Avatar
import chat.cursus.ui.components.CardTitle
import chat.cursus.ui.components.CursusCard
import chat.cursus.ui.components.CursusField
import chat.cursus.ui.components.IconTool
import chat.cursus.ui.components.LinkBox
import chat.cursus.ui.components.EditField
import chat.cursus.ui.components.EditableLine
import chat.cursus.ui.components.FieldEditor
import chat.cursus.ui.components.LineStyle
import chat.cursus.ui.components.ProfileIdentity
import chat.cursus.ui.components.PhotoActions
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import chat.cursus.ui.components.NavRow
import chat.cursus.ui.components.CursusOverlay
import chat.cursus.ui.components.pageRow
import chat.cursus.ui.components.DangerButton
import chat.cursus.ui.components.RoundedField
import chat.cursus.ui.components.PageColumn
import chat.cursus.ui.components.PrimaryButton
import chat.cursus.ui.components.SecondaryButton
import chat.cursus.ui.components.SectionLabel
import chat.cursus.ui.components.SwitchRow
import chat.cursus.ui.theme.Cursus
import chat.cursus.ui.theme.Radius
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import kotlinx.coroutines.launch

private enum class CreateKind { Group, Channel }

/** One invitation per page lifetime, including operations finishing after close. */
private class ConnectDraft {
    val invitation = mutableStateOf<Invitation?>(null)
    val copied = mutableStateOf(false)
    val busy = mutableStateOf(false)
    var active = true

    suspend fun create(model: AppModel, regenerate: Boolean = false) {
        if (busy.value) return
        busy.value = true
        try {
            withContext(NonCancellable) {
                val result = if (regenerate) model.regenerateInvitation(invitation.value?.connId) else model.newInvitation()
                if (!active) {
                    result?.let { model.deleteInvitation(it.connId, quiet = true) }
                } else if (result != null) {
                    invitation.value = result
                    copied.value = false
                }
            }
        } finally { busy.value = false }
    }
}

/** Linux "New" page: Connect card + Create card; New group / channel open the name overlay. */
@Composable
fun NewChatPage(model: AppModel, initialLink: String?, onClose: () -> Unit, onOpened: (String) -> Unit) {
    val c = Cursus.colors
    var link by rememberSaveable { mutableStateOf(initialLink ?: "") }
    var create by rememberSaveable { mutableStateOf<CreateKind?>(null) }
    val draft = remember { ConnectDraft() }
    val scope = rememberCoroutineScope()
    var groupName by rememberSaveable { mutableStateOf("") }
    var channelName by rememberSaveable { mutableStateOf("") }
    var creating by remember { mutableStateOf(false) }
    LaunchedEffect(draft) { draft.create(model) }
    androidx.compose.runtime.DisposableEffect(draft) {
        onDispose {
            draft.active = false
            draft.invitation.value?.takeUnless { draft.copied.value }?.let { model.deleteInvitation(it.connId, quiet = true) }
        }
    }

    Column(Modifier.fillMaxSize().background(c.bgPanel)) {
        PaneHeader("New conversation", onBack = onClose)
        Column(Modifier.verticalScroll(rememberScrollState())) {
            PageColumn {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    for ((label, icon, kind) in listOf(
                        Triple("Connect", R.drawable.ic_link_minimalistic_2, null),
                        Triple("New group", R.drawable.ic_user_group, CreateKind.Group),
                        Triple("New channel", R.drawable.ic_channels, CreateKind.Channel),
                    )) {
                        // Same tile as a profile's quick actions; the open one is outlined.
                        Column(Modifier.weight(1f).clip(Radius.control).background(if (create == kind) c.accentSoft else c.bgElevated)
                            .border(1.dp, if (create == kind) c.accent else c.border, Radius.control)
                            .clickable { create = kind }.padding(vertical = 12.dp, horizontal = 4.dp),
                            horizontalAlignment = Alignment.CenterHorizontally,
                            verticalArrangement = Arrangement.spacedBy(6.dp)) {
                            androidx.compose.material3.Icon(androidx.compose.ui.res.painterResource(icon), null, Modifier.size(24.dp), tint = c.accent)
                            Text(label, style = Cursus.type.meta, color = c.textSecondary, maxLines = 1,
                                overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis)
                        }
                    }
                }
                if (create == null) ConnectCard(model, link, { link = it }, onClose, draft, scope)
                else {
                    val name = if (create == CreateKind.Group) groupName else channelName
                    CursusCard {
                        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                            Text(if (create == CreateKind.Group) "New group" else "New channel", style = Cursus.type.title, color = c.textPrimary)
                            CursusField(name, { if (create == CreateKind.Group) groupName = it else channelName = it }, if (create == CreateKind.Group) "Group name" else "Channel name")
                            PrimaryButton(if (creating) "Creating…" else if (create == CreateKind.Group) "Create group" else "Create channel", {
                                if (creating) return@PrimaryButton
                                creating = true
                                val channel = create == CreateKind.Channel
                                model.createGroup(name.trim(), channel = channel, finished = { creating = false }) {
                                    model.section = if (channel) chat.cursus.Section.Channels else chat.cursus.Section.Chats
                                    onOpened(it)
                                }
                            }, enabled = name.isNotBlank() && !creating)
                        }
                    }
                }
            }
        }
    }

}

/**
 * Linux Connect card: title + ⓘ sheet, a one-time link created as the page
 * opens with its incognito switch and "Replace link" (asks first: the shown link
 * is deleted), then joining with a pasted link. Leaving the page deletes the
 * link if it was never copied — nobody can have it; copied ones stay listed
 * under the gold button beside "+". The reusable address is in Settings.
 */
@Composable
private fun ConnectCard(model: AppModel, link: String, onLink: (String) -> Unit, onClose: () -> Unit,
                        draft: ConnectDraft, scope: CoroutineScope) {
    val c = Cursus.colors
    val clipboard = LocalClipboardManager.current
    var invitation by draft.invitation
    var copied by draft.copied
    var busy by draft.busy
    var info by remember { mutableStateOf(false) }
    var confirmNew by remember { mutableStateOf(false) }
    var connecting by remember { mutableStateOf(false) }
    val incognito = model.prefs.incognito
    CursusCard {
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.weight(1f)) { CardTitle(R.drawable.ic_link_minimalistic_2, "Connect") }
                IconTool(R.drawable.ic_info_circle, "What do these do?", { info = true }, tint = c.textMuted)
            }
            SectionLabel("Copy one-time link")
            Text("Share privately with one person. The link expires after they connect.",
                style = Cursus.type.meta, color = c.textSecondary, modifier = Modifier.padding(horizontal = 12.dp))
            Row(Modifier.padding(horizontal = 12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Box(Modifier.weight(1f)) {
                    val inv = invitation
                    if (inv != null) {
                        LinkBox(inv.link.display) { copied = true; model.notice = "One-time link copied" }
                    } else {
                        Text(
                            if (busy) "Creating link…" else "No link yet",
                            style = Cursus.type.meta, color = c.textMuted,
                            modifier = Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
                        )
                    }
                }
            }
            SwitchRow("Use a random profile", incognito, { on ->
                model.prefs.incognito = on
                val inv = invitation
                if (inv != null && !busy) {
                    busy = true
                    scope.launch {
                        val state = model.setInvitationIncognito(inv.connId, on)
                        if (state != null) invitation = inv.copy(incognito = state)
                        else invitation = null // used or deleted meanwhile
                        busy = false
                    }
                }
            }, note = "Use a random name instead of your profile.", enabled = !busy)
            // Same look as the Connect button below (full width on phones).
            Column(Modifier.padding(horizontal = 8.dp, vertical = 4.dp)) {
                SecondaryButton("Replace link", {
                    if (invitation != null) {
                        confirmNew = true
                    } else {
                        scope.launch { draft.create(model) }
                    }
                }, Modifier.fillMaxWidth(), enabled = !busy)
            }
            SectionLabel("Join with a link")
            Text("Paste a contact, group or channel link to connect.",
                style = Cursus.type.meta, color = c.textSecondary, modifier = Modifier.padding(horizontal = 12.dp))
            Column(Modifier.padding(horizontal = 12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    RoundedField(link, onLink, "Invite or address link…", Modifier.weight(1f))
                    IconTool(R.drawable.ic_paste, "Paste", { clipboard.getText()?.let { onLink(it.text.trim()) } })
                }
                PrimaryButton(if (connecting) "Connecting…" else "Connect", {
                    if (!connecting) {
                        connecting = true
                        model.connect(link.trim(), incognito, finished = { connecting = false }) { onClose() }
                    }
                }, Modifier.fillMaxWidth(), enabled = link.isNotBlank() && !connecting)
            }
        }
    }
    if (confirmNew) {
        CursusOverlay({ confirmNew = false }) {
            Text("Create a new link?", style = Cursus.type.body, color = c.textPrimary, fontWeight = FontWeight.SemiBold)
            Text(
                "The previous link will be deleted. Anyone you already sent it to won't be able to connect with it.",
                style = Cursus.type.meta, color = c.textSecondary,
            )
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End)) {
                SecondaryButton("Cancel", { confirmNew = false })
                DangerButton("Delete and create new", {
                    confirmNew = false
                    scope.launch { draft.create(model, regenerate = true) }
                })
            }
        }
    }
    if (info) {
        CursusOverlay({ info = false }) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Icon(painterResource(R.drawable.ic_info_circle), null, Modifier.size(20.dp), tint = c.accent)
                Text("Connect", style = Cursus.type.body, color = c.textPrimary, fontWeight = FontWeight.SemiBold)
            }
            for (para in ConnectInfoText) {
                Text(para, style = Cursus.type.meta, color = c.textSecondary)
            }
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                PrimaryButton("Got it", { info = false })
            }
        }
    }
}

/** Same text as the Linux ⓘ sheet (`SettingInfo::Connect`). */
private val ConnectInfoText = listOf(
    "One-time link: a new one is made each time you open this page. Send it to one person \u2014 whoever opens it first becomes your contact, then it stops working. \u201cReplace link\u201d deletes the shown link and makes a fresh one.",
    "Links you copied that nobody has used yet stay valid: the gold link button next to + lists them, to copy, name or delete. A link you never copied is deleted when you leave this page \u2014 nobody can have it.",
    "Incognito: when on, the person who connects sees a new random name instead of your profile \u2014 a different one for each contact. You can switch it until the link is used. It also applies when you connect with a link below.",
    "Join with a link: paste a link someone sent you \u2014 a one-time link, an address, or a group or channel link \u2014 and tap Connect.",
    "Your reusable address, which many people can use, is in Settings \u2192 Address.",
)

/** Linux `create_chat_overlay`. */
/** The settings pages, in list order (the profile is the list's header):
 *  the same grouping and wording as the Linux app. */
enum class SettingsPage(val icon: Int, val title: String) {
    Profile(R.drawable.ic_user_alt_1, "Profile"),
    Address(R.drawable.ic_address_card, "Address"),
    Privacy(R.drawable.ic_shield_check, "Privacy & Security"),
    Notifications(R.drawable.ic_notifications_outline, "Notifications"),
    Appearance(R.drawable.ic_image_square, "Appearance"),
    About(R.drawable.ic_info_circle, "About"),
}

@Composable
fun SettingsListPane(model: AppModel, selected: SettingsPage?, onOpen: (SettingsPage) -> Unit) {
    val c = Cursus.colors
    val user = model.user
    Column(Modifier.fillMaxSize()) {
        PaneHeader("Settings")
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(bottom = LocalNavClearance.current)) {
            PageColumn {
                Column {
                    Row(
                        Modifier.pageRow { onOpen(SettingsPage.Profile) },
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(14.dp),
                    ) {
                        chat.cursus.ui.components.Avatar(user?.displayName ?: "", user?.image, 56.dp)
                        Column(Modifier.weight(1f)) {
                            Text(user?.displayName ?: "", style = Cursus.type.title, color = c.textPrimary, maxLines = 1)
                            chat.cursus.ui.components.ProfileText(
                                user?.shortDescr?.ifBlank { null } ?: "Add a bio",
                                style = Cursus.type.meta, color = c.textSecondary, maxLines = 1,
                            )
                        }
                    }
                    Box(Modifier.size(12.dp))
                    // Every category in one list, no headings (same on Linux / macOS).
                    val pages = listOf(
                        SettingsPage.Profile, SettingsPage.Address, SettingsPage.Privacy,
                        SettingsPage.Notifications, SettingsPage.Appearance, SettingsPage.About,
                    )
                    PreferenceGroup {
                        for (p in pages) Row(
                            Modifier.clip(chat.cursus.ui.theme.Radius.insetControl)
                                // Beside the page only (two panes), like the chat list.
                                .background(if (selected == p && !chat.cursus.ui.components.LocalPhonePages.current) c.accentSoft else androidx.compose.ui.graphics.Color.Transparent)
                                .pageRow { onOpen(p) },
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(14.dp),
                        ) {
                            chat.cursus.ui.components.IconDisc(p.icon)
                            Text(p.title, style = Cursus.type.body, color = c.textPrimary, modifier = Modifier.weight(1f).padding(vertical = 2.dp))
                            Icon(painterResource(R.drawable.ic_chevron_down), null, Modifier.size(16.dp).rotate(-90f), tint = c.textMuted)
                        }
                    }
                }
                if (model.busy) CircularProgressIndicator(color = c.accent)
            }
        }
    }
}

@Composable
fun SettingsDetailPane(model: AppModel, page: SettingsPage, onBack: () -> Unit) {
    val c = Cursus.colors
    val uri = LocalUriHandler.current
    val user = model.user
    var editing by remember { mutableStateOf<EditField?>(null) }
    var showLicences by remember { mutableStateOf(false) }
    PageScaffold(onBack) {
        Text(page.title, style = Cursus.type.title, color = c.textPrimary)
        Column(verticalArrangement = Arrangement.spacedBy(20.dp)) {
            when (page) {
                SettingsPage.Profile -> {
                    val pickPhoto = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri ->
                        uri?.let { model.withProfileImage(it) { url -> model.setOwnImage(url) } }
                    }
                    val photo = PhotoActions(
                        hasImage = user?.image != null,
                        onPick = { pickPhoto.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) },
                        onClear = { model.setOwnImage("") },
                    )
                    ProfileIdentity(user?.displayName ?: "", user?.image, photo = photo) {
                        EditableLine(user?.displayName ?: "", "Your name", LineStyle.Name) { editing = EditField.OwnName }
                        EditableLine(user?.shortDescr ?: "", "Add a bio", LineStyle.Sub) { editing = EditField.OwnBio }
                    }
                    Box(Modifier.size(18.dp))
                    CardTitle(0, "Description")
                    EditableLine(user?.description ?: "", "Tell people about yourself", LineStyle.Body) { editing = EditField.OwnDescription }
                }
                SettingsPage.Address -> AddressCard(model)
                SettingsPage.Appearance -> {
                    var themes by remember { mutableStateOf(false) }
                    var sizes by remember { mutableStateOf(false) }
                    var clocks by remember { mutableStateOf(false) }
                    PreferenceGroup("Display") {
                        chat.cursus.ui.components.PrefRow("Theme") {
                            chat.cursus.ui.components.ChoiceChip(model.prefs.theme.name, { themes = true })
                        }
                        chat.cursus.ui.components.PrefRow("Text size") {
                            chat.cursus.ui.components.ChoiceChip(model.prefs.textSize.label, { sizes = true })
                        }
                        chat.cursus.ui.components.PrefRow("Time format") {
                            chat.cursus.ui.components.ChoiceChip(model.prefs.timeFormat.label, { clocks = true })
                        }
                    }
                    if (themes) {
                        chat.cursus.ui.components.ChoiceMenu(
                            "Theme", ThemeMode.entries.map { it to it.name }, model.prefs.theme, { themes = false },
                        ) { model.prefs.theme = it; themes = false }
                    }
                    if (sizes) {
                        chat.cursus.ui.components.ChoiceMenu(
                            "Text size", chat.cursus.TextSize.entries.map { it to it.label }, model.prefs.textSize, { sizes = false },
                        ) { model.prefs.textSize = it; sizes = false }
                    }
                    if (clocks) {
                        chat.cursus.ui.components.ChoiceMenu(
                            "Time format", chat.cursus.TimeFormat.entries.map { it to it.label }, model.prefs.timeFormat, { clocks = false },
                        ) { model.prefs.timeFormat = it; clocks = false }
                    }
                }
                SettingsPage.About -> {
                    Column(Modifier.padding(horizontal = 12.dp, vertical = 6.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text("Cursus ${model.info?.version ?: ""} for Android", style = Cursus.type.body, color = c.textPrimary)
                        Text(
                            "Free software under AGPL-3.0-or-later, built on the official SimpleX Chat core.",
                            style = Cursus.type.meta, color = c.textSecondary,
                        )
                    }
                    NavRow(R.drawable.ic_link_minimalistic_2, "Source code", "github.com/dplz01/cursus") {
                        uri.openUri("https://github.com/dplz01/cursus")
                    }
                    NavRow(R.drawable.ic_scripture_scroll, "Licences and credits") { showLicences = true }
                }
                else -> PrivacyRows(model, page)
            }
        }
    }
    if (showLicences) LicencesOverlay { showLicences = false }
    editing?.let { f ->
        val initial = when (f) {
            EditField.OwnName -> user?.displayName
            EditField.OwnBio -> user?.shortDescr
            else -> user?.description
        } ?: ""
        FieldEditor(f, initial, { editing = null }) { v ->
            when (f) {
                EditField.OwnName -> model.updateOwnProfile(displayName = v)
                EditField.OwnBio -> model.updateOwnProfile(shortDescr = v)
                else -> model.updateOwnProfile(description = v)
            }
            editing = null
        }
    }
}

/** Linux Settings → Address (official apps): "Create address" when there is
 *  none; hidden until "Show address"; share on profile / auto-accept;
 *  "Delete address" asks first. */
@Composable
private fun AddressCard(model: AppModel) {
    val c = Cursus.colors
    val scope = rememberCoroutineScope()
    var loaded by remember { mutableStateOf(false) }
    var address by remember { mutableStateOf<ConnLink?>(null) }
    var revealed by remember { mutableStateOf(false) }
    var autoAccept by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        address = model.currentAddress()
        if (address != null) autoAccept = model.addressAutoAccept()
        loaded = true
    }
    PreferenceGroup("Your address") {
        val a = address
        when {
            !loaded -> Text("Loading…", style = Cursus.type.meta, color = c.textMuted, modifier = Modifier.padding(horizontal = 12.dp, vertical = 6.dp))
            a == null -> NavRow(R.drawable.ic_address_card, "Create address") {
                scope.launch { address = model.addressLink(); revealed = address != null }
            }
            else -> {
                if (revealed) {
                    Row(Modifier.padding(horizontal = 12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        Box(Modifier.weight(1f)) { LinkBox(a.short ?: a.full) { model.notice = "Address copied" } }
                        IconTool(R.drawable.ic_eye_observer, "Hide address", { revealed = false }, tint = c.accent)
                    }
                } else {
                    NavRow(R.drawable.ic_eye_observer, "Show address") { revealed = true }
                }
                SwitchRow("Share address on profile", !model.user?.contactLink.isNullOrEmpty(), { model.setProfileAddress(it) })
                SwitchRow("Auto-accept address requests", autoAccept, { on ->
                    autoAccept = on
                    scope.launch { autoAccept = model.setAddressAutoAccept(on) }
                })
                ActionTile(R.drawable.ic_trash_delete, "Delete address", { confirmDelete = true }, danger = true)
            }
        }
    }
    if (confirmDelete) {
        CursusOverlay({ confirmDelete = false }) {
            Text("Delete address?", style = Cursus.type.body, color = c.textPrimary, fontWeight = FontWeight.SemiBold)
            Text(
                "All your contacts will remain connected. Nobody new can use the address, and your profile is updated for your contacts.",
                style = Cursus.type.meta, color = c.textSecondary,
            )
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End)) {
                SecondaryButton("Cancel", { confirmDelete = false })
                DangerButton("Delete address", {
                    confirmDelete = false
                    scope.launch {
                        if (model.deleteAddress()) {
                            address = null
                            revealed = false
                            autoAccept = false
                        }
                    }
                })
            }
        }
    }
}

/** A change that shows or shares more, waiting for the user's yes. */
private data class PrivacyAsk(val title: String, val body: String, val action: String, val apply: () -> Unit)

/** Linux Settings → Privacy & security: same sections, order, wording and
 *  confirmations; explanations behind ⓘ (phone wording). */
/** The rows of one settings page that are privacy switches (each explains
 *  itself behind ⓘ and asks before lowering your privacy). */
@Composable
private fun PrivacyRows(model: AppModel, page: SettingsPage) {
    val p = model.prefs
    var ask by remember { mutableStateOf<PrivacyAsk?>(null) }
    var lockMenu by remember { mutableStateOf(false) }
    var remembered by remember { mutableStateOf(model.rememberedPassword) }
    val user = model.user
    when (page) {
        SettingsPage.Privacy -> {
            PreferenceGroup("On screen") {
            InfoSwitchRow("Message previews in chat list", p.chatPreviews, { on ->
            if (!on) p.chatPreviews = false
            else ask = PrivacyAsk("Show message previews?",
                "The last message of every chat will be visible in the chat list to anyone who can see your screen.",
                "Show previews") { p.chatPreviews = true }
        }, InfoText.ChatPreviews)
            InfoSwitchRow("Blur photos and videos", p.blurMedia, { on ->
            if (on) p.blurMedia = true
            else ask = PrivacyAsk("Stop blurring media?",
                "Photos and videos will be shown in full in your chats, visible to anyone who can see your screen.",
                "Show media") { p.blurMedia = false }
        }, InfoText.BlurMedia)
            InfoSwitchRow("Block screenshots", p.secureScreen, { on ->
            if (on) p.secureScreen = true
            else ask = PrivacyAsk("Allow screenshots?",
                "Screenshots and screen recordings will show your chats, and so will the recent-apps preview.",
                "Allow") { p.secureScreen = false }
        }, InfoText.SecureScreen)
            }
            PreferenceGroup("What contacts see") {
            InfoSwitchRow("Delivery receipts to contacts", user?.sendRcptsContacts ?: true, { on ->
            if (!on) model.setDeliveryReceipts(groups = false, on = false)
            else ask = PrivacyAsk("Send delivery receipts?",
                "Your contacts will see when their messages reach this phone.",
                "Send receipts") { model.setDeliveryReceipts(groups = false, on = true) }
        }, InfoText.ReceiptsContacts)
            InfoSwitchRow("Delivery receipts in small groups", user?.sendRcptsSmallGroups ?: true, { on ->
            if (!on) model.setDeliveryReceipts(groups = true, on = false)
            else ask = PrivacyAsk("Send delivery receipts?",
                "Members of small groups will see when their messages reach this phone.",
                "Send receipts") { model.setDeliveryReceipts(groups = true, on = true) }
        }, InfoText.ReceiptsGroups)
            }
            PreferenceGroup("Links") {
            InfoSwitchRow("Remove tracking from links", p.cleanLinks, { on ->
            if (on) p.cleanLinks = true
            else ask = PrivacyAsk("Keep tracking codes in links?",
                "Links you send and open will keep codes like utm_source or fbclid, which let websites track who shared them and where you came from.",
                "Keep codes") { p.cleanLinks = false }
        }, InfoText.CleanLinks)
            }

            PreferenceGroup("App access") {
            InfoSwitchRow("App lock", p.appLock, { on ->
            if (on) p.appLock = true
            else ask = PrivacyAsk("Turn off app lock?",
                "Anyone who has your unlocked phone while Cursus is running can read your chats.",
                "Turn off") { p.appLock = false }
        }, InfoText.AppLock)
            if (p.appLock) {
                PrefRow("Lock") { ChoiceChip(lockAfterLabel(p.lockAfter), { lockMenu = true }) }
            }
            ActionTile(R.drawable.ic_lock, "Lock now", { model.lockApp() })
            }
            if (remembered) {
                PreferenceGroup("Starting") {
                ActionTile(R.drawable.ic_lock, "Forget saved password", {
                    model.forgetPassword(); remembered = false
                    model.notice = "Cursus will ask for your password next time"
                })
            }
            }

            PreferenceGroup("Downloads") {
            InfoSwitchRow("Download images and voice automatically", p.autoDownload, { on ->
            if (!on) p.autoDownload = false
            else ask = PrivacyAsk("Download media automatically?",
                "Images, voice messages and short videos from anyone who writes to you will download without asking \u2014 including from strangers in groups.",
                "Download automatically") { p.autoDownload = true }
        }, InfoText.AutoDownload)
            InfoSwitchRow("Protect IP address", p.protectIp, { on ->
            if (on) p.protectIp = true
            else ask = PrivacyAsk("Stop protecting your IP address?",
                "Files will download from any server the sender chooses, including automatic downloads. Without Tor or VPN, those servers see your IP address.",
                "Turn off") { p.protectIp = false }
        }, InfoText.ProtectIp)
            }
            PreferenceGroup("On this phone") {
            InfoSwitchRow("Encrypt stored files", model.info?.encryptLocalFiles ?: true, { on ->
            if (on) model.setEncryptLocalFiles(true)
            else ask = PrivacyAsk("Store new files unencrypted?",
                "Photos, videos, voice messages and files you send or receive from now on will be saved readable on this phone. Files already stored stay encrypted.",
                "Store unencrypted") { model.setEncryptLocalFiles(false) }
        }, InfoText.EncryptFiles)
            }
        }
        SettingsPage.Notifications -> {
            PreferenceGroup("Alerts") {
            InfoSwitchRow("Hide message text in notifications", p.hideNotificationText, { on ->
            if (on) p.hideNotificationText = true
            else ask = PrivacyAsk("Show message text in notifications?",
                "Anyone who can see your screen will be able to read incoming messages, also on the lock screen, and Android may keep them in its notification history.",
                "Show text") { p.hideNotificationText = false }
        }, InfoText.HideNotificationText)
            }
            PreferenceGroup("Background") {
            SwitchRow("Receive in background", p.background, {
                p.background = it
                model.syncBackground()
            })
            }
        }
        else -> Unit
    }
    if (lockMenu) {
        ChoiceMenu("Lock", LockAfterChoices, p.lockAfter, { lockMenu = false }) { p.lockAfter = it; lockMenu = false }
    }
    ask?.let { a -> ConfirmSheet(a.title, a.body, a.action, a.apply) { ask = null } }
}

/** Linux `LockAfter`, in seconds (0 = as soon as Cursus is left). */
private val LockAfterChoices = listOf(
    0 to "Immediately",
    30 to "After 30 seconds",
    60 to "After 1 minute",
    300 to "After 5 minutes",
    900 to "After 15 minutes",
)

private fun lockAfterLabel(secs: Int) = LockAfterChoices.firstOrNull { it.first == secs }?.second ?: "After $secs seconds"

/** Linux `SettingInfo` explanations, worded for a phone. */
private object InfoText {
    val AppLock = "App lock" to "Cursus asks for your database password again after you leave it alone or switch to another app for a while.\n\nWhile locked, nothing of your chats is shown, decrypted photos and voice messages are deleted from the cache, and notifications show only \u201cNew message\u201d. Messages keep arriving."
    val ChatPreviews = "Message previews in chat list" to "Shows the last message under each chat in the list. Turn it off so people who can see your screen only see chat names."
    val BlurMedia = "Blur photos and videos" to "Photos in chats are blurred until you tap them. A tapped photo stays visible until Cursus locks or restarts."
    val HideNotificationText = "Hide message text in notifications" to "Notifications show the chat name and \u201cNew message\u201d only.\n\nWith it off, the message text appears in the notification \u2014 also on the lock screen \u2014 and Android may keep it in its notification history."
    val SecureScreen = "Hide chats from screenshots" to "Android blocks screenshots and screen recordings of Cursus, and shows a blank card for it in recent apps."
    val ProtectIp = "Protect IP address" to "Files are stored on file servers. Your own servers are trusted; servers chosen by the person who sent the file are not.\n\nWhen on, Cursus asks before downloading from a server you don't use, and automatic downloads skip them. Without Tor or a VPN, any server you download from can see your IP address."
    val AutoDownload = "Download images and voice automatically" to "Images, voice messages and short videos download as soon as you open their chat, so they are ready to view.\n\nWhen off, nothing downloads until you tap it \u2014 useful to save data or to avoid files from strangers in groups."
    val EncryptFiles = "Encrypt stored files" to "Photos, videos, voice messages and files are saved encrypted on this phone; Cursus decrypts them only while you view them.\n\nTurning it off affects new files only \u2014 files already stored stay encrypted."
    val CleanLinks = "Remove tracking from links" to "Cursus removes tracking codes such as utm_source or fbclid from links in the messages you send \u2014 so they never reach your contacts \u2014 and from links before opening them. They tell websites who shared the link and where you came from. The page itself is the same."
    val ReceiptsContacts = "Delivery receipts to contacts" to "Your contacts see a second tick when their messages reach this phone. It does not tell them you read them.\n\nYou can choose differently for a single contact in its chat preferences."
    val ReceiptsGroups = "Delivery receipts in small groups" to "Members see when their messages reach this phone. SimpleX only sends receipts in groups with up to 20 members.\n\nYou can choose differently for a single group in its preferences."
}

/** What each bundled notice file covers (licenses/NOTICE.md has the table). */
private val LicenceFiles = listOf(
    "NOTICE.md" to "Overview: every component, its licence and source",
    "LICENSE" to "Cursus — GNU Affero General Public License v3",
    "ANDROID-LIBRARIES.txt" to "AndroidX, Jetpack Compose, Kotlin (Apache-2.0)",
    "THIRD_PARTY_LICENSES.txt" to "Rust crates in the Cursus core",
    "SIMPLEX-CORE-LICENSES.txt" to "Libraries inside the SimpleX Chat core",
    "ICONS-ATTRIBUTION.md" to "Icon authors and licences",
    "MIT-Ionicons.txt" to "Ionicons by Ben Sperry / Ionic (MIT)",
    "MIT-Line-MD.txt" to "Line MD by Vjacheslav Trushkin (MIT)",
)

/**
 * Licence texts bundled in the APK (`assets/licenses`, copied from the repo's
 * `LICENSE` and `licenses/` by the build): a list of the notice files, each
 * opening in a reader. The big ones (Rust crates, SimpleX core) run to a few
 * MB, so the reader renders lines lazily and loads off the main thread.
 */
@Composable
private fun LicencesOverlay(onDismiss: () -> Unit) {
    val c = Cursus.colors
    val context = LocalContext.current
    val files = remember {
        val present = context.assets.list("licenses")?.toSet().orEmpty()
        val known = LicenceFiles.filter { it.first in present }
        known + (present - known.map { it.first }.toSet()).sorted().map { it to "" }
    }
    var open by remember { mutableStateOf<String?>(null) }
    androidx.compose.ui.window.Dialog(
        onDismissRequest = onDismiss,
        properties = androidx.compose.ui.window.DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
    ) {
        val name = open
        val back = { if (name != null) open = null else onDismiss() }
        androidx.activity.compose.BackHandler(onBack = back)
        chat.cursus.ui.components.DialogEscape(back)
        Column(
            Modifier.fillMaxSize().background(c.bgPanel)
                .windowInsetsPadding(androidx.compose.foundation.layout.WindowInsets.safeDrawing),
        ) {
            if (name == null) {
                PaneHeader("Licences and credits", onBack = onDismiss)
                Column(Modifier.verticalScroll(rememberScrollState())) {
                    PageColumn {
                        Text(
                            "Cursus is free software under the GNU AGPL v3. It is built on the SimpleX Chat core " +
                                "and on the open-source libraries, icons and fonts credited here.",
                            style = Cursus.type.meta, color = c.textSecondary,
                            modifier = Modifier.padding(horizontal = 12.dp),
                        )
                        CursusCard {
                            for ((file, about) in files) {
                                NavRow(R.drawable.ic_review_file, file, about.ifEmpty { null }) { open = file }
                            }
                        }
                    }
                }
            } else {
                PaneHeader(name, onBack = { open = null })
                val lines by chat.cursus.ui.components.rememberLoaded<List<String>?>(null, name) {
                    kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) {
                        runCatching { context.assets.open("licenses/$name").bufferedReader().use { it.readLines() } }
                            .getOrDefault(listOf("Could not read $name."))
                    }
                }
                val mono = Cursus.type.meta.copy(fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace, fontSize = 11.sp)
                androidx.compose.foundation.lazy.LazyColumn(
                    Modifier.fillMaxSize(),
                    contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = 12.dp, vertical = 8.dp),
                ) {
                    val l = lines.orEmpty()
                    items(l.size) { i -> Text(l[i].ifEmpty { " " }, style = mono, color = c.textPrimary) }
                }
            }
        }
    }
}
