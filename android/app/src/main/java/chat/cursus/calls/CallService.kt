package chat.cursus.calls

import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import chat.cursus.CursusApplication

/** Started from the visible call/answer action after microphone permission. */
class CallService : Service() {
    private var awake: PowerManager.WakeLock? = null
    private val calls get() = (application as CursusApplication).model.calls
    override fun onBind(intent: Intent?): IBinder? = null
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val action = intent?.getStringExtra("callAction")
        if (action != "start" && action != "accept") { stopSelf(); return START_NOT_STICKY }
        try {
            val name = intent.getStringExtra("name") ?: "Cursus"
            val type = if (Build.VERSION.SDK_INT >= 30) ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE else 0
            ServiceCompat.startForeground(this, CallNotifications.ONGOING,
                CallNotifications.ongoingNotification(this, name, "connecting"), type)
            if (awake == null) awake = getSystemService(PowerManager::class.java)
                .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "Cursus:audio-call").apply { acquire(6 * 60 * 60 * 1000L) }
            calls.serviceStarted(action, intent.getLongExtra("contactId", 0), name, intent.getLongExtra("generation", -1))
        } catch (e: Exception) {
            (application as CursusApplication).model.error = e.message ?: "Could not start the call service."
            stopSelf()
        }
        return START_NOT_STICKY // An interrupted call must never resurrect after process death.
    }
    override fun onDestroy() {
        CallNotifications.resetOngoing()
        awake?.let { if (it.isHeld) it.release() }; awake = null
        calls.serviceStopped()
        super.onDestroy()
    }
    companion object {
        fun start(context: Context, action: String, contactId: Long, name: String, generation: Long) {
            ContextCompat.startForegroundService(context, Intent(context, CallService::class.java)
                .putExtra("generation", generation).putExtra("callAction", action).putExtra("contactId", contactId).putExtra("name", name))
        }
        fun stop(context: Context) { context.stopService(Intent(context, CallService::class.java)) }
    }
}
