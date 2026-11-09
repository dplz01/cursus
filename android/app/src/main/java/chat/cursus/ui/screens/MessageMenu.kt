package chat.cursus.ui.screens

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import chat.cursus.AppModel
import chat.cursus.ComposeMode
import chat.cursus.R
import chat.cursus.core.Message
import chat.cursus.ui.components.CursusOverlay
import chat.cursus.ui.components.DangerButton
import chat.cursus.ui.components.SecondaryButton
import chat.cursus.ui.theme.Cursus
import chat.cursus.ui.theme.Radius

private enum class MenuPane { Actions, ConfirmDelete, ForwardPick, Info }

private enum class MsgAction(val icon: Int) {
    Reply(R.drawable.ic_reply), Copy(R.drawable.ic_copy), Edit(R.drawable.ic_edit_3), Forward(R.drawable.ic_forward),
    Info(R.drawable.ic_info_circle), Delete(R.drawable.ic_trash_delete), Download(R.drawable.ic_download),
}

/** Linux `icon_menu_action`: glyph + label, full width, radius 8. */
@Composable
private fun IconMenuAction(icon: Int, label: String, danger: Boolean = false, onClick: () -> Unit) {
    val c = Cursus.colors
    val tint = if (danger) c.danger else c.textPrimary
    Row(
        Modifier.fillMaxWidth().clip(Radius.sm).clickable(onClick = onClick).padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Icon(painterResource(icon), null, Modifier.size(18.dp), tint = tint)
        Text(label, style = Cursus.type.body, color = tint)
    }
}

/** Linux `message_menu_overlay`: reactions, actions, delete confirm, forward picker, info. */
@Composable
fun MessageMenu(model: AppModel, m: Message, onDismiss: () -> Unit) {
    val c = Cursus.colors
    val clipboard = LocalClipboardManager.current
    var pane by remember { mutableStateOf(MenuPane.Actions) }
    val chat = model.selectedChat
    val deleted = m.deleted != null
    val save = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("*/*")) { uri ->
        val f = m.file
        if (uri != null && f != null) model.saveFile(f, uri)
        onDismiss()
    }

    CursusOverlay(onDismiss) {
        when (pane) {
            MenuPane.Actions -> {
                if (!deleted) {
                    Row(horizontalArrangement = Arrangement.spacedBy(2.dp)) {
                        for (glyph in model.info?.reactions.orEmpty()) {
                            val mine = m.reactions.any { it.glyph == glyph && it.mine }
                            Text(
                                glyph, fontSize = 22.sp,
                                modifier = Modifier.clip(Radius.sm).background(if (mine) c.accentSoft else Color.Transparent)
                                    .clickable { onDismiss(); model.react(m, glyph, !mine) }.padding(5.dp),
                            )
                        }
                    }
                }
                Text("Message", style = Cursus.type.meta, color = c.textMuted)
                Column {
                    val f = m.file
                    if (!deleted && f != null && f.state == "complete" && f.path != null) {
                        IconMenuAction(MsgAction.Download.icon, "Download") { save.launch(f.name) }
                    }
                    val actions: List<Pair<MsgAction, String>> = when {
                        deleted -> listOf(MsgAction.Info to "Info", MsgAction.Delete to "Delete for me")
                        m.outgoing -> listOfNotNull(
                            MsgAction.Reply to "Reply", MsgAction.Copy to "Copy",
                            if (m.canEdit) MsgAction.Edit to "Edit" else null,
                            MsgAction.Forward to "Forward", MsgAction.Info to "Info",
                            MsgAction.Delete to "Delete for everyone",
                        )
                        else -> listOf(
                            MsgAction.Reply to "Reply", MsgAction.Copy to "Copy", MsgAction.Forward to "Forward",
                            MsgAction.Info to "Info", MsgAction.Delete to "Delete for me",
                        )
                    }
                    for ((action, label) in actions) {
                        IconMenuAction(action.icon, label, danger = action == MsgAction.Delete) {
                            when (action) {
                                MsgAction.Reply -> { onDismiss(); model.composeMode = ComposeMode.Reply(m) }
                                MsgAction.Copy -> { onDismiss(); clipboard.setText(AnnotatedString(m.caption.ifBlank { m.text })) }
                                MsgAction.Edit -> { onDismiss(); model.startEdit(m) }
                                MsgAction.Forward -> pane = MenuPane.ForwardPick
                                MsgAction.Info -> pane = MenuPane.Info
                                MsgAction.Delete -> pane = MenuPane.ConfirmDelete
                                MsgAction.Download -> Unit
                            }
                        }
                    }
                }
            }
            MenuPane.ConfirmDelete -> {
                // Linux wording: own messages go for everyone; Full deletion decides how complete that is.
                val full = when {
                    chat == null -> false
                    chat.isGroup -> chat.groupPrefs?.features?.get("fullDelete")?.on == true
                    else -> chat.contactPrefs?.features?.get("fullDelete")?.enabled == true
                }
                val forEveryone = m.outgoing && !deleted
                val (title, blurb) = when {
                    !forEveryone -> "Delete for me?" to "Removes the message only on this device. The sender still has it."
                    full -> "Delete for everyone?" to "Full deletion is on — the message will be removed for you and the contact."
                    else -> "Delete for everyone?" to
                        "Contact has not allowed Full deletion — they will only see it marked deleted (content may remain). Enable Full deletion on both sides for a real wipe."
                }
                Text(title, style = Cursus.type.body, color = c.textPrimary, fontWeight = FontWeight.SemiBold)
                Text(blurb, style = Cursus.type.meta, color = c.textSecondary)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    SecondaryButton("Cancel", { pane = MenuPane.Actions }, Modifier.weight(1f))
                    DangerButton("Delete", { onDismiss(); model.deleteMessage(m, forEveryone) }, Modifier.weight(1f))
                }
            }
            MenuPane.ForwardPick -> {
                Text("Forward to…", style = Cursus.type.body, color = c.textPrimary, fontWeight = FontWeight.SemiBold)
                Column(Modifier.heightIn(max = 360.dp).verticalScroll(rememberScrollState())) {
                    for (target in model.chats.filter { it.ref != model.selection && !it.isRequest }) {
                        Text(
                            target.title, style = Cursus.type.body, color = c.textPrimary, maxLines = 1, overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.fillMaxWidth().clip(Radius.sm).clickable { onDismiss(); model.forward(m, target) }
                                .padding(horizontal = 12.dp, vertical = 10.dp),
                        )
                    }
                }
                SecondaryButton("Cancel", onDismiss)
            }
            MenuPane.Info -> {
                val info by produceState<chat.cursus.core.MessageInfo?>(null) { value = model.messageInfo(m) }
                Text("Message info", style = Cursus.type.body, color = c.textPrimary, fontWeight = FontWeight.SemiBold)
                Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text("Item #${m.id}", style = Cursus.type.meta, color = c.textSecondary)
                    info?.time?.let { Text("Time: $it", style = Cursus.type.meta, color = c.textSecondary) }
                    info?.status?.let { Text("Status: $it", style = Cursus.type.meta, color = c.textSecondary) }
                    m.file?.let { Text("File: ${it.name} (${humanSize(it.size)})", style = Cursus.type.meta, color = c.textSecondary) }
                    info?.text?.takeIf { it.isNotBlank() }?.let {
                        Text("Text: $it", style = Cursus.type.meta, color = c.textSecondary, maxLines = 8, overflow = TextOverflow.Ellipsis)
                    }
                }
                SecondaryButton("Close", onDismiss)
            }
        }
    }
}

/** Linux `reaction_members_overlay`: who reacted with one emoji (groups). */
@Composable
fun ReactionMembers(model: AppModel, m: Message, emoji: String, onDismiss: () -> Unit) {
    val c = Cursus.colors
    val names by produceState<List<String>?>(null, emoji) { value = model.reactionMembers(m, emoji) }
    val glyph = m.reactions.firstOrNull { it.emoji == emoji }?.glyph ?: emoji
    CursusOverlay(onDismiss) {
        Text("Reacted $glyph", style = Cursus.type.body, color = c.textPrimary, fontWeight = FontWeight.SemiBold)
        when {
            names == null -> Text("Loading…", style = Cursus.type.meta, color = c.textMuted)
            names!!.isEmpty() -> Text("No members listed.", style = Cursus.type.meta, color = c.textMuted)
            else -> Column(Modifier.heightIn(max = 320.dp).verticalScroll(rememberScrollState())) {
                for (n in names!!) Text(n, style = Cursus.type.body, color = c.textPrimary, modifier = Modifier.padding(vertical = 4.dp))
            }
        }
        SecondaryButton("Close", onDismiss)
    }
}
