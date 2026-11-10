package chat.cursus

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

enum class ThemeMode { System, Light, Dark }

/** App preferences on this phone (not chat data: that is in the core). */
class Prefs(context: Context) {
    private val sp = context.getSharedPreferences("cursus.prefs", Context.MODE_PRIVATE)

    private fun bool(key: String, default: Boolean) = mutableStateOf(sp.getBoolean(key, default))

    private val themeState = mutableStateOf(
        runCatching { ThemeMode.valueOf(sp.getString("theme", null) ?: "System") }.getOrDefault(ThemeMode.System)
    )
    var theme: ThemeMode
        get() = themeState.value
        set(v) { themeState.value = v; sp.edit().putString("theme", v.name).apply() }

    private val cleanLinksState = bool("cleanLinks", true)
    /** Remove tracking codes from links you send and open (core rule). */
    var cleanLinks: Boolean
        get() = cleanLinksState.value
        set(v) { cleanLinksState.value = v; sp.edit().putBoolean("cleanLinks", v).apply() }

    private val hideTextState = bool("hideNotificationText", false)
    var hideNotificationText: Boolean
        get() = hideTextState.value
        set(v) { hideTextState.value = v; sp.edit().putBoolean("hideNotificationText", v).apply() }

    private val incognitoState = bool("incognito", false)
    /** New one-time links and links pasted on the Connect card use a random
     *  incognito profile (the official apps' toggle). */
    var incognito: Boolean
        get() = incognitoState.value
        set(v) { incognitoState.value = v; sp.edit().putBoolean("incognito", v).apply() }

    private val secureScreenState = bool("secureScreen", true)
    /** Keep chats out of screenshots and the recent-apps preview. */
    var secureScreen: Boolean
        get() = secureScreenState.value
        set(v) { secureScreenState.value = v; sp.edit().putBoolean("secureScreen", v).apply() }

    private val backgroundState = bool("background", true)
    /** Keep receiving while Cursus is in the background (foreground service). */
    var background: Boolean
        get() = backgroundState.value
        set(v) { backgroundState.value = v; sp.edit().putBoolean("background", v).apply() }
}
