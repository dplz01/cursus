package chat.cursus.calls

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.media.AudioAttributes
import android.media.RingtoneManager
import androidx.core.app.NotificationCompat
import chat.cursus.CursusApplication
import chat.cursus.MainActivity
import chat.cursus.R

object CallNotifications {
    private const val INCOMING = 20
    const val ONGOING = 21
    private var incomingName: String? = null
    private var ongoingKey: Pair<String, String>? = null
    fun createChannels(context: Context) {
        val manager = context.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(NotificationChannel("incoming-calls", "Incoming calls", NotificationManager.IMPORTANCE_HIGH).apply {
            enableVibration(true)
            setSound(RingtoneManager.getDefaultUri(RingtoneManager.TYPE_RINGTONE),
                AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_NOTIFICATION_RINGTONE)
                    .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION).build())
        })
        manager.createNotificationChannel(NotificationChannel("active-call", "Ongoing call", NotificationManager.IMPORTANCE_LOW))
    }
    private fun open(context: Context, answer: Boolean = false): PendingIntent = PendingIntent.getActivity(
        context, if (answer) 21 else 20, Intent(context, if (answer) AnswerCallActivity::class.java else MainActivity::class.java)
            .putExtra("showCall", true), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )
    private fun action(context: Context, reject: Boolean) = PendingIntent.getBroadcast(
        context, if (reject) 22 else 23, Intent(context, CallActionReceiver::class.java).putExtra("reject", reject),
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )
    private fun notify(context: Context, id: Int, notification: Notification) {
        try { context.getSystemService(NotificationManager::class.java).notify(id, notification) }
        catch (_: SecurityException) { /* Notification access can be revoked during a call. */ }
    }
    fun incoming(context: Context, name: String) {
        if (incomingName == name) return
        incomingName = name
        val notification = NotificationCompat.Builder(context, "incoming-calls")
            .setSmallIcon(R.drawable.ic_calls).setContentTitle(name).setContentText("Incoming audio call")
            .setContentIntent(open(context)).setCategory(NotificationCompat.CATEGORY_CALL)
            .setPriority(NotificationCompat.PRIORITY_HIGH).setOngoing(true).setTimeoutAfter(60_000)
            .addAction(0, "Decline", action(context, true)).addAction(0, "Answer", open(context, true)).build()
        notification.flags = notification.flags or Notification.FLAG_INSISTENT
        notify(context, INCOMING, notification)
    }
    fun dismissIncoming(context: Context) {
        if (incomingName != null) context.getSystemService(NotificationManager::class.java).cancel(INCOMING)
        incomingName = null
    }
    fun ongoingNotification(context: Context, name: String, phase: String): Notification =
        NotificationCompat.Builder(context, "active-call").setSmallIcon(R.drawable.ic_calls)
            .setContentTitle(name).setContentText(when (phase) {
                "connected" -> "Audio call in progress"; "ringing" -> "Calling…"; else -> "Connecting…"
            }).setContentIntent(open(context)).setOngoing(true).setOnlyAlertOnce(true)
            .setCategory(NotificationCompat.CATEGORY_CALL).addAction(0, "Hang up", action(context, false)).build()
    fun ongoing(context: Context, name: String, phase: String) {
        val key = name to phase
        if (ongoingKey != key) { ongoingKey = key; notify(context, ONGOING, ongoingNotification(context, name, phase)) }
    }
    fun resetOngoing() { ongoingKey = null }
}

class CallActionReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val pending = goAsync()
        (context.applicationContext as CursusApplication).model.calls.end(intent.getBooleanExtra("reject", false))
            .invokeOnCompletion { pending.finish() }
    }
}
