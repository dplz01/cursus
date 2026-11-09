package chat.cursus

import android.app.Application
import chat.cursus.core.Native

class CursusApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        // Before anything else touches the core: private storage for the
        // database, files and libsimplex's temp dir.
        Native.init(filesDir.resolve("cursus").absolutePath)
        Notifier.createChannel(this)
    }
}
