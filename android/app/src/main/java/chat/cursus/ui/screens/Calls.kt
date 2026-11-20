package chat.cursus.ui.screens

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import chat.cursus.AppModel
import chat.cursus.R
import chat.cursus.ui.components.*
import chat.cursus.ui.theme.Cursus
import chat.cursus.ui.theme.Radius

@Composable
fun rememberCallPermission(model: AppModel): (() -> Unit) -> Unit {
    val context = LocalContext.current
    var pending by remember { mutableStateOf<(() -> Unit)?>(null) }
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { allowed ->
        val action = pending; pending = null
        if (allowed) action?.invoke() else model.error = "Allow microphone access to make or answer audio calls."
    }
    return { action ->
        if (ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) action()
        else { pending = action; launcher.launch(Manifest.permission.RECORD_AUDIO) }
    }
}

/** Back minimizes the call; only the explicit hang-up button ends it. */
@Composable
fun CallSurface(model: AppModel) {
    val calls = model.calls
    val permission = rememberCallPermission(model)
    LaunchedEffect(calls.answerRequested, calls.state.active?.phase) {
        if (calls.answerRequested && calls.state.active?.phase == "incoming") {
            calls.answerRequested = false
            permission { calls.accept() }
        }
    }
    if (calls.expanded && (calls.state.active != null || calls.starting)) {
        CursusOverlay({ calls.expanded = false }) { CallControls(model) }
    }
}

@Composable
fun CallBanner(model: AppModel) {
    val call = model.calls.state.active ?: return
    val c = Cursus.colors
    Row(Modifier.fillMaxWidth().background(c.accentSoft).clickable { model.calls.expanded = true }
        .padding(horizontal = 16.dp, vertical = 9.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Text(if (call.phase == "incoming") "Incoming audio call" else "Audio call", style = Cursus.type.meta, color = c.accent)
        Text(call.name, Modifier.weight(1f), style = Cursus.type.body, color = c.textPrimary, maxLines = 1)
        Text(callLabel(call.phase, call.durationSecs), style = Cursus.type.meta, color = c.accent)
    }
}

internal fun callLabel(phase: String, seconds: Long): String = when (phase) {
    "incoming" -> "Ringing…"
    "ringing" -> "Calling…"
    "connected" -> "%d:%02d".format(java.util.Locale.ROOT, seconds / 60, seconds % 60)
    else -> "Connecting…"
}

@Composable
fun CallControls(model: AppModel) {
    val c = Cursus.colors
    val calls = model.calls
    val call = calls.state.active
    val permission = rememberCallPermission(model)
    val contact = model.chats.firstOrNull { it.isContact && it.apiId == call?.contactId }
    Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(vertical = 16.dp), horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Avatar(call?.name ?: "Audio call", contact?.image, 84.dp)
        Text(call?.name ?: "Audio call", style = Cursus.type.title, color = c.textPrimary,
            fontWeight = FontWeight.SemiBold, textAlign = TextAlign.Center, maxLines = 2, overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis)
        Text(callLabel(call?.phase ?: "connecting", call?.durationSecs ?: 0), style = Cursus.type.body, color = c.textSecondary)
        if (call != null && call.phase !in listOf("ringing", "incoming")) {
            Text(if (call.encrypted) "End-to-end encrypted audio" else "Secure audio connection",
                style = Cursus.type.meta, color = c.textMuted)
        }
        if (call?.phase == "incoming") {
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                SecondaryButton("Decline", { calls.end(reject = true) }, Modifier.weight(1f))
                PrimaryButton("Answer", { permission { calls.accept() } }, Modifier.weight(1f), enabled = !calls.starting)
            }
        } else {
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                SecondaryButton(if (call?.muted == true) "Unmute" else "Mute", { calls.mute() }, Modifier.weight(1f), enabled = call != null)
                SecondaryButton(if (calls.speaker) "Speaker on" else "Speaker", { calls.toggleSpeaker() }, Modifier.weight(1f),
                    enabled = call?.phase == "connecting" || call?.phase == "connected")
            }
            DangerButton("Hang up", { calls.end() }, Modifier.fillMaxWidth())
        }
        Text("Back returns to your chats. The call stays active.", style = Cursus.type.meta,
            color = c.textMuted, textAlign = TextAlign.Center)
    }
}

@Composable
fun CallsDetail(model: AppModel) {
    if (model.calls.state.active != null) Column(Modifier.fillMaxSize().padding(24.dp),
        verticalArrangement = Arrangement.Center) { CallControls(model) }
    else EmptyPane("Choose a contact to start an audio call.")
}

@Composable
fun NewCallButton(model: AppModel) {
    var choosing by remember { mutableStateOf(false) }
    var search by remember { mutableStateOf("") }
    val permission = rememberCallPermission(model)
    HeaderPlus("New audio call") { if (model.calls.state.active == null && !model.calls.starting) { search = ""; choosing = true } }
    if (choosing) CursusOverlay({ choosing = false }) {
        val c = Cursus.colors
        Text("New audio call", style = Cursus.type.title, color = c.textPrimary)
        CursusField(search, { search = it }, "Search contacts")
        val contacts = model.chats.filter { it.isContact && !it.isRequest && it.title.contains(search, ignoreCase = true) }.sortedBy { it.title.lowercase() }
        if (contacts.isEmpty()) Text("Add a contact to start a call.", color = c.textSecondary, style = Cursus.type.body)
        LazyColumn(Modifier.heightIn(max = 360.dp)) {
            items(contacts, key = { it.ref }) { contact ->
                Row(Modifier.fillMaxWidth().clip(Radius.lg).clickable {
                    choosing = false; permission { model.calls.start(contact.apiId, contact.title) }
                }.padding(12.dp), verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Avatar(contact.title, contact.image, 40.dp)
                    Text(contact.title, style = Cursus.type.body, color = c.textPrimary)
                }
            }
        }
    }
}
