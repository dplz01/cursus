package chat.cursus.ui.screens

import chat.cursus.ui.components.AttentionButton
import android.content.Intent
import android.graphics.BitmapFactory
import android.net.Uri
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
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
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.draw.shadow
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
import chat.cursus.core.Quote
import chat.cursus.ui.components.backdropSource
import chat.cursus.ui.components.rememberBackdrop
import chat.cursus.ui.components.LocalBackdrop
import chat.cursus.ui.components.glass
import chat.cursus.core.Segment
import chat.cursus.core.Stamps
import chat.cursus.ui.components.AccentSquare
import chat.cursus.ui.components.Avatar
import chat.cursus.ui.components.systemBack
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
    val callPermission = rememberCallPermission(model)
    Column(Modifier.fillMaxSize().background(c.bgPanel).windowInsetsPadding(WindowInsets.ime.union(WindowInsets.navigationBars))) {
        // The composer floats over the history (which scrolls underneath
        // it); the list keeps its height as bottom padding so the newest
        // message can still be scrolled clear of the field.
        val floating = model.selected.isEmpty() && !chat.isRequest && !(chat.isChannel && chat.role != "owner")
        var composerHeight by remember { mutableStateOf(0.dp) }
        val density = androidx.compose.ui.platform.LocalDensity.current
        // The history is the backdrop the composer's glass blurs.
        val backdrop = rememberBackdrop()
        Box(Modifier.weight(1f)) {
            // Its 8dp top inset is the whole gap above the field (Linux
            // composer_inset).
            Box(Modifier.fillMaxSize().backdropSource(backdrop, c.bgPanel)) {
                MessageList(model, chat, bottomInset = if (floating) composerHeight else 0.dp)
            }
            // The title bar floats over the top of the history like the
            // composer and the destinations bar: a glass card over the
            // blurred history (Linux CHAT_HEADER_*).
            androidx.compose.runtime.CompositionLocalProvider(LocalBackdrop provides backdrop) {
                Row(
                    Modifier.align(Alignment.TopCenter).fillMaxWidth()
                        .padding(start = 10.dp, end = 10.dp, top = HEADER_INSET)
                        .height(HEADER_HEIGHT).glass(Radius.control).padding(horizontal = 4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    if (showBack) IconTool(R.drawable.ic_back, "Back", systemBack { model.select(null) })
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
                    // Unread support messages: the yellow counter (Linux header).
                    if (chat.isGroup && !chat.isChannel && chat.supportBadge > 0) {
                        AttentionButton(R.drawable.ic_flag_outline, chat.supportBadge, if (chat.canModerate) "Chats with members" else "Chat with admins") {
                            if (chat.canModerate) onInfo() else model.select("${chat.ref}(_support)")
                        }
                    }
                    if (chat.isContact && !chat.isRequest) IconTool(R.drawable.ic_calls, "Audio call", {
                        callPermission { model.calls.start(chat.apiId, chat.title) }
                    }, enabled = model.calls.state.active == null && !model.calls.starting)
                    // No (i): tapping the name / picture opens the profile.
                }
            }
            if (floating) Box(
                Modifier.align(Alignment.BottomCenter).fillMaxWidth()
                    .onSizeChanged { composerHeight = with(density) { it.height.toDp() } },
            ) {
                androidx.compose.runtime.CompositionLocalProvider(LocalBackdrop provides backdrop) {
                    Composer(model)
                }
            }
        }

        when {
            model.selected.isNotEmpty() -> SelectionBar(model)
            chat.isRequest -> RequestBar(model, chat)
            else -> Unit
        }
    }
    androidx.activity.compose.BackHandler(model.selected.isNotEmpty()) { model.selected.clear() }
}

/** Floating title bar: height, inset from the top, and the room the
 *  history keeps above its first message (bar + inset + 8dp gap). */
private val HEADER_HEIGHT = 56.dp
private val HEADER_INSET = 8.dp
private val HEADER_ROOM = HEADER_INSET + HEADER_HEIGHT + 8.dp

/** Selection mode (message menu → Select): count, forward, delete, cancel. */
@Composable
private fun SelectionBar(model: AppModel) {
    val c = Cursus.colors
    var confirm by remember { mutableStateOf(false) }
    var forward by remember { mutableStateOf(false) }
    val count = model.selected.size
    Row(
        Modifier.fillMaxWidth().background(c.bgSidebar).padding(horizontal = 8.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        IconTool(R.drawable.ic_cancel, "Cancel selection", { model.selected.clear() })
        Text("$count selected", style = Cursus.type.body, color = c.textPrimary, modifier = Modifier.weight(1f))
        IconTool(R.drawable.ic_forward, "Forward", { forward = true })
        IconTool(R.drawable.ic_trash_delete, "Delete", { confirm = true }, tint = c.danger)
    }
    if (confirm) {
        ConfirmDialog(
            title = if (count == 1) "Delete this message?" else "Delete $count messages?",
            text = if (model.selectionIsOwn) "They are deleted for everyone in this chat (only marked deleted where Full deletion is off)."
            else "They are removed only on this device.",
            action = "Delete",
            onConfirm = { confirm = false; model.deleteSelected() },
            onDismiss = { confirm = false },
        )
    }
    if (forward) {
        chat.cursus.ui.components.CursusOverlay({ forward = false }) {
            ForwardTargets(model) { to -> forward = false; model.forwardSelected(to) }
            SecondaryButton("Cancel", { forward = false })
        }
    }
}

@Composable
private fun RequestBar(model: AppModel, chat: Chat) {
    Column(Modifier.fillMaxWidth().padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(if (chat.memberRequest) "${chat.title} requests a direct message from your group." else "${chat.title} wants to connect with you.", style = Cursus.type.body, color = Cursus.colors.textPrimary)
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            SecondaryButton("Reject", { model.rejectRequest(chat) }, Modifier.weight(1f))
            PrimaryButton("Accept", { model.acceptRequest(chat) }, Modifier.weight(1f))
        }
    }
}

/** One row of the history: a day label or a message. */
private sealed interface Row_ {
    data class Day(val date: LocalDate) : Row_
    data class Msg(val message: Message, val firstOfRun: Boolean, val lastOfRun: Boolean) : Row_
}

@Composable
private fun MessageList(model: AppModel, chat: Chat, bottomInset: androidx.compose.ui.unit.Dp = 0.dp) {
    val rows = remember(model.messages.toList()) {
        buildList<Row_> {
            var lastDay: LocalDate? = null
            for (m in model.messages) {
                val day = Stamps.day(m.createdAt)
                if (day != null && day != lastDay) {
                    add(Row_.Day(day)); lastDay = day
                }
                // Runs (same sender, same day, ≤5 min) come from the core.
                add(Row_.Msg(m, firstOfRun = !m.joinedPrev, lastOfRun = !m.joinedNext))
            }
        }.asReversed()
    }
    val state = rememberLazyListState()
    var displayedLast by androidx.compose.runtime.saveable.rememberSaveable(chat.ref) { mutableStateOf<Long?>(null) }
    // Returning from a profile is not a new outgoing message. Keep the saved
    // viewport unless the history actually advances while following the end.
    LaunchedEffect(model.messages.lastOrNull()?.id) {
        val last = model.messages.lastOrNull()
        if (displayedLast == null && model.openingReadAnchor != null) {
            val index = rows.indexOfFirst { it is Row_.Msg && it.message.id == model.openingReadAnchor }
            if (index >= 0) state.scrollToItem(index)
        } else if (last?.id != displayedLast && (state.firstVisibleItemIndex <= 2 || last?.outgoing == true)) {
            state.animateScrollToItem(0)
        }
        displayedLast = last?.id
    }
    // Tapping a reply's quote scrolls to the original and flashes it
    // (Linux `jump_to_item`).
    val scope = androidx.compose.runtime.rememberCoroutineScope()
    var focused by remember(chat.ref) { mutableStateOf<Long?>(null) }
    val jumpTo: (Long) -> Unit = { itemId ->
        val index = rows.indexOfFirst { it is Row_.Msg && it.message.id == itemId }
        if (index < 0) {
            model.notice = "Original message is no longer available."
        } else {
            focused = itemId
            scope.launch {
                state.animateScrollToItem(index)
                kotlinx.coroutines.delay(1600)
                if (focused == itemId) focused = null
            }
        }
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
        contentPadding = androidx.compose.foundation.layout.PaddingValues(top = HEADER_ROOM, bottom = if (bottomInset > 0.dp) bottomInset else 8.dp),
    ) {
        items(rows.size, key = { i ->
            when (val r = rows[i]) {
                is Row_.Day -> "d" + r.date
                is Row_.Msg -> "m" + r.message.id
            }
        }) { i ->
            when (val r = rows[i]) {
                is Row_.Day -> DayLabel(r.date)
                is Row_.Msg -> MessageRow(model, chat, r.message, r.firstOfRun, r.lastOfRun,
                    focused = r.message.id == focused, onQuote = jumpTo)
            }
        }
        // Reversed list: the last item is the top of the history.
        if (model.hasEarlierMessages) {
            item(key = "earlier") {
                Box(Modifier.fillMaxWidth().padding(vertical = 6.dp), contentAlignment = Alignment.Center) {
                    SecondaryButton("Load earlier messages", { model.loadEarlierMessages() })
                }
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
private fun MessageRow(
    model: AppModel, chat: Chat, m: Message, firstOfRun: Boolean, lastOfRun: Boolean,
    focused: Boolean = false, onQuote: (Long) -> Unit = {},
) {
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
    val visualMedia = m.deleted == null && m.file?.kind in setOf("image", "video")
    val shape = bubbleShape(m.outgoing, lastOfRun, visualMedia)
    // Selection mode: a tap anywhere on the row adds / removes the message.
    val selecting = model.selected.isNotEmpty()
    val onLongPress = { if (selecting) model.toggleSelected(m) else menu = true }
    Row(
        Modifier.fillMaxWidth()
            .background(if (m.id in model.selected || focused) c.accentSoft else Color.Transparent)
            .then(if (selecting) Modifier.clickable { model.toggleSelected(m) } else Modifier)
            .padding(start = 8.dp, end = 8.dp, top = if (firstOfRun) 6.dp else 2.dp),
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
                    Modifier.width(IntrinsicSize.Max)
                        // Light mode: a whisper of shadow lifts white bubbles off the grey (Linux).
                        .then(if (c.isDark) Modifier else Modifier.shadow(1.dp, shape))
                        .clip(shape)
                        .background(if (m.outgoing) c.bubbleOut else c.bubbleIn)
                        .combinedClickable(
                            onClick = { if (selecting) model.toggleSelected(m) },
                            onLongClick = onLongPress,
                        )
                        .padding(horizontal = if (visualMedia) 4.dp else 12.dp,
                            vertical = if (visualMedia) 4.dp else 8.dp),
                ) {
                    if (groupIncoming && firstOfRun) {
                        Text(
                            m.sender?.name ?: "",
                            // A clear gap between the name and the message (Linux: 4).
                            modifier = if (visualMedia) Modifier.padding(horizontal = 8.dp, vertical = 4.dp) else Modifier.padding(bottom = 4.dp),
                            style = Cursus.type.meta,
                            color = c.accent,
                            fontWeight = FontWeight.SemiBold,
                            maxLines = 1,
                        )
                    }
                    m.quote?.let { q ->
                        QuotePreview(
                            q,
                            Modifier.padding(horizontal = if (visualMedia) 8.dp else 0.dp).padding(bottom = 6.dp),
                            onClick = q.itemId?.let { id -> { if (selecting) model.toggleSelected(m) else onQuote(id) } },
                        )
                    }
                    val imageStamp = visualMedia && m.file?.state == "complete"
                    val inlineStamp = !imageStamp && m.deleted == null && m.groupInvitation == null &&
                        (if (m.file == null) m.text.isNotEmpty() else m.caption.isNotBlank())
                    MessageBody(model, m, onLongPress, inlineStamp, showReceipt = !chat.isNotes)
                    if (!inlineStamp && !imageStamp) {
                        MessageStamp(m, showReceipt = !chat.isNotes,
                            modifier = Modifier.align(Alignment.End).padding(top = 2.dp))
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

private val RoundedChip = chat.cursus.ui.theme.Radius.control

/** Linux `quote_preview`: an inset card in the bubble — rounded accent bar,
 *  "You" / "Reply", up to three lines of the original. Tapping it jumps to
 *  the original message. */
@Composable
private fun QuotePreview(q: Quote, modifier: Modifier, onClick: (() -> Unit)?) {
    val c = Cursus.colors
    Row(
        modifier.fillMaxWidth().height(IntrinsicSize.Min)
            .clip(QuoteShape)
            .background(Color.Black.copy(alpha = 0.22f))
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = 8.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Box(Modifier.width(3.dp).fillMaxHeight().heightIn(min = 28.dp).clip(QuoteBarShape).background(c.accent))
        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(if (q.outgoing) "You" else "Reply", style = Cursus.type.meta, color = c.accent, maxLines = 1)
            Text(
                q.text,
                style = Cursus.type.meta,
                color = c.textSecondary,
                maxLines = 3,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

private val QuoteShape = androidx.compose.foundation.shape.RoundedCornerShape(10.dp)
private val QuoteBarShape = androidx.compose.foundation.shape.RoundedCornerShape(2.dp)

/**
 * Bubbles round off at 16 except on the sender's side (right for sent, left
 * for received): the top corner there is always small, and the bottom one
 * too on every bubble of a run but its last, so the run reads as one block.
 */
private fun bubbleShape(outgoing: Boolean, lastOfRun: Boolean, media: Boolean = false): androidx.compose.foundation.shape.RoundedCornerShape {
    // Same numbers as Linux / Mac: 18 (16 around flush media), joins at 6.
    val r = if (media) 16.dp else 18.dp
    val top = 6.dp
    val bottom = if (lastOfRun) r else 6.dp
    return androidx.compose.foundation.shape.RoundedCornerShape(
        topStart = if (outgoing) r else top,
        topEnd = if (outgoing) top else r,
        bottomStart = if (outgoing) r else bottom,
        bottomEnd = if (outgoing) bottom else r,
    )
}

@Composable
internal fun DeliveryGlyph(delivery: String, c: CursusColors) {
    val (icon, tint) = when (delivery) {
        "pending" -> R.drawable.ic_history to c.textMuted
        "delivered" -> R.drawable.ic_check_read to c.bubbleOutTick
        "error" -> R.drawable.ic_block_prohibited to c.danger
        "warning" -> R.drawable.ic_history to c.gold
        else -> R.drawable.ic_check to c.bubbleOutMeta
    }
    Icon(painterResource(icon), delivery, Modifier.size(14.dp), tint = tint)
}

@Composable
private fun MessageBody(model: AppModel, m: Message, onLongPress: () -> Unit, inlineStamp: Boolean, showReceipt: Boolean) {
    val c = Cursus.colors
    if (m.deleted != null) {
        // Core's wording; a stored text (marked deleted, hidden member) stays behind "Reveal".
        val revealed = m.revealable && m.id in model.revealedDeleted
        Text(m.deletedLabel ?: if (m.deleted == "marked") "marked deleted" else "deleted", style = Cursus.type.body, color = c.textMuted, fontStyle = FontStyle.Italic)
        if (!revealed) return
    }
    m.groupInvitation?.let { g ->
        Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text("Invitation to ${g.name}", style = Cursus.type.body, color = c.textPrimary, fontWeight = FontWeight.Medium)
            if (g.pending && !m.outgoing) PrimaryButton("Join group", { model.joinGroup(g.groupId) })
        }
        return
    }
    m.file?.let { f ->
        FileBody(model, m, f, onLongPress, showReceipt)
        if (m.caption.isNotBlank()) {
            Box(if (f.kind in setOf("image", "video")) Modifier.padding(horizontal = 8.dp, vertical = 4.dp) else Modifier) {
                RichText(model, m.segments.ifEmpty { listOf(Segment(m.caption)) }, m.emojiOnly,
                    stamp = m.takeIf { inlineStamp }, showReceipt = showReceipt)
            }
        }
        return
    }
    RichText(model, m.segments.ifEmpty { listOf(Segment(m.text)) }, m.emojiOnly,
        stamp = m.takeIf { inlineStamp }, showReceipt = showReceipt)
}

/** SimpleX markdown segments (parsed by the core) → styled text with tappable links. */
@Composable
private fun RichText(model: AppModel, segments: List<Segment>, emojiOnly: Boolean, stamp: Message?, showReceipt: Boolean) {
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
                "snippet" -> SpanStyle(fontFamily = FontFamily.Monospace, background = c.bgField)
                "small" -> SpanStyle(fontSize = 13.sp, color = c.textSecondary)
                "colored" -> SpanStyle(color = segmentColor(s.color, c))
                "mention" -> SpanStyle(color = c.accent, fontWeight = FontWeight.Medium)
                "secret" -> if (i in revealed) SpanStyle(background = c.bgField) else SpanStyle(color = Color.Transparent, background = c.textMuted)
                else -> SpanStyle()
            }
            val link = s.link
            when {
                // SimpleX links also come with an italic "(via …)" part: same link.
                link != null && (s.style in setOf("uri", "email", "phone") || s.simplex != null) -> withLink(
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
                ) { withStyle(style.merge(SpanStyle(color = c.accent, textDecoration = TextDecoration.Underline))) { append(s.text) } }
                s.style == "secret" -> withLink(
                    LinkAnnotation.Clickable("s$i") { revealed = if (i in revealed) revealed - i else revealed + i },
                ) { withStyle(style) { append(s.text) } }
                else -> withStyle(style) { append(s.text) }
            }
        }
    }
    val textStyle = if (emojiOnly) Cursus.type.body.copy(fontSize = 32.sp, lineHeight = 38.sp) else Cursus.type.body
    if (stamp != null) {
        StampedMessageText(text, textStyle, stamp, showReceipt)
    } else {
        Text(text, style = textStyle, color = c.textPrimary)
    }
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

