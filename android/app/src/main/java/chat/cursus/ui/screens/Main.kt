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
        Phase.Ready -> if (model.appLocked) AppLockScreen(model) else MainScreen(model)
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
            Page.ChatInfo -> model.selectedChat?.let { chat ->
                ChatInfoPage(model, chat, onClose = { page = null }) { ref ->
                    page = null
                    model.select(ref)
                }
            } ?: run { page = null }
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

    val listTone = if (model.section == Section.Settings) c.bgPanel else c.bgList
    Box(Modifier.fillMaxSize().background(c.bgApp)) {
        if (mode == LayoutMode.Desktop) {
            Row(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal + WindowInsetsSides.Top))) {
                Rail(model)
                Box(Modifier.width(320.dp).fillMaxHeight().background(listTone)) { list() }
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
                    // Linux main page: the section nav floats over the list
                    // page's bottom, with a clearance band so nothing hides
                    // under it; Settings sits on the panel tone (cards here).
                    else -> Box(Modifier.fillMaxSize().background(listTone)) {
                        Box(
                            Modifier.fillMaxSize()
                                .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.Horizontal + WindowInsetsSides.Bottom))
                                .padding(bottom = NavBarHeight + NavInset),
                        ) { list() }
                        BottomBar(model, Modifier.align(Alignment.BottomCenter))
                    }
                }
            }
        }
        SnackbarHost(snackbar, Modifier.align(Alignment.BottomCenter).padding(bottom = 96.dp))
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
            .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Bottom + WindowInsetsSides.Horizontal))
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
                    NavIcon(s, sectionUnread(model, s))
                }
            }
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
