package chat.cursus.ui.screens

import androidx.compose.foundation.layout.PaddingValues
import androidx.activity.compose.BackHandler
import androidx.compose.material3.LocalContentColor
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.animation.togetherWith
import androidx.compose.ui.graphics.Color
import chat.cursus.ui.components.backdropSource
import chat.cursus.ui.components.glass
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.clickable
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import chat.cursus.AppModel
import chat.cursus.Phase
import chat.cursus.Section
import chat.cursus.ui.theme.Cursus
import chat.cursus.ui.theme.Radius

/** Pages pushed over a section (New chat, profile, …). */
sealed interface Page {
    data object NewChat : Page
    data object ChatInfo : Page
    data class Settings(val section: SettingsPage) : Page
}

@Composable
fun Root(model: AppModel) {
    // The one base fill of the window (the window's own background is off,
    // see MainActivity); pages paint over it only where they differ.
    Box(Modifier.fillMaxSize().background(Cursus.colors.bgApp)) {
        when (val phase = model.phase) {
            Phase.Starting -> StartingScreen()
            Phase.Welcome -> WelcomeScreen(model)
            Phase.Unlock -> UnlockScreen(model)
            Phase.Ready -> if (model.appLocked) AppLockScreen(model) else MainScreen(model)
            is Phase.Failed -> FailedScreen(phase.message)
        }
        if (model.phase == Phase.Ready && !model.appLocked) CallSurface(model)
    }
    model.error?.let { text ->
        AlertDialog(
            onDismissRequest = { model.error = null },
            confirmButton = { TextButton({ model.error = null }) { Text("OK", color = Cursus.colors.accent) } },
            text = {
                chat.cursus.ui.components.DialogEscape { model.error = null }
                Text(text, style = Cursus.type.body)
            },
            containerColor = Cursus.colors.bgElevated,
            shape = Radius.lg,
        )
    }
}

@Composable
fun MainScreen(model: AppModel) {
    Column(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
        CallBanner(model)
        androidx.compose.foundation.layout.BoxWithConstraints(Modifier.weight(1f).fillMaxWidth()) {
            MainContent(model, twoPane = PaneLayout.isSplit(maxWidth.value), listWidth = PaneLayout.listWidth(maxWidth.value).dp)
        }
    }
}

@Composable
private fun MainContent(model: AppModel, twoPane: Boolean, listWidth: androidx.compose.ui.unit.Dp) {
    val c = Cursus.colors
    var newChatOverlay by remember(model.section) { mutableStateOf(false) }
    var page by remember(model.section) { mutableStateOf<Page?>(null) }
    var linkToOpen by rememberSaveable(model.section) { mutableStateOf<String?>(null) }
    val snackbar = remember { SnackbarHostState() }

    LaunchedEffect(model.notice) {
        model.notice?.let {
            snackbar.showSnackbar(it)
            model.notice = null
        }
    }
    // Opened from a notification or a simplex: link.
    LaunchedEffect(model.pendingChat, model.chats.size) {
        val ref = model.pendingChat ?: return@LaunchedEffect
        val chat = model.chats.firstOrNull { it.ref == ref } ?: return@LaunchedEffect
        model.section = if (chat.isChannel) Section.Channels else Section.Chats
        model.select(ref)
        model.pendingChat = null
    }
    LaunchedEffect(model.pendingLink) {
        model.pendingLink?.let {
            linkToOpen = it
            page = Page.NewChat
            model.pendingLink = null
        }
    }

    val inChatSection = model.section == Section.Chats || model.section == Section.Channels
    val chatOpen = inChatSection && model.selectedChat?.isChannel == (model.section == Section.Channels)

    // Two panes: Back closes the page / chat at once. Phone: the page stack
    // below handles Back (with Android's predictive back).
    BackHandler(enabled = twoPane && page != null) { page = null }
    BackHandler(enabled = twoPane && page == null && chatOpen) { model.select(null) }

    // One section's list; `s` is passed in so a list sliding away keeps
    // showing its own section.
    val list: @Composable (Section) -> Unit = { s ->
        when (s) {
            Section.Chats, Section.Channels -> ChatListPane(model, channels = s == Section.Channels,
                onNew = { newChatOverlay = true }, onSelect = { ref -> page = null; model.open(ref) })
            Section.Calls -> CallsPane(model)
            Section.Settings -> SettingsListPane(model, (page as? Page.Settings)?.section) { page = Page.Settings(it) }
        }
    }
    // `back` closes the page (animated on a phone).
    val pageContent: @Composable (Page, () -> Unit) -> Unit = { p, back ->
        when (p) {
            Page.NewChat -> NewChatPage(model, initialLink = linkToOpen, onClose = { back(); linkToOpen = null }) { ref ->
                page = null
                linkToOpen = null
                model.select(ref)
            }
            is Page.Settings -> SettingsDetailPane(model, p.section, onBack = back)
            Page.ChatInfo -> model.selectedChat?.let { chat ->
                ChatInfoPage(model, chat, onClose = back) { ref ->
                    page = null
                    model.select(ref)
                }
            } ?: run { page = null }
        }
    }
    // `back` leaves the chat (animated on a phone).
    val conversation: @Composable (back: () -> Unit) -> Unit = { back ->
        val chat = model.selectedChat?.takeIf { it.isChannel == (model.section == Section.Channels) }
        if (chat != null) {
            ConversationPane(model, chat, showBack = true, onBack = back, onInfo = { page = Page.ChatInfo })
        } else {
            EmptyPane("Pick a chat to start messaging.")
        }
    }

    val listTone = if (model.section == Section.Settings) c.bgPanel else c.bgList
    val detail: @Composable () -> Unit = {
        val current = page
        androidx.compose.runtime.key(model.selection) {
            // Preserve scroll while visiting this chat's profile, but create
            // fresh state (latest message) when a different chat is opened.
            val chatState = androidx.compose.runtime.saveable.rememberSaveableStateHolder()
            when {
                current != null -> androidx.compose.runtime.key(current) { pageContent(current) { page = null } }
                inChatSection -> chatState.SaveableStateProvider("conversation") { conversation { model.select(null) } }
                model.section == Section.Settings -> EmptyPane("Choose a settings category.")
                else -> CallsDetail(model)
            }
        }
    }
    // The destinations bar floats over the list (as on Linux): the list
    // scrolls underneath it and keeps room at its end so nothing hides.
    val sidebar: @Composable () -> Unit = {
        val labelGrowth = 14.dp * (LocalDensity.current.fontScale - 1f).coerceAtLeast(0f)
        // The list is the backdrop the bar's glass blurs.
        val backdrop = chat.cursus.ui.components.rememberBackdrop()
        Box(Modifier.fillMaxSize().background(listTone)) {
            Box(Modifier.fillMaxSize().backdropSource(backdrop, listTone)) {
                CompositionLocalProvider(LocalNavClearance provides NavBarHeight + labelGrowth + NavInset + 12.dp) {
                    // Changing destination slides the lists sideways, in the
                    // bar's order (Chats → Channels → Calls → Settings); the
                    // bar itself stays put.
                    androidx.compose.animation.AnimatedContent(
                        targetState = model.section,
                        transitionSpec = {
                            val dir = if (targetState.ordinal > initialState.ordinal) 1 else -1
                            val spec = androidx.compose.animation.core.tween<androidx.compose.ui.unit.IntOffset>(
                                300, easing = androidx.compose.animation.core.FastOutSlowInEasing,
                            )
                            androidx.compose.animation.slideInHorizontally(spec) { dir * it } togetherWith
                                androidx.compose.animation.slideOutHorizontally(spec) { -dir * it }
                        },
                        label = "section",
                    ) { s ->
                        Box(Modifier.fillMaxSize().background(if (s == Section.Settings) c.bgPanel else c.bgList)) { list(s) }
                    }
                }
            }
            CompositionLocalProvider(chat.cursus.ui.components.LocalBackdrop provides backdrop) {
                BottomBar(model, Modifier.align(Alignment.BottomCenter))
            }
        }
    }
    Box(Modifier.fillMaxSize()) {
        if (twoPane) {
            Row(Modifier.fillMaxSize()) {
                Box(Modifier.width(listWidth).fillMaxHeight()) { sidebar() }
                VerticalDivider(color = c.border, thickness = 1.dp)
                Box(Modifier.weight(1f).fillMaxHeight().background(c.bgPanel)) { detail() }
            }
        } else {
            // Phone: list → chat / settings page → profile, each page over
            // the one before it (Android page transitions, predictive back).
            CompositionLocalProvider(chat.cursus.ui.components.LocalPhonePages provides true) {
                val current = page
                val chatLevel = chatOpen && (current == null || current == Page.ChatInfo)
                chat.cursus.ui.components.StackedPage(
                    open = current != null || chatOpen,
                    onBack = { if (chatLevel) { page = null; model.select(null) } else page = null },
                    below = { sidebar() },
                ) { back ->
                    Box(Modifier.fillMaxSize().background(c.bgPanel)) {
                        if (chatLevel) {
                            androidx.compose.runtime.key(model.selection) {
                                chat.cursus.ui.components.StackedPage(
                                    open = current == Page.ChatInfo,
                                    onBack = { page = null },
                                    below = { conversation(back) },
                                ) { closeInfo ->
                                    Box(Modifier.fillMaxSize().background(c.bgPanel)) { pageContent(Page.ChatInfo, closeInfo) }
                                }
                            }
                        } else if (current != null) {
                            androidx.compose.runtime.key(current) { pageContent(current, back) }
                        }
                    }
                }
            }
        }
        SnackbarHost(snackbar, Modifier.align(Alignment.BottomCenter).padding(bottom = 80.dp))
        ImageGallery(model)
    }
    if (newChatOverlay) chat.cursus.ui.components.CursusOverlay({ newChatOverlay = false }) {
        Box(Modifier.heightIn(max = 560.dp)) {
            NewChatPage(model, initialLink = null, onClose = { newChatOverlay = false }) { ref ->
                newChatOverlay = false; page = null; model.select(ref)
            }
        }
    }
    FileApprovalDialog(model)
}

/** Room a list keeps at its end for the floating destinations bar. */
val LocalNavClearance = androidx.compose.runtime.compositionLocalOf { 0.dp }

private val NavBarHeight = 64.dp
private val NavInset = 12.dp
private val NavShape = chat.cursus.ui.theme.Radius.control
private val NavItemShape = chat.cursus.ui.theme.Radius.dockSelection

/**
 * Linux `bottom_nav` + `floating_nav_style`: the composer's glass card
 * (the list blurred behind it, a translucent tint, a light edge) inset
 * 12dp from the edges, with nothing drawn around it; each destination
 * fills an equal cell, the active one on an accent-soft rounded square.
 */
@Composable
private fun BottomBar(model: AppModel, modifier: Modifier = Modifier) {
    val c = Cursus.colors
    val labelStyle = TextStyle(fontSize = 11.sp, lineHeight = 14.sp, fontWeight = FontWeight.Medium)
    val labelGrowth = 14.dp * (LocalDensity.current.fontScale - 1f).coerceAtLeast(0f)
    Row(
        modifier
            .fillMaxWidth()
            .padding(start = NavInset, end = NavInset, bottom = NavInset)
            .height(NavBarHeight + labelGrowth)
            .glass(NavShape)
            .padding(5.dp),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        for (s in Section.entries) {
            val active = model.section == s
            Box(
                Modifier.weight(1f).fillMaxHeight().clip(NavItemShape)
                    .background(if (active) c.accentSoft else Color.Transparent)
                    .clickable { model.section = s },
                contentAlignment = Alignment.Center,
            ) {
                CompositionLocalProvider(LocalContentColor provides if (active) c.accent else c.textSecondary) {
                    Column(
                        modifier = Modifier.padding(vertical = 6.dp, horizontal = 2.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        NavIcon(s, sectionUnread(model, s))
                        Text(s.title, style = labelStyle, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                }
            }
        }
    }
}

private fun sectionUnread(model: AppModel, s: Section): Boolean = when (s) {
    Section.Chats -> model.chats.any { !it.isChannel && !it.muted && (it.unread > 0 || it.isRequest) }
    Section.Channels -> model.chats.any { it.isChannel && !it.muted && it.unread > 0 }
    else -> false
}

@Composable
private fun NavIcon(s: Section, dot: Boolean) {
    Box {
        Icon(painterResource(s.icon), s.title, Modifier.size(22.dp))
        if (dot) {
            Box(
                Modifier.align(Alignment.TopEnd).padding(start = 2.dp).size(8.dp)
                    .background(Cursus.colors.accent, Radius.pill),
            )
        }
    }
}

@Composable
fun EmptyPane(text: String) {
    // No fill: it sits on its pane's own background.
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Text(text, style = Cursus.type.body, color = Cursus.colors.textMuted)
    }
}

@Composable
fun CallsPane(model: AppModel) {
    val permission = rememberCallPermission(model)
    val calls = model.calls.history
    var failure by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(model.calls.historyRevision) {
        failure = null
        try {
            model.calls.loadHistory()
        } catch (e: kotlinx.coroutines.CancellationException) {
            throw e
        } catch (e: Exception) {
            failure = e.message ?: "Could not load calls."
        }
    }
    val c = Cursus.colors
    val contactsById = remember(model.chats.toList()) {
        model.chats.filter { it.isContact }.associateBy { it.apiId }
    }
    Column(Modifier.fillMaxSize()) {
        PaneHeader("Calls", actions = { NewCallButton(model) })
        when {
            calls == null && failure != null -> EmptyPane(failure.orEmpty())
            calls == null -> EmptyPane("Loading calls…")
            calls.isEmpty() -> EmptyPane("Your recent calls will appear here.")
            else -> androidx.compose.foundation.lazy.LazyColumn(Modifier.weight(1f),
                contentPadding = PaddingValues(bottom = LocalNavClearance.current)) {
                items(calls.size, key = { calls[it].itemId }) { index ->
                    val call = calls[index]
                    val contact = contactsById[call.contactId]
                    val icon = when {
                        call.missed -> chat.cursus.R.drawable.ic_call_missed
                        call.status in setOf("rejected", "error") -> chat.cursus.R.drawable.ic_call_rejected
                        call.status in setOf("accepted", "negotiated", "progress") -> chat.cursus.R.drawable.ic_call_active
                        call.outgoing -> chat.cursus.R.drawable.ic_call_outgoing
                        else -> chat.cursus.R.drawable.ic_call_incoming
                    }
                    val tint = when { call.missed || call.status in setOf("rejected", "error") -> c.danger; call.outgoing -> c.accent; else -> c.success }
                    Row(Modifier.fillMaxWidth().clickable(enabled = contact != null) {
                        model.section = Section.Chats; model.select(contact?.ref)
                    }.padding(horizontal = 16.dp, vertical = 12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        chat.cursus.ui.components.Avatar(contact?.title ?: call.peerName, contact?.image, 40.dp)
                        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                            Text(contact?.title ?: call.peerName, style = Cursus.type.body, fontWeight = androidx.compose.ui.text.font.FontWeight.SemiBold, color = if (call.missed) c.danger else c.textPrimary, maxLines = 1, overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis)
                            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                Icon(painterResource(icon), null, Modifier.size(14.dp), tint = tint)
                                val label = when { call.missed -> "Missed"; call.status == "rejected" -> "Declined"; call.status == "error" -> "Failed"; call.outgoing -> "Outgoing"; else -> "Incoming" }
                                Text(if (call.durationSecs > 0) "$label · ${formatDuration(call.durationSecs)}" else label, style = Cursus.type.meta, color = tint)
                            }
                        }
                        Text(chat.cursus.core.Stamps.list(call.ts), style = Cursus.type.meta, color = c.textMuted)
                        if (contact != null) chat.cursus.ui.components.IconTool(chat.cursus.R.drawable.ic_calls, "Call ${contact.title}", {
                            permission { model.calls.start(contact.apiId, contact.title) }
                        }, enabled = model.calls.state.active == null && !model.calls.starting)
                    }
                }
            }
        }
    }
}
