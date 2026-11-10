package chat.cursus

import android.Manifest
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.LaunchedEffect
import chat.cursus.ui.screens.Root
import chat.cursus.ui.theme.CursusTheme

class MainActivity : ComponentActivity() {
    private val model: AppModel get() = (application as CursusApplication).model
    private val askNotifications = registerForActivityResult(ActivityResultContracts.RequestPermission()) {}

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        model.bootstrap()
        handle(intent)
        setContent {
            CursusTheme(model.prefs.theme) {
                Root(model)
            }
            LaunchedEffect(model.prefs.secureScreen) {
                if (model.prefs.secureScreen) {
                    window.setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE)
                } else {
                    window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
                }
            }
            // App lock: idle check while Cursus is on screen.
            LaunchedEffect(Unit) {
                while (true) {
                    kotlinx.coroutines.delay(5_000)
                    model.checkIdle()
                }
            }
            LaunchedEffect(model.phase) {
                if (model.phase == Phase.Ready && Build.VERSION.SDK_INT >= 33 && !Notifier.allowed(this@MainActivity)) {
                    askNotifications.launch(Manifest.permission.POST_NOTIFICATIONS)
                }
            }
        }
    }

    override fun dispatchTouchEvent(ev: android.view.MotionEvent): Boolean {
        model.touch()
        return super.dispatchTouchEvent(ev)
    }

    override fun onStart() {
        super.onStart()
        model.onForeground()
    }

    override fun onStop() {
        super.onStop()
        // Not when only rotating (the activity is recreated, the app stays).
        if (!isChangingConfigurations) model.onBackground()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handle(intent)
    }

    /** A notification opens its chat; a simplex: link opens the connect flow. */
    private fun handle(intent: Intent?) {
        intent ?: return
        intent.getStringExtra(Notifier.EXTRA_CHAT)?.let { model.pendingChat = it }
        if (intent.action == Intent.ACTION_VIEW) intent.dataString?.let { model.pendingLink = it }
    }
}
