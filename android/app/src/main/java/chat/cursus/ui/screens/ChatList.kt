package chat.cursus.ui.screens

import androidx.compose.ui.graphics.Color
import androidx.compose.foundation.layout.defaultMinSize
import chat.cursus.ui.components.RoundedField
import chat.cursus.ui.components.CursusOverlay
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import chat.cursus.AppModel
import chat.cursus.R
import chat.cursus.core.Chat
import chat.cursus.core.Stamps
import chat.cursus.ui.components.Avatar
import chat.cursus.ui.components.DangerButton
import chat.cursus.ui.components.IconTool
import chat.cursus.ui.components.PrimaryButton
import chat.cursus.ui.components.SecondaryButton
import chat.cursus.ui.components.UnreadBadge
import chat.cursus.ui.theme.Cursus
import chat.cursus.ui.theme.Radius

/** Title bar of a list / page: title, then tools on the right. */
@Composable
fun PaneHeader(
    title: String,
    onBack: (() -> Unit)? = null,
    actions: @Composable RowScope.() -> Unit = {},
) {
    Row(
        Modifier.fillMaxWidth().height(56.dp).padding(horizontal = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (onBack != null) {
            IconTool(R.drawable.ic_back, "Back", onBack)
        } else {
            Box(Modifier.size(8.dp))
        }
        Text(
            title,
            style = Cursus.type.title,
            color = Cursus.colors.textPrimary,
            modifier = Modifier.weight(1f).padding(horizontal = 4.dp),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        actions()
    }
}

/** Linux list header "+": secondary button, 36px, accent glyph. */
@Composable
fun HeaderPlus(description: String, onClick: () -> Unit) {
    val c = Cursus.colors
    Box(
        Modifier.padding(end = 8.dp).size(36.dp).clip(Radius.md).background(c.bgField)
            .border(1.dp, c.border, Radius.md).clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(painterResource(R.drawable.ic_plus), description, Modifier.size(20.dp), tint = c.accent)
    }
}

@Composable
fun ChatListPane(model: AppModel, channels: Boolean, onNew: () -> Unit) {
    val c = Cursus.colors
    val chats = model.chats
        .filter { it.isChannel == channels }
        .sortedWith(compareByDescending<Chat> { it.isRequest }.thenByDescending { it.favorite }.thenByDescending { it.lastTs ?: "" })
    Column(Modifier.fillMaxSize()) {
        var pendingOpen by remember { mutableStateOf(false) }
        PaneHeader(if (channels) "Channels" else "Chats") {
            if (model.pendingInvitations.isNotEmpty()) {
                PendingLinksButton(model.pendingInvitations.size) { pendingOpen = true }
            }
            HeaderPlus(if (channels) "New channel" else "New chat", onNew)
        }
        if (pendingOpen) PendingLinksOverlay(model) { pendingOpen = false }
        if (chats.isEmpty()) {
            Column(
                Modifier.fillMaxSize().padding(24.dp),
                verticalArrangement = Arrangement.Center,
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Text(
                    if (channels) "Channels you join or create show up here." else "Invite someone or paste their link to start a chat.",
                    style = Cursus.type.body, color = c.textMuted,
                )
                Box(Modifier.height(16.dp))
                PrimaryButton(if (channels) "New channel" else "New chat", onNew)
            }
        } else {
            LazyColumn(Modifier.fillMaxSize()) {
                items(chats, key = { it.ref }) { chat ->
                    ChatRow(model, chat, selected = chat.ref == model.selection)
                }
            }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun ChatRow(model: AppModel, chat: Chat, selected: Boolean) {
    val c = Cursus.colors
    var menu by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }
    Box {
        Row(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = 8.dp, vertical = 2.dp)
                .clip(Radius.lg)
                .background(if (selected) c.accentSoft else c.bgList)
                .combinedClickable(onClick = { model.select(chat.ref) }, onLongClick = { menu = true })
                .padding(horizontal = 8.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Avatar(chat.title, chat.image, 44.dp, group = chat.isGroup)
            Column(Modifier.weight(1f)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        chat.title.ifBlank { "Unnamed" },
                        style = Cursus.type.body,
                        color = c.textPrimary,
                        fontWeight = if (chat.unread > 0) FontWeight.SemiBold else FontWeight.Normal,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f, fill = false),
                    )
                    if (chat.favorite) {
                        Icon(painterResource(R.drawable.ic_star_filled), "Favourite", Modifier.padding(start = 4.dp).size(14.dp), tint = c.gold)
                    }
                    if (chat.muted) {
                        Icon(painterResource(R.drawable.ic_notifications_off), "Muted", Modifier.padding(start = 4.dp).size(14.dp), tint = c.textMuted)
                    }
                    Box(Modifier.weight(1f, fill = true))
                    Text(Stamps.list(chat.lastTs), style = Cursus.type.meta, color = c.textMuted)
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        when {
                            chat.isRequest -> "Wants to connect with you"
                            // "Message previews in chat list" off: names only.
                            !model.prefs.chatPreviews -> ""
                            else -> chat.preview
                        },
                        style = Cursus.type.meta,
                        color = if (chat.isRequest) c.gold else c.textSecondary,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f),
                    )
                    if (chat.unread > 0 || chat.unreadChat) {
                        Box(Modifier.padding(start = 8.dp)) { UnreadBadge(chat.unread, muted = chat.muted) }
                    }
                }
            }
        }
        DropdownMenu(menu, { menu = false }, containerColor = c.bgElevated, shape = Radius.lg) {
            val unread = chat.unread > 0 || chat.unreadChat
            MenuEntry(if (unread) "Mark as read" else "Mark as unread") {
                menu = false
                if (unread) model.select(chat.ref) else model.markUnread(chat)
            }
            MenuEntry(if (chat.favorite) "Remove from favourites" else "Add to favourites") {
                menu = false; model.setFavorite(chat, !chat.favorite)
            }
            MenuEntry(if (chat.muted) "Unmute" else "Mute") {
                menu = false; model.setMuted(chat, !chat.muted)
            }
            MenuEntry("Clear messages") { menu = false; model.clear(chat) }
            MenuEntry("Delete", danger = true) { menu = false; confirmDelete = true }
        }
        if (confirmDelete) {
            ConfirmDialog(
                title = "Delete ${chat.title}?",
                text = if (chat.isGroup) "You leave the group and its messages are deleted from this phone." else "The chat and its messages are deleted from this phone.",
                action = "Delete",
                onConfirm = { confirmDelete = false; model.delete(chat) },
                onDismiss = { confirmDelete = false },
            )
        }
    }
}

@Composable
fun MenuEntry(text: String, danger: Boolean = false, onClick: () -> Unit) {
    DropdownMenuItem(
        text = { Text(text, style = Cursus.type.body, color = if (danger) Cursus.colors.danger else Cursus.colors.textPrimary) },
        onClick = onClick,
    )
}

/** Confirmation in the overlay card: question + Cancel / solid danger. */
@Composable
fun ConfirmDialog(title: String, text: String, action: String, onConfirm: () -> Unit, onDismiss: () -> Unit) {
    val c = Cursus.colors
    chat.cursus.ui.components.CursusOverlay(onDismiss) {
        Text(title, style = Cursus.type.body, color = c.textPrimary, fontWeight = FontWeight.SemiBold)
        Text(text, style = Cursus.type.meta, color = c.textSecondary)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            SecondaryButton("Cancel", onDismiss, Modifier.weight(1f))
            DangerButton(action, onConfirm, Modifier.weight(1f))
        }
    }
}


/** Linux `pending_links_button`: attention-yellow square (same box as "+")
 *  with the number of unused one-time links; opens their list. The count is
 *  round (a pill past 9), borderless, centred on the top-right corner. */
@Composable
private fun PendingLinksButton(count: Int, onClick: () -> Unit) {
    val c = Cursus.colors
    Box(Modifier.padding(end = 8.dp).size(36.dp)) {
        Box(
            Modifier.fillMaxSize().clip(Radius.md).background(c.attention).clickable(onClick = onClick),
            contentAlignment = Alignment.Center,
        ) {
            Icon(painterResource(R.drawable.ic_link_minimalistic_2), "Pending links", Modifier.size(20.dp), tint = Color(0xFF1F1A05))
        }
        Box(
            Modifier.align(Alignment.TopEnd).offset(x = 8.dp, y = (-8).dp)
                .defaultMinSize(minWidth = 16.dp, minHeight = 16.dp)
                .clip(Radius.pill).background(c.accent)
                .padding(horizontal = if (count < 10) 0.dp else 4.dp),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                if (count > 99) "99+" else "$count",
                style = Cursus.type.meta.copy(fontSize = 10.sp, lineHeight = 10.sp),
                color = c.accentOn, fontWeight = FontWeight.Bold,
            )
        }
    }
}

/** Unused one-time links: copy (short / full), name, delete (asks first).
 *  Anyone holding one can still connect, so they stay visible here. */
@Composable
private fun PendingLinksOverlay(model: AppModel, onDismiss: () -> Unit) {
    val c = Cursus.colors
    val clipboard = androidx.compose.ui.platform.LocalClipboardManager.current
    var renaming by remember { mutableStateOf<Long?>(null) }
    var draft by remember { mutableStateOf("") }
    var deleting by remember { mutableStateOf<Long?>(null) }
    val copy = { text: String -> clipboard.setText(androidx.compose.ui.text.AnnotatedString(text)); model.notice = "Link copied" }
    CursusOverlay(onDismiss) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Box(Modifier.size(36.dp).clip(Radius.pill).background(c.accentSoft), contentAlignment = Alignment.Center) {
                Icon(painterResource(R.drawable.ic_link_minimalistic_2), null, Modifier.size(18.dp), tint = c.attention)
            }
            Text("Pending links", style = Cursus.type.body, color = c.textPrimary, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
            IconTool(R.drawable.ic_cancel, "Close", onDismiss)
        }
        Text(
            "One-time links you created that nobody has used yet. Anyone who has one can still connect \u2014 delete the ones you no longer need.",
            style = Cursus.type.meta, color = c.textSecondary,
        )
        if (model.pendingInvitations.isEmpty()) {
            Text("No pending links.", style = Cursus.type.meta, color = c.textMuted)
        }
        Column(
            Modifier.heightIn(max = 420.dp).verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            for (inv in model.pendingInvitations) {
                Column(
                    Modifier.fillMaxWidth().clip(Radius.lg).background(c.bgField).border(1.dp, c.border, Radius.lg)
                        .padding(horizontal = 12.dp, vertical = 10.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    if (renaming == inv.connId) {
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            RoundedField(draft, { draft = it }, "Name, e.g. who it's for", Modifier.weight(1f))
                            PrimaryButton("Save", {
                                model.setInvitationAlias(inv.connId, draft.trim())
                                renaming = null
                            })
                        }
                    } else {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Column(Modifier.weight(1f)) {
                                Text(
                                    inv.alias.ifBlank { "Unnamed link" }, style = Cursus.type.body,
                                    color = if (inv.alias.isBlank()) c.textSecondary else c.textPrimary,
                                )
                                val created = Stamps.list(inv.createdAt).takeIf { it.isNotEmpty() }?.let { "Created $it" }
                                Text(
                                    listOfNotNull(created, if (inv.incognito) "incognito" else null).joinToString(" \u00b7 "),
                                    style = Cursus.type.meta, color = c.textMuted,
                                )
                            }
                            IconTool(R.drawable.ic_trash_delete, "Delete link", { deleting = inv.connId }, tint = c.danger)
                        }
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        if (inv.link.short != null) {
                            SecondaryButton("Copy", { copy(inv.link.display) })
                            SecondaryButton("Copy full", { copy(inv.link.full) })
                        } else {
                            SecondaryButton("Copy", { copy(inv.link.full) })
                        }
                        SecondaryButton("Name", { draft = inv.alias; renaming = inv.connId })
                    }
                }
            }
        }
    }
    deleting?.let { id ->
        CursusOverlay({ deleting = null }) {
            Text("Delete this link?", style = Cursus.type.body, color = c.textPrimary, fontWeight = FontWeight.SemiBold)
            Text("It will stop working. Anyone you sent it to won't be able to connect with it.", style = Cursus.type.meta, color = c.textSecondary)
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End)) {
                SecondaryButton("Cancel", { deleting = null })
                DangerButton("Delete", { model.deleteInvitation(id); deleting = null })
            }
        }
    }
}
