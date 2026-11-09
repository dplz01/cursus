package chat.cursus

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat

/**
 * Keeps Cursus (and the SimpleX core inside it) running while the app is in
 * the background, so messages arrive and notify — the Android counterpart
 * of "run in background" in the Linux tray. Runs only while unlocked.
 */
class CursusService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val open = PendingIntent.getActivity(
            this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE,
        )
        val n: Notification = NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_chats)
            .setContentTitle("Cursus is receiving messages")
            .setContentText("Tap to open. Turn off in Settings → Notifications.")
            .setContentIntent(open)
            .setOngoing(true)
            .setPriority(NotificationCompat.PRIORITY_MIN)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .build()
        val type = if (Build.VERSION.SDK_INT >= 34) ServiceInfo.FOREGROUND_SERVICE_TYPE_REMOTE_MESSAGING else 0
        ServiceCompat.startForeground(this, ID, n, type)
        return START_STICKY
    }

    companion object {
        private const val CHANNEL = "background"
        private const val ID = 1

        fun createChannel(context: Context) {
            val ch = NotificationChannel(CHANNEL, "Running in background", NotificationManager.IMPORTANCE_MIN)
            ch.setShowBadge(false)
            context.getSystemService(NotificationManager::class.java).createNotificationChannel(ch)
        }

        fun start(context: Context) {
            runCatching { ContextCompat.startForegroundService(context, Intent(context, CursusService::class.java)) }
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, CursusService::class.java))
        }
    }
}
