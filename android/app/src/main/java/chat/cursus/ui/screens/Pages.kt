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

@Composable
fun NewChatPage(model: AppModel, initialLink: String?, onClose: () -> Unit, onOpened: (String) -> Unit) {
    val c = Cursus.colors
    val context = LocalContext.current
    val clipboard = LocalClipboardManager.current
    val scope = rememberCoroutineScope()
    var invitation by remember { mutableStateOf<ConnLink?>(null) }
    var creating by remember { mutableStateOf(false) }
    var link by rememberSaveable { mutableStateOf(initialLink ?: "") }
    var groupName by rememberSaveable { mutableStateOf("") }
    var channelName by rememberSaveable { mutableStateOf("") }

    Column(Modifier.fillMaxSize().background(c.bgPanel)) {
        PaneHeader("New chat", onBack = onClose)
        Column(Modifier.verticalScroll(rememberScrollState())) {
            PageColumn {
                CursusCard {
                    CardTitle(R.drawable.ic_link_minimalistic_2, "Invite someone", "A one-time link: whoever opens it first connects with you")
                    Column(Modifier.padding(top = 10.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        val inv = invitation
                        if (inv == null) {
                            PrimaryButton(if (creating) "Creating…" else "Create link", {
                                creating = true
                                scope.launch {
                                    invitation = model.createInvitation()
                                    creating = false
                                    model.reloadChats()
                                }
                            }, Modifier.fillMaxWidth(), enabled = !creating)
                        } else {
                            val shown = inv.short ?: inv.full
                            LinkBox(shown) { model.notice = "Link copied" }
                            SecondaryButton("Share link", { share(context, shown) }, Modifier.fillMaxWidth())
                        }
                    }
                }
                CursusCard {
                    CardTitle(R.drawable.ic_paste, "Connect with a link", "Paste a link someone shared with you")
                    Column(Modifier.padding(top = 10.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        CursusField(link, { link = it }, "simplex:/… or https://smp…", singleLine = false, trailing = {
                            IconTool(R.drawable.ic_paste, "Paste", { clipboard.getText()?.let { link = it.text } })
                        })
                        PrimaryButton("Connect", { model.connect(link.trim()) { onClose() } }, Modifier.fillMaxWidth(), enabled = link.isNotBlank())
                    }
                }
                CursusCard {
                    CardTitle(R.drawable.ic_user_group, "New group", "Chat with several people")
                    Column(Modifier.padding(top = 10.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        CursusField(groupName, { groupName = it }, "Group name")
                        SecondaryButton("Create group", {
                            model.createGroup(groupName.trim(), channel = false) { onOpened(it) }
                        }, Modifier.fillMaxWidth(), enabled = groupName.isNotBlank())
                    }
                }
                CursusCard {
                    CardTitle(R.drawable.ic_channels, "New channel", "Broadcast to subscribers through relays")
                    Column(Modifier.padding(top = 10.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        CursusField(channelName, { channelName = it }, "Channel name")
                        SecondaryButton("Create channel", {
                            model.createGroup(channelName.trim(), channel = true) { onOpened(it) }
                        }, Modifier.fillMaxWidth(), enabled = channelName.isNotBlank())
                    }
                }
            }
        }
    }
}

@Composable
fun ChatInfoPage(model: AppModel, chat: Chat, onClose: () -> Unit) {
    val c = Cursus.colors
    var confirmDelete by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxSize().background(c.bgPanel)) {
        PaneHeader(if (chat.isChannel) "Channel" else if (chat.isGroup) "Group" else "Contact", onBack = onClose)
        Column(Modifier.verticalScroll(rememberScrollState())) {
            PageColumn {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                    Avatar(chat.title, chat.image, 72.dp, group = chat.isGroup)
                    Column {
                        Text(chat.title, style = Cursus.type.title, color = c.textPrimary)
                        if (chat.alias.isNotBlank() && chat.name != chat.alias) {
                            Text(chat.name, style = Cursus.type.meta, color = c.textSecondary)
                        }
                        if (chat.fullName.isNotBlank()) Text(chat.fullName, style = Cursus.type.meta, color = c.textSecondary)
                    }
                }
                val about = chat.shortDescr.ifBlank { chat.description }
                if (about.isNotBlank()) {
                    CursusCard { Text(about, style = Cursus.type.body, color = c.textPrimary) }
                }
                CursusCard {
                    SwitchRow("Favourite", chat.favorite, { model.setFavorite(chat, it) })
                    SwitchRow("Notifications", !chat.muted, { model.setMuted(chat, !it) })
                }
                CursusCard {
                    ActionTile(R.drawable.ic_clear_all, "Clear messages", { model.clear(chat) })
                    ActionTile(
                        R.drawable.ic_trash_delete,
                        if (chat.isGroup && chat.role != "owner") "Leave and delete" else "Delete",
                        { confirmDelete = true },
                        danger = true,
                    )
                }
            }
        }
    }
    if (confirmDelete) {
        ConfirmDialog(
            title = "Delete ${chat.title}?",
            text = "This can't be undone.",
            action = "Delete",
            onConfirm = { confirmDelete = false; onClose(); model.delete(chat) },
            onDismiss = { confirmDelete = false },
        )
    }
}

@Composable
fun SettingsPane(model: AppModel) {
    val c = Cursus.colors
    val scope = rememberCoroutineScope()
    val uri = LocalUriHandler.current
    val user = model.user
    var name by rememberSaveable(user?.displayName) { mutableStateOf(user?.displayName ?: "") }
    var bio by rememberSaveable(user?.shortDescr) { mutableStateOf(user?.shortDescr ?: "") }
    var address by remember { mutableStateOf<ConnLink?>(null) }
    var loadingAddress by remember { mutableStateOf(false) }
    val context = LocalContext.current

    Column(Modifier.fillMaxSize()) {
        PaneHeader("Settings")
        Column(Modifier.verticalScroll(rememberScrollState())) {
            PageColumn {
                CursusCard {
                    CardTitle(R.drawable.ic_user_alt_1, "Profile", "Name, picture and bio")
                    Row(
                        Modifier.padding(top = 12.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(14.dp),
                    ) {
                        Avatar(user?.displayName ?: "", user?.image, 72.dp)
                        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            CursusField(name, { name = it }, "Your name")
                            CursusField(bio, { bio = it }, "Bio")
                        }
                    }
                    val changed = name.trim() != (user?.displayName ?: "") || bio != (user?.shortDescr ?: "")
                    if (changed) {
                        PrimaryButton("Save", {
                            model.updateProfile(displayName = name.trim(), shortDescr = bio)
                        }, Modifier.fillMaxWidth().padding(top = 10.dp), enabled = name.isNotBlank())
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
}
