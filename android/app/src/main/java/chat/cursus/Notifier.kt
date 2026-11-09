package chat.cursus

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import chat.cursus.core.Incoming

/** New-message notifications while Cursus runs in the background. */
object Notifier {
    private const val CHANNEL = "messages"
    const val EXTRA_CHAT = "chat.cursus.CHAT"

    fun createChannel(context: Context) {
        val channel = NotificationChannel(CHANNEL, "Messages", NotificationManager.IMPORTANCE_HIGH)
        context.getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }

    fun allowed(context: Context): Boolean =
        Build.VERSION.SDK_INT < 33 ||
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) ==
            PackageManager.PERMISSION_GRANTED

    /** [hideText]: privacy setting — show only "New message". */
    fun show(context: Context, msg: Incoming, hideText: Boolean) {
        if (!allowed(context)) return
        val open = Intent(context, MainActivity::class.java)
            .setFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
            .putExtra(EXTRA_CHAT, msg.chat)
        val pending = PendingIntent.getActivity(
            context, msg.chat.hashCode(), open,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val body = when {
            hideText -> "New message"
            msg.sender != null -> "${msg.sender}: ${msg.text}"
            else -> msg.text
        }
        val n = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.ic_chats)
            .setContentTitle(if (hideText) "Cursus" else msg.title)
            .setContentText(body)
            .setAutoCancel(true)
            .setContentIntent(pending)
            .setCategory(NotificationCompat.CATEGORY_MESSAGE)
            .build()
        runCatching { NotificationManagerCompat.from(context).notify(msg.chat.hashCode(), n) }
    }

    fun clear(context: Context, chat: String) {
        NotificationManagerCompat.from(context).cancel(chat.hashCode())
    }
}
