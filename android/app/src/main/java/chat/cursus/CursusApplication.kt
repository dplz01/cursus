package chat.cursus

import android.app.Application
import chat.cursus.core.Native

class CursusApplication : Application() {
    /**
     * App state lives as long as the process (not the activity): with the
     * background service, events keep arriving and notifying after the
     * window is closed.
     */
    val model: AppModel by lazy { AppModel(this) }

    override fun onCreate() {
        super.onCreate()
        // Before anything else touches the core: private storage for the
        // database, files and libsimplex's temp dir.
        Native.init(filesDir.resolve("cursus").absolutePath)
        Notifier.createChannel(this)
        CursusService.createChannel(this)
    }
}
