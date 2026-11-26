@file:OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)

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
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.foundation.content.contentReceiver
import androidx.compose.foundation.content.consume
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.snapshotFlow
import kotlinx.coroutines.flow.collectLatest
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.runtime.getValue
import chat.cursus.ui.components.rememberLoaded
import androidx.compose.ui.graphics.ImageBitmap
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import androidx.compose.ui.graphics.Color
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
import chat.cursus.ui.components.glass
import chat.cursus.ui.theme.Cursus
import chat.cursus.ui.theme.Radius

/** One-line field and its action: 42dp (between the 36dp "+" and the
 *  48dp of other messengers), radius 12 like the "+", so corners match. */
private val ROW_HIT = 42.dp
private val FieldShape get() = Radius.smallControl
/** Soft lift of the square action beside the field (as the glass card). */
private val FLOAT_ELEVATION = 3.dp

/** A card floating above the field (reply / edit bar, pasted image). */
@Composable
private fun FloatingCard(content: @Composable () -> Unit) {
    val c = Cursus.colors
    Box(
        Modifier.fillMaxWidth().padding(start = 10.dp, end = 10.dp, top = 8.dp).glass(Radius.smallControl),
    ) { content() }
}

@Composable
fun Composer(model: AppModel) {
    val c = Cursus.colors
    val context = LocalContext.current
    val field = rememberTextFieldState(model.draft)
    LaunchedEffect(model.draft) {
        if (field.text.toString() != model.draft) field.edit { replace(0, length, model.draft) }
    }
    LaunchedEffect(field) {
        snapshotFlow { field.text.toString() }.collectLatest { if (it != model.draft) model.draftChanged(it) }
    }
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

    // No bar behind it: the field and its action float over the chat with
    // Material elevation; anything above the field (reply / edit, pasted
    // image) floats on its own card.
    // No bar or fade behind it: only the field, its action and the cards
    // above it float, each on its own glass (`glass`, blurred backdrop).
    Column(Modifier.fillMaxWidth()) {
        EmojiSuggestions(model)
        if (model.composeMode !is ComposeMode.New) FloatingCard { ContextBar(model) }
        model.pastedImage?.let { file ->
            FloatingCard { Row(Modifier.fillMaxWidth().padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
                val bitmap by rememberLoaded<ImageBitmap?>(null, file) {
                    withContext(Dispatchers.IO) { decodeScaled(file.absolutePath, 256) }
                }
                bitmap?.let { androidx.compose.foundation.Image(it, "Image to send", Modifier.size(72.dp).clip(Radius.md)) }
                Text("Image ready to send", Modifier.weight(1f).padding(12.dp), color = c.textSecondary)
                IconTool(R.drawable.ic_cancel, "Discard image", model::discardPastedImage)
            } }
        }
        // A floating field with the control radius of every other button,
        // the paperclip inside it (✕ while recording), and one rounded-square
        // action beside it: mic while empty, Send once there is text.
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 10.dp, vertical = 8.dp),
            verticalAlignment = Alignment.Bottom,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Row(
                Modifier.weight(1f).heightIn(min = ROW_HIT).glass(FieldShape),
                verticalAlignment = Alignment.Bottom,
            ) {
                Box(
                    Modifier.weight(1f).heightIn(min = ROW_HIT).padding(start = 14.dp, end = 4.dp, top = 9.dp, bottom = 9.dp),
                    contentAlignment = Alignment.CenterStart,
                ) {
                    if (model.recording) {
                        Text("Recording… ${formatDuration(model.recordSecs.toLong())}", style = Cursus.type.body, color = c.danger)
                    } else if (model.voicePreparing) {
                        Text("Preparing voice…", style = Cursus.type.body, color = c.textSecondary)
                    } else {
                        if (model.draft.isEmpty()) Text("Message", style = Cursus.type.body, color = c.textSecondary)
                        BasicTextField(
                            state = field,
                            textStyle = Cursus.type.body.copy(color = c.textPrimary),
                            cursorBrush = SolidColor(c.accent),
                            lineLimits = androidx.compose.foundation.text.input.TextFieldLineLimits.MultiLine(maxHeightInLines = 6),
                            modifier = Modifier.fillMaxWidth().contentReceiver { content ->
                                content.consume { item ->
                                    val uri = item.uri
                                    if (uri != null && context.contentResolver.getType(uri)?.startsWith("image/") == true) {
                                        model.pasteImage(uri)
                                        true
                                    } else false
                                }
                            },
                        )
                    }
                }
                InFieldTool(
                    if (model.recording) R.drawable.ic_cancel else R.drawable.ic_paperclip,
                    if (model.recording) "Discard recording" else "Attach",
                ) { if (model.recording) model.discardRecording() else pick.launch(arrayOf("*/*")) }
            }
            when {
                model.recording -> {
                    SquareAction(R.drawable.ic_stop, "Stop", soft = true, tint = c.danger) { model.stopRecording() }
                    SquareAction(R.drawable.ic_send, "Send voice message") { model.sendVoiceNow() }
                }
                model.pastedImage == null && model.draft.isBlank() && model.composeMode !is ComposeMode.Edit ->
                    SquareAction(R.drawable.ic_mic, "Record voice message", soft = true, onClick = startMic)
                else -> SquareAction(R.drawable.ic_send, "Send", enabled = model.pastedImage != null || model.draft.isNotBlank()) { model.send() }
            }
        }
    }
}

/** Icon button inside the composer field (paperclip / ✕), field-height hit. */
@Composable
private fun InFieldTool(icon: Int, description: String, onClick: () -> Unit) {
    Box(Modifier.size(ROW_HIT).clip(FieldShape).clickable(onClick = onClick), contentAlignment = Alignment.Center) {
        Icon(painterResource(icon), description, Modifier.size(20.dp), tint = Cursus.colors.textSecondary)
    }
}

/** The composer's action: a rounded square like the list header's "+".
 *  Soft (accent tint) for mic / stop, solid accent for Send. */
@Composable
private fun SquareAction(icon: Int, description: String, soft: Boolean = false, enabled: Boolean = true, tint: Color? = null, onClick: () -> Unit) {
    val c = Cursus.colors
    val fill = when {
        soft -> c.accentSoft
        enabled -> c.accent
        else -> c.disabledFill
    }
    Box(
        Modifier.size(ROW_HIT).shadow(FLOAT_ELEVATION, Radius.smallControl, clip = false,
            ambientColor = Color.Black.copy(alpha = 0.25f), spotColor = Color.Black.copy(alpha = 0.3f)).clip(Radius.smallControl)
            .background(fill).clickable(enabled = enabled, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(painterResource(icon), description, Modifier.size(20.dp), tint = tint ?: if (soft) c.accent else c.accentOn)
    }
}

/** Linux `composer_context_bar`: "Editing" / "Replying" with a preview, ✕ cancels. */
@Composable
private fun ContextBar(model: AppModel) {
    val c = Cursus.colors
    val (icon, label, text) = when (val mode = model.composeMode) {
        is ComposeMode.Reply -> Triple(R.drawable.ic_reply, "Replying", mode.message.caption.ifBlank { mode.message.text })
        is ComposeMode.Edit -> Triple(R.drawable.ic_edit_3, "Editing", mode.message.text)
        ComposeMode.New -> return
    }
    Row(
        Modifier.fillMaxWidth().padding(start = 12.dp, end = 4.dp, top = 4.dp, bottom = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(painterResource(icon), null, Modifier.size(16.dp), tint = c.accent)
        Column(Modifier.weight(1f)) {
            Text(label, style = Cursus.type.meta, color = c.accent)
            Text(text, style = Cursus.type.meta, color = c.textSecondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        IconTool(R.drawable.ic_cancel, "Cancel", { model.cancelCompose() })
    }
}

/** Linux `SUGGEST_INSET` / `SUGGEST_ROW_RADIUS`: concentric with the field. */
private val SUGGEST_INSET = 4.dp
private val SuggestRowShape = RoundedCornerShape(12.dp - SUGGEST_INSET)

/** Linux emoji suggestions: fixed 5-row card, hits glued to the field. */
@Composable
private fun EmojiSuggestions(model: AppModel) {
    val c = Cursus.colors
    val hits = model.emojiSuggestions
    if (hits.isEmpty()) return
    // Same surface and width as the field: its glass card, ending where the
    // field ends (the action button sits beside the field, not under it).
    Column(
        Modifier.fillMaxWidth().padding(start = 10.dp, end = 10.dp + ROW_HIT + 8.dp, top = 6.dp)
            .height(38.dp * 5 + 8.dp)
            .glass(FieldShape).padding(SUGGEST_INSET),
        verticalArrangement = Arrangement.spacedBy(2.dp, Alignment.Bottom),
    ) {
        for (hit in hits.take(5)) {
            Row(
                Modifier.fillMaxWidth().height(36.dp).clip(SuggestRowShape).clickable { model.acceptEmoji(hit) }.padding(horizontal = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Text(hit.glyph, style = Cursus.type.body)
                Text(":${hit.shortcode}:", style = Cursus.type.meta, color = c.textPrimary)
            }
        }
    }
}

/** Linux `voice_draft_bar`: ✕ · "Voice · 0:05" over the waveform · Send. */
@Composable
private fun VoiceDraftBar(model: AppModel, secs: Long, peaks: List<Float>) {
    val c = Cursus.colors
    // Same floating field + square action as the composer.
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 10.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(
            Modifier.weight(1f).glass(FieldShape).padding(end = 12.dp, top = 6.dp, bottom = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            InFieldTool(R.drawable.ic_cancel, "Discard") { model.discardVoiceDraft() }
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text("Voice · ${formatDuration(secs)}", style = Cursus.type.meta, color = c.textSecondary)
                Waveform(peaks, 1f, Modifier.fillMaxWidth().height(28.dp))
            }
        }
        SquareAction(R.drawable.ic_send, "Send voice message") { model.sendVoiceDraft() }
    }
}
