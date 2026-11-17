package chat.cursus.ui.screens

import androidx.activity.compose.BackHandler
import androidx.compose.material3.LocalContentColor
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.clickable
import androidx.compose.foundation.border
import androidx.compose.foundation.shape.RoundedCornerShape
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
    when (val phase = model.phase) {
        Phase.Starting -> StartingScreen()
        Phase.Welcome -> WelcomeScreen(model)
        Phase.Unlock -> UnlockScreen(model)
        Phase.Ready -> if (model.appLocked) AppLockScreen(model) else MainScreen(model)
        is Phase.Failed -> FailedScreen(phase.message)
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
    androidx.compose.foundation.layout.BoxWithConstraints(
        Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing),
    ) {
        MainContent(model, twoPane = PaneLayout.isSplit(maxWidth.value), listWidth = PaneLayout.listWidth(maxWidth.value).dp)
    }
}

@Composable
private fun MainContent(model: AppModel, twoPane: Boolean, listWidth: androidx.compose.ui.unit.Dp) {
    val c = Cursus.colors
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

    BackHandler(enabled = page != null) { page = null }
    BackHandler(enabled = page == null && chatOpen) { model.select(null) }

    val list: @Composable () -> Unit = {
        when (model.section) {
            Section.Chats, Section.Channels -> ChatListPane(model, channels = model.section == Section.Channels,
                onNew = { page = Page.NewChat }, onSelect = { ref -> page = null; model.select(ref) })
            Section.Calls -> CallsPane()
            Section.Settings -> SettingsListPane(model, (page as? Page.Settings)?.section) { page = Page.Settings(it) }
        }
    }
    val pageContent: @Composable (Page) -> Unit = { p ->
        when (p) {
            Page.NewChat -> NewChatPage(model, initialLink = linkToOpen, onClose = { page = null; linkToOpen = null }) { ref ->
                page = null
                linkToOpen = null
                model.select(ref)
            }
            is Page.Settings -> SettingsDetailPane(model, p.section, onBack = { page = null })
            Page.ChatInfo -> model.selectedChat?.let { chat ->
                ChatInfoPage(model, chat, onClose = { page = null }) { ref ->
                    page = null
                    model.select(ref)
                }
            } ?: run { page = null }
        }
    }
    val conversation: @Composable () -> Unit = {
        val chat = model.selectedChat?.takeIf { it.isChannel == (model.section == Section.Channels) }
        if (chat != null) {
            ConversationPane(model, chat, showBack = true, onInfo = { page = Page.ChatInfo })
        } else {
            EmptyPane("Pick a chat to start messaging.")
        }
    }

    val listTone = if (model.section == Section.Settings) c.bgPanel else c.bgList
    val detail: @Composable () -> Unit = {
        val current = page
        androidx.compose.runtime.key(current, model.selection) {
            when {
                current != null -> pageContent(current)
                inChatSection -> conversation()
                model.section == Section.Settings -> EmptyPane("Choose a settings category.")
                else -> EmptyPane("Audio calls are coming to Cursus for Android.")
            }
        }
    }
    val sidebar: @Composable () -> Unit = {
        Column(Modifier.fillMaxSize().background(listTone)) {
            Box(Modifier.weight(1f).fillMaxWidth()) { list() }
            BottomBar(model)
        }
    }
    Box(Modifier.fillMaxSize().background(c.bgApp)) {
        if (twoPane) {
            Row(Modifier.fillMaxSize()) {
                Box(Modifier.width(listWidth).fillMaxHeight()) { sidebar() }
                VerticalDivider(color = c.border, thickness = 1.dp)
                Box(Modifier.weight(1f).fillMaxHeight().background(c.bgPanel)) { detail() }
            }
        } else if (page != null || chatOpen) {
            Box(Modifier.fillMaxSize().background(c.bgPanel)) { detail() }
        } else {
            sidebar()
        }
        SnackbarHost(snackbar, Modifier.align(Alignment.BottomCenter).padding(bottom = 80.dp))
        ImageGallery(model)
    }
    FileApprovalDialog(model)
}

private val NavBarHeight = 54.dp
private val NavInset = 12.dp
private val NavShape = RoundedCornerShape(18.dp)
private val NavItemShape = RoundedCornerShape(12.dp)

/**
 * Linux `bottom_nav` + `floating_nav_style`: a rounded elevated card inset
 * 12dp from the edges, 1px border and a soft drop shadow; each destination
 * fills an equal cell, the active one on an accent-soft rounded square.
 */
@Composable
private fun BottomBar(model: AppModel, modifier: Modifier = Modifier) {
    val c = Cursus.colors
    Row(
        modifier
            .padding(start = NavInset, end = NavInset, bottom = NavInset)
            .fillMaxWidth()
            .height(NavBarHeight)
            .shadow(
                if (c.isDark) 12.dp else 8.dp, NavShape,
                ambientColor = Color.Black, spotColor = Color.Black,
            )
            .clip(NavShape)
            .background(c.bgElevated)
            .border(1.dp, c.border, NavShape)
            .padding(horizontal = 8.dp, vertical = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
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
                    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(3.dp)) {
                        NavIcon(s, sectionUnread(model, s))
                        Text(s.title, fontSize = 10.sp, maxLines = 1)
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
    Box(Modifier.fillMaxSize().background(Cursus.colors.bgPanel), contentAlignment = Alignment.Center) {
        Text(text, style = Cursus.type.body, color = Cursus.colors.textMuted)
    }
}

@Composable
fun CallsPane() {
    Column(Modifier.fillMaxSize()) {
        PaneHeader("Calls")
        Box(Modifier.weight(1f).padding(16.dp), contentAlignment = Alignment.Center) {
            Text(
                "Audio calls are coming to Cursus for Android.",
                style = Cursus.type.body, color = Cursus.colors.textMuted,
            )
        }
    }
}
