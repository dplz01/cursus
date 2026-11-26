package chat.cursus.ui.components

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import chat.cursus.ui.theme.Cursus

private val MenuCardShape = chat.cursus.ui.theme.Radius.control

/**
 * Linux overlay: 45% black backdrop (tap closes) with a centred menu card
 * (`menu_card`: `bg.panel`, 1px border, shared radius, max 400 wide).
 */
@Composable
fun CursusOverlay(
    onDismiss: () -> Unit,
    padding: androidx.compose.ui.unit.Dp = 16.dp,
    spacing: androidx.compose.ui.unit.Dp = 12.dp,
    content: @Composable ColumnScope.() -> Unit,
) {
    val c = Cursus.colors
    Dialog(onDismissRequest = onDismiss, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        BackHandler(onBack = onDismiss)
        DialogEscape(onDismiss)
        Box(
            Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.45f))
                .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onDismiss)
                .windowInsetsPadding(WindowInsets.safeDrawing)
                .imePadding()
                .padding(horizontal = 20.dp, vertical = 24.dp),
            contentAlignment = Alignment.Center,
        ) {
            Column(
                Modifier.widthIn(max = 400.dp).fillMaxWidth().clip(MenuCardShape).background(c.bgPanel)
                    .border(1.dp, c.border, MenuCardShape)
                    .clickable(remember { MutableInteractionSource() }, indication = null) {}
                    .verticalScroll(rememberScrollState())
                    .padding(padding),
                verticalArrangement = Arrangement.spacedBy(spacing),
                content = content,
            )
        }
    }
}

/** Dialogs have their own Android window, so Escape must be handled there too. */
@Composable
fun DialogEscape(onBack: () -> Unit) {
    val view = androidx.compose.ui.platform.LocalView.current
    val currentBack by androidx.compose.runtime.rememberUpdatedState(onBack)
    androidx.compose.runtime.DisposableEffect(view) {
        val window = (view.parent as? androidx.compose.ui.window.DialogWindowProvider)?.window
        val previous = window?.callback
        if (window == null || previous == null) return@DisposableEffect onDispose {}
        val dispatcher = (previous as? androidx.activity.OnBackPressedDispatcherOwner)?.onBackPressedDispatcher
        val back = object : androidx.activity.OnBackPressedCallback(true) {
            override fun handleOnBackPressed() = currentBack()
        }
        dispatcher?.addCallback(back)
        val callback = object : android.view.Window.Callback by previous {
            override fun dispatchKeyEvent(event: android.view.KeyEvent): Boolean {
                if (event.keyCode != android.view.KeyEvent.KEYCODE_ESCAPE) return previous.dispatchKeyEvent(event)
                if (event.action == android.view.KeyEvent.ACTION_UP) {
                    if (dispatcher != null) dispatcher.onBackPressed()
                    else previous.dispatchKeyEvent(android.view.KeyEvent(android.view.KeyEvent.ACTION_UP, android.view.KeyEvent.KEYCODE_BACK))
                }
                return true
            }
        }
        window.callback = callback
        onDispose { back.remove(); if (window.callback === callback) window.callback = previous }
    }
}
