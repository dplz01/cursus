package chat.cursus.ui.screens

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
import chat.cursus.ui.components.NavRow
import chat.cursus.ui.components.CursusOverlay
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
    var shared by remember { mutableStateOf<ConnLink?>(null) }
    var link by rememberSaveable { mutableStateOf(initialLink ?: "") }
    var create by remember { mutableStateOf<CreateKind?>(null) }

    Column(Modifier.fillMaxSize().background(c.bgPanel)) {
        PaneHeader("New", onBack = onClose)
        Column(Modifier.verticalScroll(rememberScrollState())) {
            PageColumn {
                CursusCard {
                    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        CardTitle(R.drawable.ic_link_minimalistic_2, "Connect", "Share a link, or paste one you received.")
                        NavRow(R.drawable.ic_repeat_once, "One-time invite", "A single-use link for one person") {
                            scope.launch { model.createInvitation()?.let { shared = it }; model.reloadChats() }
                        }
                        NavRow(R.drawable.ic_address_card, "My address", "Your reusable contact link") {
                            scope.launch { model.addressLink()?.let { shared = it } }
                        }
                        shared?.let { l ->
                            SectionLabel("Ready to share — tap to copy")
                            Column(Modifier.padding(horizontal = 8.dp)) {
                                LinkBox(l.short ?: l.full) { model.notice = "Link copied" }
                            }
                        }
                        SectionLabel("Join with a link")
                        Column(Modifier.padding(horizontal = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                RoundedField(link, { link = it }, "Invite or address link…", Modifier.weight(1f))
                                IconTool(R.drawable.ic_paste, "Paste", { clipboard.getText()?.let { link = it.text.trim() } })
                            }
                            PrimaryButton("Connect", { model.connect(link.trim()) { onClose() } }, Modifier.fillMaxWidth(), enabled = link.isNotBlank())
                        }
                    }
                }
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
                        ProfileIdentity(user?.displayName ?: "", user?.image) {
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
                                    .background(if (on) c.accentSoft else c.bgSidebar)
                                    .clickable { model.prefs.theme = mode }
                                    .padding(vertical = 10.dp)
                                    .then(Modifier),
                                textAlign = androidx.compose.ui.text.style.TextAlign.Center,
                            )
                        }
                    }
                }
                CursusCard {
                    CardTitle(R.drawable.ic_shield_check, "Privacy & security", "Lock, screen, links and notifications")
                    SectionLabel("Links")
                    SwitchRow("Remove tracking from links", model.prefs.cleanLinks, { model.prefs.cleanLinks = it },
                        note = "Cleans utm_*, fbclid… from links you send and open")
                    SectionLabel("Screen")
                    SwitchRow("Hide chats from screenshots", model.prefs.secureScreen, { model.prefs.secureScreen = it },
                        note = "Also blanks Cursus in recent apps")
                    SwitchRow("Hide message text in notifications", model.prefs.hideNotificationText, { model.prefs.hideNotificationText = it })
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
                    }
                }
                if (model.busy) CircularProgressIndicator(color = c.accent)
            }
        }
    }
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
