package chat.cursus.ui.screens

import androidx.compose.ui.res.painterResource
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
import chat.cursus.ui.components.DangerButton
import chat.cursus.ui.components.RoundedField
import chat.cursus.ui.components.PageColumn
import chat.cursus.ui.components.PrimaryButton
import chat.cursus.ui.components.SecondaryButton
import chat.cursus.ui.components.SectionLabel
import chat.cursus.ui.components.SwitchRow
import chat.cursus.ui.theme.Cursus
import chat.cursus.ui.theme.Radius
import kotlinx.coroutines.launch

private fun share(context: android.content.Context, text: String) {
    val send = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, text)
    context.startActivity(Intent.createChooser(send, null))
}

private enum class CreateKind { Group, Channel }

/** Linux "New" page: Connect card + Create card; New group / channel open the name overlay. */
@Composable
fun NewChatPage(model: AppModel, initialLink: String?, onClose: () -> Unit, onOpened: (String) -> Unit) {
    val c = Cursus.colors
    val clipboard = LocalClipboardManager.current
    val scope = rememberCoroutineScope()
    var link by rememberSaveable { mutableStateOf(initialLink ?: "") }
    var create by remember { mutableStateOf<CreateKind?>(null) }

    Column(Modifier.fillMaxSize().background(c.bgPanel)) {
        PaneHeader("New", onBack = onClose)
        Column(Modifier.verticalScroll(rememberScrollState())) {
            PageColumn {
                ConnectCard(model, link, { link = it }, onClose)
                CursusCard {
                    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        CardTitle(R.drawable.ic_plus, "Create", "Start a conversation for many people.")
                        NavRow(R.drawable.ic_user_group, "New group", "Private group — invite contacts after creating") {
                            create = CreateKind.Group
                        }
                        NavRow(R.drawable.ic_channels, "New channel", "Public channel using enabled chat relays") {
                            create = CreateKind.Channel
                        }
                    }
                }
            }
        }
    }
    create?.let { kind ->
        CreateChatOverlay(kind, onDismiss = { create = null }) { name ->
            create = null
            model.createGroup(name, channel = kind == CreateKind.Channel) { onOpened(it) }
        }
    }
}

/**
 * Linux Connect card: title + ⓘ sheet, a one-time link created as the page
 * opens with its incognito switch and "New link" (asks first: the shown link
 * is deleted), then joining with a pasted link. Leaving the page deletes the
 * link if it was never copied — nobody can have it; copied ones stay listed
 * under the gold button beside "+". The reusable address is in Settings.
 */
@Composable
private fun ConnectCard(model: AppModel, link: String, onLink: (String) -> Unit, onClose: () -> Unit) {
    val c = Cursus.colors
    val clipboard = LocalClipboardManager.current
    val scope = rememberCoroutineScope()
    var invitation by remember { mutableStateOf<Invitation?>(null) }
    var copied by remember { mutableStateOf(false) }
    var busy by remember { mutableStateOf(true) }
    var info by remember { mutableStateOf(false) }
    var confirmNew by remember { mutableStateOf(false) }
    val incognito = model.prefs.incognito
    LaunchedEffect(Unit) {
        invitation = model.newInvitation()
        busy = false
    }
    // Never copied: nobody can have it, so don't leave it behind.
    val latest by androidx.compose.runtime.rememberUpdatedState(invitation to copied)
    androidx.compose.runtime.DisposableEffect(Unit) {
        onDispose {
            val (inv, wasCopied) = latest
            if (inv != null && !wasCopied) model.deleteInvitation(inv.connId, quiet = true)
        }
    }
    CursusCard {
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Row(Modifier.padding(4.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Box(Modifier.size(36.dp).clip(Radius.pill).background(c.accentSoft), contentAlignment = Alignment.Center) {
                    Icon(painterResource(R.drawable.ic_link_minimalistic_2), null, Modifier.size(18.dp), tint = c.accent)
                }
                Text("Connect", style = Cursus.type.body, color = c.textPrimary, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
                IconTool(R.drawable.ic_info_circle, "What do these do?", { info = true }, tint = c.textMuted)
            }
            SectionLabel("Copy one-time link")
            Row(Modifier.padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
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
                IconTool(
                    if (incognito) R.drawable.ic_incognito_filled else R.drawable.ic_incognito,
                    if (incognito) "Incognito on" else "Incognito off",
                    {
                        val on = !incognito
                        model.prefs.incognito = on
                        val inv = invitation
                        if (inv != null && !busy) {
                            busy = true
                            scope.launch {
                                val state = model.setInvitationIncognito(inv.connId, on)
                                if (state != null) {
                                    invitation = inv.copy(incognito = state)
                                } else {
                                    invitation = null // used or deleted meanwhile
                                }
                                busy = false
                            }
                        }
                    },
                    tint = if (incognito) c.accent else c.textSecondary,
                )
            }
            Row(Modifier.padding(horizontal = 8.dp, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    if (incognito) "Incognito: they'll see a random profile." else "They'll see your profile.",
                    style = Cursus.type.meta, color = c.textSecondary, modifier = Modifier.weight(1f),
                )
                SecondaryButton("New link", {
                    if (invitation != null) {
                        confirmNew = true
                    } else {
                        busy = true
                        scope.launch { invitation = model.newInvitation(); copied = false; busy = false }
                    }
                }, enabled = !busy)
            }
            SectionLabel("Join with a link")
            Column(Modifier.padding(horizontal = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    RoundedField(link, onLink, "Invite or address link…", Modifier.weight(1f))
                    IconTool(R.drawable.ic_paste, "Paste", { clipboard.getText()?.let { onLink(it.text.trim()) } })
                }
                PrimaryButton("Connect", { model.connect(link.trim(), incognito) { onClose() } }, Modifier.fillMaxWidth(), enabled = link.isNotBlank())
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
                    busy = true
                    scope.launch {
                        model.regenerateInvitation(invitation?.connId)?.let { invitation = it; copied = false }
                        busy = false
                    }
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
    "One-time link: a new one is made each time you open this page. Send it to one person \u2014 whoever opens it first becomes your contact, then it stops working. \u201cNew link\u201d deletes the shown link and makes a fresh one.",
    "Links you copied that nobody has used yet stay valid: the gold link button next to + lists them, to copy, name or delete. A link you never copied is deleted when you leave this page \u2014 nobody can have it.",
    "Incognito: when on, the person who connects sees a new random name instead of your profile \u2014 a different one for each contact. You can switch it until the link is used. It also applies when you connect with a link below.",
    "Join with a link: paste a link someone sent you \u2014 a one-time link, an address, or a group or channel link \u2014 and tap Connect.",
    "Your reusable address, which many people can use, is in Settings \u2192 Address.",
)

/** Linux `create_chat_overlay`. */
@Composable
private fun CreateChatOverlay(kind: CreateKind, onDismiss: () -> Unit, onCreate: (String) -> Unit) {
    val c = Cursus.colors
    var name by rememberSaveable { mutableStateOf("") }
    val (title, hint, placeholder) = when (kind) {
        CreateKind.Group -> Triple("New group", "Private group — invite contacts after creating.", "Group name")
        CreateKind.Channel -> Triple(
            "New channel", "Public channel using your enabled chat relays (from SimpleX network settings).", "Channel name",
        )
    }
    CursusOverlay(onDismiss) {
        Text(title, style = Cursus.type.body, color = c.textPrimary)
        Text(hint, style = Cursus.type.meta, color = c.textSecondary)
        CursusField(name, { name = it }, placeholder)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            SecondaryButton("Cancel", onDismiss)
            PrimaryButton("Create", { onCreate(name.trim()) }, enabled = name.isNotBlank())
        }
    }
}

@Composable
fun SettingsPane(model: AppModel) {
    val c = Cursus.colors
    val scope = rememberCoroutineScope()
    val uri = LocalUriHandler.current
    val user = model.user
    var editing by remember { mutableStateOf<EditField?>(null) }
    var showLicences by remember { mutableStateOf(false) }
    var address by remember { mutableStateOf<ConnLink?>(null) }
    var loadingAddress by remember { mutableStateOf(false) }
    val context = LocalContext.current

    Column(Modifier.fillMaxSize()) {
        PaneHeader("Settings")
        Column(Modifier.verticalScroll(rememberScrollState())) {
            PageColumn {
                CursusCard {
                    CardTitle(R.drawable.ic_user_alt_1, "Profile", "Name, picture and bio")
                    Column(Modifier.padding(top = 12.dp)) {
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
                        SectionLabel("Description")
                        EditableLine(user?.description ?: "", "Tell people about yourself", LineStyle.Body) { editing = EditField.OwnDescription }
                    }
                }
                CursusCard {
                    CardTitle(R.drawable.ic_link_minimalistic_2, "Address", "How people reach you")
                    Column(Modifier.padding(top = 10.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        val a = address
                        if (a == null) {
                            SecondaryButton(if (loadingAddress) "Loading…" else "Show my address", {
                                loadingAddress = true
                                scope.launch { address = model.addressLink(); loadingAddress = false }
                            }, Modifier.fillMaxWidth(), enabled = !loadingAddress)
                        } else {
                            val shown = a.short ?: a.full
                            LinkBox(shown) { model.notice = "Address copied" }
                            SecondaryButton("Share address", { share(context, shown) }, Modifier.fillMaxWidth())
                        }
                    }
                }
                CursusCard {
                    CardTitle(R.drawable.ic_image_square, "Appearance", "Theme")
                    Row(Modifier.padding(top = 10.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        for (mode in ThemeMode.entries) {
                            val on = model.prefs.theme == mode
                            Text(
                                mode.name,
                                style = Cursus.type.body,
                                color = if (on) c.accent else c.textPrimary,
                                fontWeight = if (on) FontWeight.SemiBold else FontWeight.Normal,
                                modifier = Modifier.weight(1f).clip(Radius.md)
                                    .background(if (on) c.accentSoft else c.bgField)
                                    .clickable { model.prefs.theme = mode }
                                    .padding(vertical = 10.dp)
                                    .then(Modifier),
                                textAlign = androidx.compose.ui.text.style.TextAlign.Center,
                            )
                        }
                    }
                }
                CursusCard {
                    CardTitle(R.drawable.ic_shield_check, "Privacy & security", "Lock, screen, files and receipts")
                    SectionLabel("Links")
                    SwitchRow("Remove tracking from links", model.prefs.cleanLinks, { model.prefs.cleanLinks = it },
                        note = "Cleans utm_*, fbclid… from links you send and open")
                    SectionLabel("Screen")
                    SwitchRow("Hide chats from screenshots", model.prefs.secureScreen, { model.prefs.secureScreen = it },
                        note = "Also blanks Cursus in recent apps")
                    SectionLabel("Lock")
                    var remembered by remember { mutableStateOf(model.rememberedPassword) }
                    if (remembered) {
                        ActionTile(R.drawable.ic_lock, "Forget saved password", {
                            model.forgetPassword(); remembered = false
                            model.notice = "Cursus will ask for your password next time"
                        })
                    }
                    ActionTile(R.drawable.ic_lock, "Lock now", { model.lock() })
                }
                CursusCard {
                    CardTitle(R.drawable.ic_notifications, "Notifications", "Alerts and background running")
                    SwitchRow("Receive in background", model.prefs.background, {
                        model.prefs.background = it
                        model.syncBackground()
                    }, note = "Keeps Cursus running with a silent notification, so messages arrive when the app is closed")
                    SwitchRow("Hide message text in notifications", model.prefs.hideNotificationText, { model.prefs.hideNotificationText = it })
                }
                CursusCard {
                    CardTitle(R.drawable.ic_info_circle, "About", "Version, source and licences")
                    Column(Modifier.padding(top = 10.dp, start = 8.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        Text("Cursus ${model.info?.version ?: ""} for Android", style = Cursus.type.body, color = c.textPrimary)
                        Text(
                            "Free software under AGPL-3.0-or-later, built on the official SimpleX Chat core.",
                            style = Cursus.type.meta, color = c.textSecondary,
                        )
                        Text(
                            "Source code",
                            style = Cursus.type.body, color = c.accent,
                            modifier = Modifier.clickable { uri.openUri("https://github.com/dplz01/cursus") },
                        )
                        Text(
                            "Licences and credits",
                            style = Cursus.type.body, color = c.accent,
                            modifier = Modifier.clickable { showLicences = true },
                        )
                    }
                }
                if (model.busy) CircularProgressIndicator(color = c.accent)
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
        androidx.activity.compose.BackHandler { if (name != null) open = null else onDismiss() }
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
                            modifier = Modifier.padding(horizontal = 8.dp),
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
                val lines by androidx.compose.runtime.produceState<List<String>?>(null, name) {
                    value = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) {
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
