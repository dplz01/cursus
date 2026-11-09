package chat.cursus.ui.screens

import android.content.Intent
import android.graphics.BitmapFactory
import android.net.Uri
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.union
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withLink
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import chat.cursus.AppModel
import chat.cursus.ComposeMode
import chat.cursus.R
import chat.cursus.core.Chat
import chat.cursus.core.Message
import chat.cursus.core.Segment
import chat.cursus.core.Stamps
import chat.cursus.ui.components.AccentSquare
import chat.cursus.ui.components.Avatar
import chat.cursus.ui.components.IconTool
import chat.cursus.ui.components.PrimaryButton
import chat.cursus.ui.components.SecondaryButton
import chat.cursus.ui.theme.Cursus
import chat.cursus.ui.theme.CursusColors
import chat.cursus.ui.theme.Radius
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.time.LocalDate


@Composable
fun ConversationPane(model: AppModel, chat: Chat, showBack: Boolean, onInfo: () -> Unit) {
    val c = Cursus.colors
    Column(Modifier.fillMaxSize().background(c.bgPanel).windowInsetsPadding(WindowInsets.ime.union(WindowInsets.navigationBars))) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 4.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (showBack) IconTool(R.drawable.ic_back, "Back", { model.select(null) })
            Row(
                Modifier.weight(1f).clip(Radius.lg).clickable(onClick = onInfo).padding(6.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Avatar(chat.title, chat.image, 36.dp, group = chat.isGroup)
                Column {
                    Text(chat.title, style = Cursus.type.body, color = c.textPrimary, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    val sub = when {
                        chat.isChannel -> "Channel"
                        chat.isGroup -> "Group"
                        chat.isNotes -> "Notes"
                        else -> chat.shortDescr.ifBlank { null }
                    }
                    if (sub != null) Text(sub, style = Cursus.type.meta, color = c.textSecondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
            IconTool(R.drawable.ic_info_circle, "Info", onInfo)
        }
        Box(Modifier.fillMaxWidth().background(c.border).heightIn(min = 1.dp, max = 1.dp))

        Box(Modifier.weight(1f)) {
            MessageList(model, chat)
        }

        when {
            chat.isRequest -> RequestBar(model, chat)
            chat.isChannel && chat.role != "owner" -> Unit // only owners post in channels
            else -> Composer(model)
        }
    }
}

@Composable
private fun RequestBar(model: AppModel, chat: Chat) {
    Column(Modifier.fillMaxWidth().padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text("${chat.title} wants to connect with you.", style = Cursus.type.body, color = Cursus.colors.textPrimary)
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            SecondaryButton("Reject", { model.rejectRequest(chat) }, Modifier.weight(1f))
            PrimaryButton("Accept", { model.acceptRequest(chat) }, Modifier.weight(1f))
        }
    }
}

/** One row of the history: a day label or a message. */
private sealed interface Row_ {
    data class Day(val date: LocalDate) : Row_
    data class Msg(val message: Message, val firstOfRun: Boolean) : Row_
}

@Composable
private fun MessageList(model: AppModel, chat: Chat) {
    val rows = remember(model.messages.toList()) {
        buildList<Row_> {
            var lastDay: LocalDate? = null
            var lastSender: Long? = null
            for (m in model.messages) {
                val day = Stamps.day(m.createdAt)
                if (day != null && day != lastDay) {
                    add(Row_.Day(day)); lastDay = day; lastSender = null
                }
                val sender = if (m.outgoing) -1L else m.sender?.memberId ?: -2L
                add(Row_.Msg(m, sender != lastSender))
                lastSender = sender
            }
        }.asReversed()
    }
    val state = rememberLazyListState()
    // Newest at the bottom: the list is reversed, item 0 is the latest.
    LaunchedEffect(model.messages.lastOrNull()?.id) {
        if (state.firstVisibleItemIndex <= 2 || model.messages.lastOrNull()?.outgoing == true) state.animateScrollToItem(0)
    }
    if (model.messagesFor != chat.ref) return
    if (rows.isEmpty()) {
        EmptyPane(if (chat.isNotes) "Notes to yourself, only on this phone." else "No messages yet.")
        return
    }
    LazyColumn(
        Modifier.fillMaxSize(),
        state = state,
        reverseLayout = true,
        contentPadding = androidx.compose.foundation.layout.PaddingValues(vertical = 8.dp),
    ) {
        items(rows.size, key = { i ->
            when (val r = rows[i]) {
                is Row_.Day -> "d" + r.date
                is Row_.Msg -> "m" + r.message.id
            }
        }) { i ->
            when (val r = rows[i]) {
                is Row_.Day -> DayLabel(r.date)
                is Row_.Msg -> MessageRow(model, chat, r.message, r.firstOfRun)
            }
        }
    }
}

@Composable
private fun DayLabel(date: LocalDate) {
    Box(Modifier.fillMaxWidth().padding(vertical = 8.dp), contentAlignment = Alignment.Center) {
        Text(
            Stamps.dayLabel(date),
            style = Cursus.type.meta,
            color = Cursus.colors.textMuted,
            modifier = Modifier.clip(Radius.pill).background(Cursus.colors.bgSidebar).padding(horizontal = 10.dp, vertical = 3.dp),
        )
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun MessageRow(model: AppModel, chat: Chat, m: Message, firstOfRun: Boolean) {
    val c = Cursus.colors
    if (m.announcement) {
        Text(
            m.text,
            style = Cursus.type.meta,
            color = c.textMuted,
            textAlign = TextAlign.Center,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 32.dp, vertical = 4.dp),
        )
        return
    }
    var menu by remember { mutableStateOf(false) }
    var whoReacted by remember { mutableStateOf<String?>(null) }
    whoReacted?.let { emoji -> ReactionMembers(model, m, emoji) { whoReacted = null } }
    val groupIncoming = chat.isGroup && !m.outgoing
    Row(
        Modifier.fillMaxWidth().padding(start = 8.dp, end = 8.dp, top = if (firstOfRun) 6.dp else 2.dp),
        horizontalArrangement = if (m.outgoing) Arrangement.End else Arrangement.Start,
    ) {
        if (groupIncoming) {
            Box(Modifier.width(40.dp)) {
                if (firstOfRun) Avatar(m.sender?.name ?: "", m.sender?.image, 32.dp)
            }
        }
        Column(horizontalAlignment = if (m.outgoing) Alignment.End else Alignment.Start, modifier = Modifier.widthIn(max = 320.dp)) {
            Box {
                Column(
                    Modifier
                        .clip(bubbleShape(m.outgoing, firstOfRun, chat.isGroup))
                        .background(if (m.outgoing) c.accentSoft else c.bgElevated)
                        .then(if (m.outgoing || c.isDark) Modifier else Modifier.border(1.dp, c.border, bubbleShape(false, firstOfRun, chat.isGroup)))
                        .combinedClickable(onClick = {}, onLongClick = { menu = true })
                        .padding(horizontal = 12.dp, vertical = 8.dp),
                ) {
                    if (groupIncoming && firstOfRun) {
                        Text(
                            m.sender?.name ?: "",
                            style = Cursus.type.meta,
                            color = c.accent,
                            fontWeight = FontWeight.SemiBold,
                            maxLines = 1,
                        )
                    }
                    m.quote?.let { q ->
                        Row(
                            Modifier.padding(bottom = 6.dp).clip(Radius.sm).background(c.bgSidebar.copy(alpha = 0.7f)),
                        ) {
                            Box(Modifier.width(3.dp).heightIn(min = 32.dp).background(c.accent))
                            Text(
                                q.text,
                                style = Cursus.type.meta,
                                color = c.textSecondary,
                                maxLines = 3,
                                overflow = TextOverflow.Ellipsis,
                                modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp),
                            )
                        }
                    }
                    MessageBody(model, m)
                    Row(
                        Modifier.align(Alignment.End).padding(top = 2.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        if (m.edited) Text("edited", style = Cursus.type.meta.copy(fontSize = 11.sp), color = c.textMuted)
                        Text(Stamps.time(m.createdAt), style = Cursus.type.meta.copy(fontSize = 11.sp), color = c.textMuted)
                        if (m.outgoing) DeliveryGlyph(m.delivery, c)
                    }
                }
                if (menu) MessageMenu(model, m) { menu = false }
            }
            if (m.reactions.isNotEmpty()) {
                Row(Modifier.padding(top = 4.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    for (r in m.reactions) {
                        Text(
                            if (r.total > 1) "${r.glyph} ${r.total}" else r.glyph,
                            style = Cursus.type.meta,
                            color = c.textPrimary,
                            modifier = Modifier
                                .clip(RoundedChip)
                                .background(if (r.mine) c.accentSoft else c.bgElevated)
                                .border(1.dp, if (r.mine) c.accent else c.border, RoundedChip)
                                .combinedClickable(
                                    onClick = { model.react(m, r.emoji, !r.mine) },
                                    onLongClick = { if (chat.isGroup) whoReacted = r.emoji },
                                )
                                .padding(horizontal = 8.dp, vertical = 2.dp),
                        )
                    }
                }
            }
        }
    }
}

private val RoundedChip = androidx.compose.foundation.shape.RoundedCornerShape(14.dp)

/** Bubbles round off at 16; the run's first one keeps a small "tail" corner at the top. */
private fun bubbleShape(outgoing: Boolean, first: Boolean, group: Boolean) =
    androidx.compose.foundation.shape.RoundedCornerShape(
        topStart = if (!outgoing && first) 4.dp else 16.dp,
        topEnd = if (outgoing && first) 4.dp else 16.dp,
        bottomStart = 16.dp,
        bottomEnd = 16.dp,
    )

@Composable
private fun DeliveryGlyph(delivery: String, c: CursusColors) {
    val (icon, tint) = when (delivery) {
        "pending" -> R.drawable.ic_history to c.textMuted
        "delivered" -> R.drawable.ic_check_read to c.accent
        "error" -> R.drawable.ic_block_prohibited to c.danger
        "warning" -> R.drawable.ic_history to c.gold
        else -> R.drawable.ic_check to c.textMuted
    }
    Icon(painterResource(icon), delivery, Modifier.size(14.dp), tint = tint)
}

@Composable
private fun MessageBody(model: AppModel, m: Message) {
    val c = Cursus.colors
    if (m.deleted != null) {
        Text("message deleted", style = Cursus.type.body, color = c.textMuted, fontStyle = FontStyle.Italic)
        return
    }
    m.groupInvitation?.let { g ->
        Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text("Invitation to ${g.name}", style = Cursus.type.body, color = c.textPrimary, fontWeight = FontWeight.Medium)
            if (g.pending && !m.outgoing) PrimaryButton("Join group", { model.joinGroup(g.groupId) })
        }
        return
    }
    m.file?.let { f ->
        FileBody(model, m, f)
        if (m.caption.isNotBlank()) RichText(model, m.segments.ifEmpty { listOf(Segment(m.caption)) }, m.emojiOnly)
        return
    }
    RichText(model, m.segments.ifEmpty { listOf(Segment(m.text)) }, m.emojiOnly)
}

/** SimpleX markdown segments (parsed by the core) → styled text with tappable links. */
@Composable
private fun RichText(model: AppModel, segments: List<Segment>, emojiOnly: Boolean) {
    val c = Cursus.colors
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var revealed by remember { mutableStateOf(setOf<Int>()) }
    val text = buildAnnotatedString {
        segments.forEachIndexed { i, s ->
            val style = when (s.style) {
                "bold" -> SpanStyle(fontWeight = FontWeight.Bold)
                "italic" -> SpanStyle(fontStyle = FontStyle.Italic)
                "strike" -> SpanStyle(textDecoration = TextDecoration.LineThrough)
                "snippet" -> SpanStyle(fontFamily = FontFamily.Monospace, background = c.bgSidebar)
                "small" -> SpanStyle(fontSize = 13.sp, color = c.textSecondary)
                "colored" -> SpanStyle(color = segmentColor(s.color, c))
                "mention" -> SpanStyle(color = c.accent, fontWeight = FontWeight.Medium)
                "secret" -> if (i in revealed) SpanStyle(background = c.bgSidebar) else SpanStyle(color = Color.Transparent, background = c.textMuted)
                else -> SpanStyle()
            }
            val link = s.link
            when {
                link != null && s.style in setOf("uri", "email", "phone") -> withLink(
                    LinkAnnotation.Clickable("l$i") {
                        scope.launch {
                            val prepared = model.prepareLink(link)
                            when {
                                prepared.simplex != null -> model.pendingLink = prepared.url
                                prepared.url != null -> runCatching {
                                    context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(prepared.url)))
                                }
                                else -> model.notice = "This link can't be opened."
                            }
                        }
                    },
                ) { withStyle(SpanStyle(color = c.accent, textDecoration = TextDecoration.Underline)) { append(s.text) } }
                s.style == "secret" -> withLink(
                    LinkAnnotation.Clickable("s$i") { revealed = if (i in revealed) revealed - i else revealed + i },
                ) { withStyle(style) { append(s.text) } }
                else -> withStyle(style) { append(s.text) }
            }
        }
    }
    Text(
        text,
        style = if (emojiOnly) Cursus.type.body.copy(fontSize = 32.sp) else Cursus.type.body,
        color = c.textPrimary,
    )
}

private fun segmentColor(name: String?, c: CursusColors): Color = when (name) {
    "red" -> c.danger
    "green" -> c.success
    "blue" -> c.accent
    "yellow" -> c.gold
    "cyan" -> Color(0xFF00A5B5)
    "magenta" -> Color(0xFFB0379A)
    "black" -> if (c.isDark) c.textSecondary else Color.Black
    "white" -> if (c.isDark) Color.White else c.textSecondary
    else -> c.textPrimary
}

fun humanSize(bytes: Long): String = when {
    bytes >= 1L shl 30 -> "%.1f GB".format(bytes / (1L shl 30).toDouble())
    bytes >= 1L shl 20 -> "%.1f MB".format(bytes / (1L shl 20).toDouble())
    bytes >= 1L shl 10 -> "%.0f KB".format(bytes / (1L shl 10).toDouble())
    else -> "$bytes B"
}

internal fun decodeScaled(path: String, maxSide: Int): ImageBitmap? = runCatching {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeFile(path, bounds)
    var sample = 1
    while (maxOf(bounds.outWidth, bounds.outHeight) / (sample * 2) >= maxSide) sample *= 2
    BitmapFactory.decodeFile(path, BitmapFactory.Options().apply { inSampleSize = sample })?.asImageBitmap()
}.getOrNull()

