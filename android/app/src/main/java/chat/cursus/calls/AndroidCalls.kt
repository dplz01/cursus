package chat.cursus.calls

import android.content.Context
import android.media.Ringtone
import android.media.RingtoneManager
import android.os.Build
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.ProcessLifecycleOwner
import chat.cursus.core.CallRecord
import chat.cursus.core.Core
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable

@Serializable
data class NativeCall(val contactId: Long, val name: String, val phase: String, val outgoing: Boolean,
    val muted: Boolean = false, val encrypted: Boolean = false, val durationSecs: Long = 0)
@Serializable
data class CallState(val active: NativeCall? = null, val error: String? = null)

/** Process-lived, independent of any activity, window size or navigation pane. */
class AndroidCalls(private val context: Context, private val scope: CoroutineScope,
    private val locked: () -> Boolean, private val ready: () -> Boolean, private val beforeCall: () -> Unit, private val report: (String) -> Unit) {
    var state by mutableStateOf(CallState()); private set
    var expanded by mutableStateOf(false)
    var speaker by mutableStateOf(false); private set
    var answerRequested by mutableStateOf(false)
    var starting by mutableStateOf(false); private set
    var historyRevision by androidx.compose.runtime.mutableIntStateOf(0); private set
    var history by mutableStateOf<List<CallRecord>?>(null); private set
    private var loadedHistoryRevision: Int? = null

    fun invalidateHistory() { historyRevision++ }

    fun clearHistory() {
        history = null
        loadedHistoryRevision = null
        invalidateHistory()
    }

    suspend fun loadHistory() {
        val revision = historyRevision
        if (loadedHistoryRevision == revision || !ready()) return
        val records = Core.call<List<CallRecord>>("callHistory")
        if (revision == historyRevision && ready()) {
            history = records
            loadedHistoryRevision = revision
        }
    }

    private var poll: Job? = null
    private var audio: CallAudio? = null
    private val mediaLock = Mutex()
    private val commands = Mutex()
    private var ending = false
    private var requestGeneration = 0L
    private var ringtone: Ringtone? = null
    private var lastError: String? = null
    private var serviceReady = false

    fun monitor() {
        if (poll != null) return
        poll = scope.launch {
            while (true) {
                if (!ready()) { delay(1000); continue }
                try { commands.withLock { val next = Core.call<CallState>("callState"); if (ready()) update(next) } }
                catch (e: kotlinx.coroutines.CancellationException) { throw e }
                catch (e: Exception) { report(e.message ?: "Could not update the call.") }
                delay(400)
            }
        }
    }

    fun start(contactId: Long, name: String) = launchService("start", contactId, name)
    fun accept() { if (state.active?.phase == "incoming") launchService("accept") }
    private fun launchService(action: String, id: Long = 0, name: String = "Contact") {
        if (starting || ending || (action == "start" && state.active != null)) return
        beforeCall(); starting = true; expanded = true
        requestGeneration++
        try { CallService.start(context, action, id, name, requestGeneration) }
        catch (e: Exception) { starting = false; report(e.message ?: "Could not start the call service.") }
    }

    /** Only invoked after Android has promoted the microphone foreground service. */
    fun serviceStarted(action: String, id: Long, name: String, generation: Long) {
        if (!starting || generation != requestGeneration) { stopService(); return }
        serviceReady = true
        scope.launch {
            commands.withLock { try {
                val p = if (action == "start") mapOf("contactId" to id, "name" to name) else emptyMap()
                update(Core.call(if (action == "start") "callStart" else "callAccept", p))
            } catch (e: Exception) { report(e.message ?: "Could not start the call."); stopMedia(); stopService() }
            finally { starting = false } }
        }
    }

    fun end(reject: Boolean = false): Job {
        if (ending) return scope.launch {}
        ending = true; starting = false; requestGeneration++
        return scope.launch { commands.withLock {
            stopMedia() // Stop the microphone before waiting for peer signaling.
            try { update(Core.call(if (reject) "callReject" else "callEnd")) }
            catch (e: Exception) { report(e.message ?: "Could not end the call.") }
            finally { stopMedia(); stopService(); ending = false }
        } }
    }
    fun mute() {
        scope.launch { commands.withLock {
            val call = state.active ?: return@withLock
            runCatching { Core.call<CallState>("callMute", mapOf("muted" to !call.muted)) }
                .onSuccess { update(it) }.onFailure { report(it.message ?: "Could not mute.") }
        } }
    }
    fun toggleSpeaker() {
        try { audio?.speaker(!speaker); speaker = !speaker }
        catch (e: Exception) { report(e.message ?: "Could not switch audio output.") }
    }
    fun serviceStopped() {
        val unexpected = serviceReady
        serviceReady = false
        starting = false
        if (unexpected && state.active != null && state.active?.phase != "incoming") end()
    }
    private fun stopService() { serviceReady = false; CallService.stop(context) }

    private suspend fun update(next: CallState) {
        val previous = state.active
        state = next
        val call = next.active
        if (previous != null && call == null) invalidateHistory()
        if (call != null && (previous?.contactId != call.contactId)) expanded = true
        if (next.error != null && next.error != lastError) report(next.error)
        lastError = next.error
        if (call?.phase == "incoming") {
            val foreground = ProcessLifecycleOwner.get().lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)
            if (foreground) {
                CallNotifications.dismissIncoming(context)
                if (ringtone == null) ringtone = RingtoneManager.getRingtone(context, RingtoneManager.getDefaultUri(RingtoneManager.TYPE_RINGTONE))
                ringtone?.let { if (Build.VERSION.SDK_INT >= 28) it.isLooping = true; if (!it.isPlaying) it.play() }
            } else { stopRinging(); CallNotifications.incoming(context, if (locked()) "Cursus" else call.name) }
        } else {
            stopRinging(); CallNotifications.dismissIncoming(context)
            if (call != null && serviceReady && !ending) {
                CallNotifications.ongoing(context, if (locked()) "Cursus" else call.name, call.phase)
                if (call.phase == "connecting" || call.phase == "connected") {
                    mediaLock.withLock {
                        if (audio == null) {
                            val device = CallAudio(context) { error -> scope.launch { report(error); end() } }
                            try { withContext(Dispatchers.IO) { device.start() }; audio = device; device.speaker(speaker) }
                            catch (e: Exception) { withContext(Dispatchers.IO) { device.close() }; report(e.message ?: "Call audio failed."); end() }
                        }
                    }
                }
            }
        }
        if (call == null && !starting) { expanded = false; answerRequested = false; stopMedia(); stopService() }
    }
    private fun stopRinging() { ringtone?.stop(); ringtone = null }
    private suspend fun stopMedia() = mediaLock.withLock {
        val device = audio; audio = null
        withContext(Dispatchers.IO) { device?.close() }
        speaker = false
    }
}
