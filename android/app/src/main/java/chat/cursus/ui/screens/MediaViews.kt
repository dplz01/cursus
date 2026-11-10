package chat.cursus.ui.screens

import androidx.compose.ui.draw.blur
import kotlin.math.roundToInt
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import chat.cursus.AppModel
import chat.cursus.R
import chat.cursus.core.FileInfo
import chat.cursus.core.Message
import chat.cursus.media.VoicePlayer
import chat.cursus.media.Waveforms
import chat.cursus.ui.theme.Cursus
import chat.cursus.ui.theme.Radius
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext

/** Linux `file_state_label`. */
private fun stateLabel(state: String, outgoing: Boolean) = when (state) {
    "offer" -> "available"
    "inProgress" -> "transferring…"
    "complete" -> if (outgoing) "sent" else "saved"
    "failed" -> "failed"
    else -> "file"
}

/** Linux `transfer_status_line`: "1.2 MB · saved". */
private fun statusLine(f: FileInfo, outgoing: Boolean): String {
    val size = if (f.size > 0) humanSize(f.size) else ""
    return when (f.state) {
        "inProgress" -> {
            val p = f.progress
            if (p != null && f.size > 0) "${humanSize((f.size * p).toLong())} / ${humanSize(f.size)} · transferring…"
            else listOf(size, "transferring…").filter { it.isNotEmpty() }.joinToString(" · ")
        }
        "failed" -> listOf(size, f.error ?: "failed").filter { it.isNotEmpty() }.joinToString(" · ")
        else -> listOf(size, stateLabel(f.state, outgoing)).filter { it.isNotEmpty() }.joinToString(" · ")
    }
}

@Composable
fun FileBody(model: AppModel, m: Message, f: FileInfo) {
    when {
        f.kind == "voice" -> VoiceRow(model, m, f)
        f.kind == "image" && f.state == "complete" -> ImageThumb(model, f)
        else -> FileRow(model, m, f)
    }
}

@Composable
private fun ImageThumb(model: AppModel, f: FileInfo) {
    val path = model.plainFiles[f.id] ?: return FileRowPlaceholder(f)
    // "Blur photos and videos": blurred until tapped. A tiny decode scaled
    // up blurs on every Android version; Modifier.blur adds to it on 12+.
    val blurred = model.prefs.blurMedia && f.id !in model.revealedMedia
    val bitmap by produceState<ImageBitmap?>(null, path, blurred) {
        value = withContext(Dispatchers.IO) { decodeScaled(path, if (blurred) 24 else 720) }
    }
    bitmap?.let {
        Image(
            it, f.name,
            Modifier.padding(bottom = 4.dp).widthIn(max = 296.dp).heightIn(max = 360.dp).clip(Radius.lg)
                .then(if (blurred) Modifier.blur(16.dp) else Modifier)
                .clickable { if (blurred) model.revealedMedia.add(f.id) else model.gallery = f.id },
            contentScale = ContentScale.Fit,
        )
    } ?: FileRowPlaceholder(f)
}

@Composable
private fun FileRowPlaceholder(f: FileInfo) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Icon(painterResource(R.drawable.ic_image_square), null, Modifier.size(18.dp), tint = Cursus.colors.textSecondary)
        Text(f.name, style = Cursus.type.body, color = Cursus.colors.textPrimary, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** Download button of an offered file (Linux `offer_action`: accent, radius 8). */
@Composable
private fun OfferAction(model: AppModel, m: Message, f: FileInfo) {
    val c = Cursus.colors
    if (m.outgoing || f.state != "offer") return
    if (f.autoReceive) {
        Text("receiving…", style = Cursus.type.meta, color = c.textMuted)
        return
    }
    Box(
        Modifier.size(32.dp).clip(Radius.sm).background(c.accent).clickable { model.receive(f) },
        contentAlignment = Alignment.Center,
    ) {
        Icon(painterResource(R.drawable.ic_download), "Download", Modifier.size(16.dp), tint = c.accentOn)
    }
}

@Composable
private fun FileRow(model: AppModel, m: Message, f: FileInfo) {
    val c = Cursus.colors
    Column(Modifier.widthIn(min = 180.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            val icon = when (f.kind) {
                "image" -> R.drawable.ic_image_square
                "video" -> R.drawable.ic_play
                else -> R.drawable.ic_review_file
            }
            Icon(painterResource(icon), null, Modifier.size(18.dp), tint = c.textSecondary)
            Text(
                f.name,
                style = Cursus.type.body,
                color = if (f.state == "complete") c.accent else c.textPrimary,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f, fill = false)
                    .then(if (f.state == "complete") Modifier.clickable { model.openFile(f) } else Modifier),
            )
            OfferAction(model, m, f)
        }
        Text(
            statusLine(f, m.outgoing),
            style = Cursus.type.meta,
            color = if (f.state == "failed") c.danger else c.textMuted,
            modifier = Modifier.padding(top = 2.dp),
        )
        if (f.state == "inProgress" && f.progress != null) {
            LinearProgressIndicator(
                progress = { f.progress.toFloat() },
                modifier = Modifier.padding(top = 4.dp).fillMaxWidth(),
                color = c.accent,
                trackColor = c.bgField,
            )
        }
    }
}

fun formatDuration(secs: Long): String = "%d:%02d".format(secs / 60, secs % 60)

/** Linux `voice_waveform_row`: 40px play square (radius 12), waveform, duration. */
@Composable
private fun VoiceRow(model: AppModel, m: Message, f: FileInfo) {
    val c = Cursus.colors
    val path = model.plainFiles[f.id]
    val playing = VoicePlayer.playing == f.id
    val peaks by produceState(path?.let { Waveforms.cached(it) }, path) {
        if (path != null && value == null) value = withContext(Dispatchers.IO) { Waveforms.load(path) }
    }
    LaunchedEffect(playing) {
        while (playing) {
            VoicePlayer.tick(); delay(50)
        }
    }
    Row(
        Modifier.widthIn(min = 220.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        val ready = f.state == "complete"
        Box(
            Modifier.size(40.dp).clip(Radius.lg).background(if (ready) c.accent else c.disabledFill)
                .clickable(enabled = ready) { model.playVoice(f) },
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                painterResource(if (playing) R.drawable.ic_stop else R.drawable.ic_play),
                if (playing) "Stop" else "Play",
                Modifier.size(20.dp),
                tint = if (ready) c.accentOn else c.textMuted,
            )
        }
        Waveform(
            peaks ?: List(96) { 0.35f },
            if (playing) VoicePlayer.progress else 0f,
            Modifier.weight(1f).height(28.dp),
        )
        Text(f.duration?.let { formatDuration(it) } ?: "voice", style = Cursus.type.meta, color = c.textSecondary)
        OfferAction(model, m, f)
    }
}

/**
 * Bars; the played part in accent (Linux `Waveform` canvas, same layout):
 * fixed 3dp bars with 2dp gaps on whole pixels so edges stay crisp, the bar
 * count follows the width and peaks are resampled to it (max per bucket).
 * Short bars are dots / capsules (never shorter than wide, so no eye
 * shapes); tall bars get slightly rounded ends.
 */
@Composable
fun Waveform(peaks: List<Float>, progress: Float, modifier: Modifier = Modifier) {
    val played = Cursus.colors.accent
    val rest = Cursus.colors.textMuted
    Canvas(modifier) {
        if (peaks.isEmpty() || size.width < 1f || size.height < 1f) return@Canvas
        val bar = 3.dp.toPx().roundToInt().coerceAtLeast(1).toFloat()
        val gap = 2.dp.toPx().roundToInt().coerceAtLeast(1).toFloat()
        val pitch = bar + gap
        val width = kotlin.math.floor(size.width)
        val height = kotlin.math.floor(size.height)
        val n = kotlin.math.floor((width + gap) / pitch).toInt().coerceAtLeast(1)
        val left = kotlin.math.floor(((width - (n * pitch - gap)) / 2f).coerceAtLeast(0f))
        val src = peaks.size
        for (i in 0 until n) {
            val a = i * src / n
            val b = ((i + 1) * src / n).coerceAtLeast(a + 1).coerceAtMost(src)
            var peak = 0f
            for (k in a.coerceAtMost(src - 1) until b) peak = maxOf(peak, peaks[k])
            // Never shorter than wide: the quietest bars are round dots.
            var h = (peak.coerceIn(0.08f, 1f) * height * 0.92f).roundToInt().toFloat().coerceAtLeast(bar)
            if ((height - h).toInt() % 2 != 0) h += 1f
            val y = ((height - h) / 2f).roundToInt().toFloat()
            val color = if ((i + 1f) / n <= progress + 0.001f) played else rest
            drawRoundRect(
                color = color,
                topLeft = Offset(left + i * pitch, y),
                size = Size(bar, h),
                cornerRadius = CornerRadius(if (h <= bar * 1.5f) bar / 2f else bar * 0.4f),
            )
        }
    }
}

/** Full-screen photos of this chat; swipe between them, pinch to zoom. */
@Composable
fun ImageGallery(model: AppModel) {
    val start = model.gallery ?: return
    val images = remember(model.messages.toList()) {
        model.messages.mapNotNull { it.file }.filter { it.kind == "image" && it.state == "complete" }
    }
    val first = images.indexOfFirst { it.id == start }.coerceAtLeast(0)
    val pager = rememberPagerState(initialPage = first) { images.size }
    BackHandler { model.gallery = null }
    Box(Modifier.fillMaxSize().background(Color.Black)) {
        HorizontalPager(pager, Modifier.fillMaxSize()) { page ->
            val f = images[page]
            val bitmap by produceState<ImageBitmap?>(null, f.id) {
                val path = model.plainPath(f)
                value = path?.let { withContext(Dispatchers.IO) { decodeScaled(it, 2048) } }
            }
            var scale by remember { mutableFloatStateOf(1f) }
            var offset by remember { mutableStateOf(Offset.Zero) }
            bitmap?.let {
                Image(
                    it, f.name,
                    Modifier.fillMaxSize()
                        .pointerInputZoom(scale) { s, o -> scale = s; offset = o }
                        .graphicsLayer(scaleX = scale, scaleY = scale, translationX = offset.x, translationY = offset.y),
                    contentScale = ContentScale.Fit,
                )
            }
        }
        Row(
            Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.safeDrawing).padding(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(
                Modifier.size(40.dp).clip(Radius.pill).background(Color.Black.copy(alpha = 0.4f)).clickable { model.gallery = null },
                contentAlignment = Alignment.Center,
            ) { Icon(painterResource(R.drawable.ic_cancel), "Close", Modifier.size(20.dp), tint = Color.White) }
            Box(Modifier.weight(1f))
            if (images.size > 1) Text("${pager.currentPage + 1} / ${images.size}", color = Color.White, style = Cursus.type.meta)
            Box(Modifier.width(8.dp))
            Box(
                Modifier.size(40.dp).clip(Radius.pill).background(Color.Black.copy(alpha = 0.4f))
                    .clickable { images.getOrNull(pager.currentPage)?.let { model.openFile(it) } },
                contentAlignment = Alignment.Center,
            ) { Icon(painterResource(R.drawable.ic_actions_new_window), "Open with", Modifier.size(20.dp), tint = Color.White) }
        }
    }
}

private fun Modifier.pointerInputZoom(scale: Float, onChange: (Float, Offset) -> Unit): Modifier =
    this.then(
        Modifier.pointerInputZoomImpl(scale, onChange)
    )

@Composable
fun FileApprovalDialog(model: AppModel) {
    val (file, servers) = model.fileApproval ?: return
    val c = Cursus.colors
    AlertDialog(
        onDismissRequest = { model.fileApproval = null },
        containerColor = c.bgElevated,
        shape = Radius.lg,
        title = { Text("Download from unknown servers?", style = Cursus.type.title) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(
                    "${file.name} is stored on servers you don't use. They will see your IP address unless you use a proxy.",
                    style = Cursus.type.body, color = c.textSecondary,
                )
                for (s in servers) Text(s, style = Cursus.type.meta, color = c.textMuted)
            }
        },
        confirmButton = {
            TextButton({ model.fileApproval = null; model.receive(file, approved = true) }) { Text("Download", color = c.accent) }
        },
        dismissButton = { TextButton({ model.fileApproval = null }) { Text("Cancel", color = c.textSecondary) } },
    )
}
