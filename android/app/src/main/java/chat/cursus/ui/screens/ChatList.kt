package chat.cursus.ui.screens

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
        PaneHeader(if (channels) "Channels" else "Chats") {
            HeaderPlus(if (channels) "New channel" else "New chat", onNew)
        }
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
                        if (chat.isRequest) "Wants to connect with you" else chat.preview,
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
