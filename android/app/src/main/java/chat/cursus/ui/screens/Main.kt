package chat.cursus.ui.screens

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
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
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.NavigationRail
import androidx.compose.material3.NavigationRailItem
import androidx.compose.material3.NavigationRailItemDefaults
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
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import chat.cursus.AppModel
import chat.cursus.Phase
import chat.cursus.Section
import chat.cursus.ui.theme.Cursus
import chat.cursus.ui.theme.Radius

/** Layout modes on logical (dp) width — docs/DESIGN_SYSTEM.md § Layout breakpoints. */
enum class LayoutMode { Mobile, Compact, Desktop }

@Composable
fun layoutMode(): LayoutMode {
    val width = LocalConfiguration.current.screenWidthDp
    return when {
        width < 650 -> LayoutMode.Mobile
        width <= 950 -> LayoutMode.Compact
        else -> LayoutMode.Desktop
    }
}

/** Pages pushed over a section (New chat, profile, …). */
sealed interface Page {
    data object NewChat : Page
    data object ChatInfo : Page
}

@Composable
fun Root(model: AppModel) {
    when (val phase = model.phase) {
        Phase.Starting -> StartingScreen()
        Phase.Welcome -> WelcomeScreen(model)
        Phase.Unlock -> UnlockScreen(model)
        Phase.Ready -> MainScreen(model)
        is Phase.Failed -> FailedScreen(phase.message)
    }
    model.error?.let { text ->
        AlertDialog(
            onDismissRequest = { model.error = null },
            confirmButton = { TextButton({ model.error = null }) { Text("OK", color = Cursus.colors.accent) } },
            text = { Text(text, style = Cursus.type.body) },
            containerColor = Cursus.colors.bgElevated,
            shape = Radius.lg,
        )
    }
}

@Composable
fun MainScreen(model: AppModel) {
    val mode = layoutMode()
    val c = Cursus.colors
    var page by remember { mutableStateOf<Page?>(null) }
    var linkToOpen by rememberSaveable { mutableStateOf<String?>(null) }
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
    val chatOpen = inChatSection && model.selection != null

    BackHandler(enabled = page != null) { page = null }
    BackHandler(enabled = page == null && chatOpen && mode != LayoutMode.Desktop) { model.select(null) }

    val list: @Composable () -> Unit = {
        when (model.section) {
            Section.Chats, Section.Channels -> ChatListPane(model, channels = model.section == Section.Channels) { page = Page.NewChat }
            Section.Calls -> CallsPane()
            Section.Settings -> SettingsPane(model)
        }
    }
    val pageContent: @Composable (Page) -> Unit = { p ->
        when (p) {
            Page.NewChat -> NewChatPage(model, initialLink = linkToOpen, onClose = { page = null; linkToOpen = null }) { ref ->
                page = null
                linkToOpen = null
                model.select(ref)
            }
            Page.ChatInfo -> model.selectedChat?.let { ChatInfoPage(model, it) { page = null } } ?: run { page = null }
        }
    }
    val conversation: @Composable () -> Unit = {
        val chat = model.selectedChat
        if (chat != null) {
            ConversationPane(model, chat, showBack = mode != LayoutMode.Desktop, onInfo = { page = Page.ChatInfo })
        } else {
            EmptyPane("Pick a chat to start messaging.")
        }
    }

    Box(Modifier.fillMaxSize().background(c.bgApp)) {
        if (mode == LayoutMode.Desktop) {
            Row(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal + WindowInsetsSides.Top))) {
                Rail(model)
                Box(Modifier.width(320.dp).fillMaxHeight().background(c.bgList)) { list() }
                VerticalDivider(color = c.border, thickness = 1.dp)
                Box(Modifier.weight(1f).fillMaxHeight().background(c.bgPanel)) {
                    val p = page
                    when {
                        p != null -> pageContent(p)
                        inChatSection -> conversation()
                        else -> EmptyPane(" ")
                    }
                }
            }
        } else {
            val p = page
            AnimatedContent(
                targetState = Triple(p, chatOpen, model.section),
                transitionSpec = { fadeIn(tween(160)) togetherWith fadeOut(tween(120)) },
                label = "push",
            ) { (shownPage, open, _) ->
                when {
                    shownPage != null -> Box(Modifier.fillMaxSize().background(c.bgPanel).windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.Horizontal))) {
                        pageContent(shownPage)
                    }
                    open -> Box(Modifier.fillMaxSize().background(c.bgPanel).windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.Horizontal))) {
                        conversation()
                    }
                    else -> Column(Modifier.fillMaxSize()) {
                        Box(
                            Modifier.weight(1f).background(c.bgList)
                                .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.Horizontal)),
                        ) { list() }
                        BottomBar(model)
                    }
                }
            }
        }
        SnackbarHost(snackbar, Modifier.align(Alignment.BottomCenter).padding(bottom = 96.dp))
        ImageGallery(model)
    }
    FileApprovalDialog(model)
}

@Composable
private fun BottomBar(model: AppModel) {
    val c = Cursus.colors
    NavigationBar(containerColor = c.bgSidebar, tonalElevation = 0.dp) {
        for (s in Section.entries) {
            val badge = sectionUnread(model, s)
            NavigationBarItem(
                selected = model.section == s,
                onClick = { model.section = s },
                icon = { NavIcon(s, badge) },
                label = null,
                alwaysShowLabel = false,
                colors = NavigationBarItemDefaults.colors(
                    selectedIconColor = c.accent,
                    unselectedIconColor = c.textSecondary,
                    indicatorColor = c.accentSoft,
                ),
            )
        }
    }
}

@Composable
private fun Rail(model: AppModel) {
    val c = Cursus.colors
    NavigationRail(containerColor = c.bgSidebar) {
        Box(Modifier.weight(1f))
        for (s in Section.entries) {
            NavigationRailItem(
                selected = model.section == s,
                onClick = { model.section = s },
                icon = { NavIcon(s, sectionUnread(model, s)) },
                colors = NavigationRailItemDefaults.colors(
                    selectedIconColor = c.accent,
                    unselectedIconColor = c.textSecondary,
                    indicatorColor = c.accentSoft,
                ),
            )
        }
        Box(Modifier.weight(1f))
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
