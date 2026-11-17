package chat.cursus.ui.components

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import kotlinx.coroutines.ensureActive

/** Reset a loaded value when its source changes; cancelled loads never publish. */
@Composable
fun <T> rememberLoaded(initial: T, vararg keys: Any?, load: suspend () -> T): State<T> {
    val state = remember(*keys) { mutableStateOf(initial) }
    LaunchedEffect(*keys) {
        val loaded = load()
        ensureActive()
        state.value = loaded
    }
    return state
}
