package chat.cursus.media

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.OpenableColumns
import android.webkit.MimeTypeMap
import androidx.core.content.FileProvider
import java.io.File

object Files {
    /** Copy a picked document into [dir] under its display name (core sends from a path). */
    fun copyIn(context: Context, uri: Uri, dir: File): File {
        val name = context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
            if (it.moveToFirst()) it.getString(0) else null
        }?.replace('/', '_')?.takeIf { it.isNotBlank() } ?: "file"
        val sub = File(dir, System.currentTimeMillis().toString()).apply { mkdirs() }
        val out = File(sub, name)
        context.contentResolver.openInputStream(uri)!!.use { input -> out.outputStream().use { input.copyTo(it) } }
        return out
    }

    fun mimeOf(name: String): String =
        MimeTypeMap.getSingleton().getMimeTypeFromExtension(name.substringAfterLast('.', "").lowercase())
            ?: "application/octet-stream"

    /** Hand a readable (already decrypted) file to another app. False when none can open it. */
    fun open(context: Context, path: String, name: String): Boolean {
        val uri = FileProvider.getUriForFile(context, "${context.packageName}.files", File(path))
        val intent = Intent(Intent.ACTION_VIEW).setDataAndType(uri, mimeOf(name))
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
        return try {
            context.startActivity(intent); true
        } catch (_: ActivityNotFoundException) {
            false
        }
    }
}
