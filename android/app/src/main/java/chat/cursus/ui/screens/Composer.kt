package chat.cursus.ui.screens

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import chat.cursus.AppModel
import chat.cursus.ComposeMode
import chat.cursus.R
import chat.cursus.ui.components.IconTool
import chat.cursus.ui.theme.Cursus
import chat.cursus.ui.theme.Radius

private val SEND = 36.dp
private val PillShape = RoundedCornerShape(20.dp)

/** Linux `composer_send_hit`: round accent face when active, bare glyph otherwise. */
@Composable
private fun SendHit(active: Boolean, description: String, onClick: () -> Unit) {
    val c = Cursus.colors
    Box(
        Modifier.size(SEND).clip(Radius.pill).background(if (active) c.accent else c.bgPanel)
            .clickable(enabled = active, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(painterResource(R.drawable.ic_send), description, Modifier.size(20.dp), tint = if (active) c.accentOn else c.textSecondary)
    }
}

@Composable
fun Composer(model: AppModel) {
    val c = Cursus.colors
    val context = LocalContext.current
    val pick = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        uri?.let { model.sendFile(it) }
    }
    val askMic = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        if (granted) model.startRecording() else model.notice = "Cursus needs the microphone for voice messages"
    }
    val startMic = {
        if (ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) {
            model.startRecording()
        } else {
            askMic.launch(Manifest.permission.RECORD_AUDIO)
        }
    }

    model.voiceDraft?.let { draft ->
        VoiceDraftBar(model, draft.durationSecs.toLong(), draft.peaks)
        return
    }

    Column(Modifier.fillMaxWidth().background(c.bgPanel)) {
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
        ContextBar(model)
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 6.dp),
            verticalAlignment = Alignment.Bottom,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            if (model.recording) {
                IconTool(R.drawable.ic_cancel, "Discard recording", { model.discardRecording() })
            } else {
                IconTool(R.drawable.ic_plus, "Attach", { pick.launch(arrayOf("*/*")) })
            }
            // Rounded pill holds the text (Linux: radius 20, 1px border, field fill).
            Box(
                Modifier.weight(1f).heightIn(min = SEND).clip(PillShape).background(c.bgSidebar)
                    .border(1.dp, c.border, PillShape).padding(horizontal = 14.dp, vertical = 8.dp),
                contentAlignment = Alignment.CenterStart,
            ) {
                if (model.recording) {
                    Text("Recording… ${formatDuration(model.recordSecs.toLong())}", style = Cursus.type.body, color = c.danger)
                } else if (model.voicePreparing) {
                    Text("Preparing voice…", style = Cursus.type.body, color = c.textSecondary)
                } else {
                    if (model.draft.isEmpty()) Text("Message", style = Cursus.type.body, color = c.textSecondary)
                    BasicTextField(
                        value = model.draft,
                        onValueChange = { model.draft = it },
                        textStyle = Cursus.type.body.copy(color = c.textPrimary),
                        cursorBrush = SolidColor(c.accent),
                        maxLines = 6,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
            }
            // One primary action at a time: mic while empty, Send once there is text;
            // recording: ■ stops to a draft, Send stops and sends.
            when {
                model.recording -> {
                    Box(
                        Modifier.size(SEND).clip(Radius.pill).clickable { model.stopRecording() },
                        contentAlignment = Alignment.Center,
                    ) { Icon(painterResource(R.drawable.ic_stop), "Stop", Modifier.size(20.dp), tint = c.danger) }
                    SendHit(true, "Send voice message") { model.sendVoiceNow() }
                }
                model.draft.isBlank() && model.composeMode !is ComposeMode.Edit ->
                    IconTool(R.drawable.ic_mic, "Record voice message", startMic)
                else -> SendHit(model.draft.isNotBlank(), "Send") { model.send() }
            }
        }
    }
}

/** Reply / edit context above the field (Linux `composer_context_bar`). */
@Composable
private fun ContextBar(model: AppModel) {
    val c = Cursus.colors
    val (label, text) = when (val mode = model.composeMode) {
        is ComposeMode.Reply -> "Reply to ${if (mode.message.outgoing) "yourself" else mode.message.sender?.name ?: "message"}" to mode.message.text
        is ComposeMode.Edit -> "Edit message" to mode.message.text
        ComposeMode.New -> return
    }
    Row(
        Modifier.fillMaxWidth().padding(start = 12.dp, end = 4.dp, top = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.width(3.dp).height(32.dp).background(c.accent))
        Column(Modifier.weight(1f).padding(horizontal = 8.dp)) {
            Text(label, style = Cursus.type.meta, color = c.accent)
            Text(text, style = Cursus.type.meta, color = c.textSecondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        IconTool(R.drawable.ic_cancel, "Cancel", { model.cancelCompose() })
    }
}

/** Linux `voice_draft_bar`: ✕ · "Voice · 0:05" over the waveform · Send. */
@Composable
private fun VoiceDraftBar(model: AppModel, secs: Long, peaks: List<Float>) {
    val c = Cursus.colors
    Column(Modifier.fillMaxWidth()) {
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
        Row(
            Modifier.fillMaxWidth().background(c.bgSidebar).padding(horizontal = 12.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            IconTool(R.drawable.ic_cancel, "Discard", { model.discardVoiceDraft() })
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text("Voice · ${formatDuration(secs)}", style = Cursus.type.meta, color = c.textSecondary)
                Waveform(peaks, 1f, Modifier.fillMaxWidth().height(36.dp))
            }
            SendHit(true, "Send voice message") { model.sendVoiceDraft() }
        }
    }
}
