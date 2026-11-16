package chat.cursus

import android.content.Context
import android.content.SharedPreferences
import androidx.compose.runtime.mutableStateOf
import kotlin.properties.ReadWriteProperty
import kotlin.reflect.KProperty

enum class ThemeMode { System, Light, Dark }

/** Text size: the Linux app's five steps, Medium being the design size. */
enum class TextSize(val label: String, val scale: Float) {
    ExtraSmall("Extra small", 0.8f), Small("Small", 0.9f), Medium("Medium", 1f),
    Large("Large", 1.1f), ExtraLarge("Extra large", 1.3f),
}

/**
 * App preferences on this phone (not chat data: that is in the core). Each
 * one is Compose state, so screens redraw when it changes, and is saved at
 * once. Same choices and defaults as the Linux app.
 */
class Prefs(context: Context) {
    private val sp = context.getSharedPreferences("cursus.prefs", Context.MODE_PRIVATE)

    /** A saved preference: [read] it once, [write] it on every change. */
    private fun <T> pref(read: SharedPreferences.() -> T, write: SharedPreferences.Editor.(T) -> Unit) =
        object : ReadWriteProperty<Any?, T> {
            private val state = mutableStateOf(sp.read())
            override fun getValue(thisRef: Any?, property: KProperty<*>): T = state.value
            override fun setValue(thisRef: Any?, property: KProperty<*>, value: T) {
                state.value = value
                sp.edit().apply { write(value) }.apply()
            }
        }

    private fun bool(key: String, default: Boolean) = pref({ getBoolean(key, default) }, { putBoolean(key, it) })

    private inline fun <reified E : Enum<E>> enum(key: String, default: E) = pref(
        { enumValues<E>().firstOrNull { it.name == getString(key, null) } ?: default },
        { putString(key, it.name) },
    )

    var theme by enum("theme", ThemeMode.System)
    var textSize by enum("textSize", TextSize.Medium)

    /** Remove tracking codes from links you send and open (core rule). */
    var cleanLinks by bool("cleanLinks", true)
    /** Notifications say only "New message" (on unless the user chose otherwise). */
    var hideNotificationText by bool("hideNotificationText", true)

    /** Ask for the database password again after Cursus sat unused / in the background. */
    var appLock by bool("appLock", false)
    /** Seconds unused or in the background before the app lock (0 = as soon as it's left). */
    var lockAfter by pref({ getInt("lockAfter", 30) }, { putInt("lockAfter", it) })

    var chatPreviews by bool("chatPreviews", true)
    var blurMedia by bool("blurMedia", false)
    var autoDownload by bool("autoDownload", true)
    /** Ask before downloading from file servers the sender chose. */
    var protectIp by bool("protectIp", true)

    /** New one-time links and links pasted on the Connect card use a random
     *  incognito profile (the official apps' toggle). */
    var incognito by bool("incognito", false)
    /** Keep chats out of screenshots and the recent-apps preview. */
    var secureScreen by bool("secureScreen", true)
    /** Keep receiving while Cursus is in the background (foreground service). */
    var background by bool("background", true)
}
