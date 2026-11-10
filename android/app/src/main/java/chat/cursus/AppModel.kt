package chat.cursus

import android.app.Application
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.ProcessLifecycleOwner
import androidx.lifecycle.viewModelScope
import chat.cursus.core.AppInfo
import chat.cursus.core.Chat
import chat.cursus.core.ConnLink
import chat.cursus.core.Core
import chat.cursus.core.CoreEvent
import chat.cursus.core.CoreJson
import chat.cursus.core.Message
import chat.cursus.core.Native
import chat.cursus.core.PreparedLink
import chat.cursus.core.User
import android.net.Uri
import chat.cursus.core.FileInfo
import chat.cursus.core.Member
import chat.cursus.media.Files
import chat.cursus.media.VoiceDraft
import chat.cursus.media.VoicePlayer
import chat.cursus.media.VoiceRecorder
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

sealed interface Phase {
    data object Starting : Phase
    data object Welcome : Phase
    data object Unlock : Phase
    data object Ready : Phase
    data class Failed(val message: String) : Phase
}

/** The four destinations, in the same order on every Cursus app. */
enum class Section(val title: String, val icon: Int) {
    Chats("Chats", R.drawable.ic_chats),
    Channels("Channels", R.drawable.ic_channels),
    Calls("Calls", R.drawable.ic_calls),
    Settings("Settings", R.drawable.ic_settings),
}

sealed interface ComposeMode {
    data object New : ComposeMode
    data class Reply(val message: Message) : ComposeMode
    data class Edit(val message: Message) : ComposeMode
}

/**
 * App state and actions. All chat logic lives in cursus-core (through the
 * bridge); this only keeps what the screens show and calls the core.
 */
class AppModel(app: Application) : AndroidViewModel(app) {
    private val passwords = PasswordStore(app)
    val prefs = Prefs(app)

    var phase by mutableStateOf<Phase>(Phase.Starting); private set
    var info by mutableStateOf<AppInfo?>(null); private set
    var user by mutableStateOf<User?>(null); private set
    var busy by mutableStateOf(false); private set
    var error by mutableStateOf<String?>(null)
    var notice by mutableStateOf<String?>(null)

    var section by mutableStateOf(Section.Chats)
    val chats = mutableStateListOf<Chat>()
    var selection by mutableStateOf<String?>(null); private set
    val messages = mutableStateListOf<Message>()
    var messagesFor by mutableStateOf<String?>(null); private set

    var draft by mutableStateOf("")
    var composeMode by mutableStateOf<ComposeMode>(ComposeMode.New)
    private val drafts = mutableStateMapOf<String, String>()

    /** Readable paths of decrypted images, by file id. */
    val plainFiles = mutableStateMapOf<Long, String>()

    // Voice messages: recording → draft → send (as on Linux).
    private val recorder = VoiceRecorder()
    var recording by mutableStateOf(false); private set
    var recordLevel by mutableStateOf(0f); private set
    var recordSecs by mutableStateOf(0); private set
    var voiceDraft by mutableStateOf<VoiceDraft?>(null); private set
    /** The take is being filtered and encoded ("Preparing voice…"). */
    var voicePreparing by mutableStateOf(false); private set
    /** A download whose relays need your OK (servers not among yours). */
    var fileApproval by mutableStateOf<Pair<FileInfo, List<String>>?>(null)
    /** Image shown full screen (file id), over the conversation. */
    var gallery by mutableStateOf<Long?>(null)

    /** Opened from a notification / a link before the chats were ready. */
    var pendingChat by mutableStateOf<String?>(null)
    var pendingLink by mutableStateOf<String?>(null)

    private var pumpStarted = false
    private var chatsReload: Job? = null
    private var messagesReload: Job? = null

    /** The open chat; a group's support thread (`#9(_support:4)`) is shown as its group, retitled. */
    val selectedChat: Chat?
        get() {
            val ref = selection ?: return null
            chats.firstOrNull { it.ref == ref }?.let { return it }
            val parent = chats.firstOrNull { ref.startsWith(it.ref + "(") } ?: return null
            val memberId = ref.substringAfter("_support:", "").removeSuffix(")").toLongOrNull()
            val title = memberId?.let { id -> members.firstOrNull { it.id == id }?.name } ?: "Chat with admins"
            return parent.copy(ref = ref, title = "${parent.title} · $title", isChannel = false, role = "member")
        }

    // region Startup / unlock

    fun bootstrap() {
        if (phase != Phase.Starting) return
        startPump()
        viewModelScope.launch {
            try {
                val i: AppInfo = Core.call("appInfo")
                info = i
                if (!i.setupComplete && !i.databaseExists) {
                    phase = Phase.Welcome
                    return@launch
                }
                phase = Phase.Unlock
                passwords.load()?.let { unlock(it, remember = true, quiet = true) }
            } catch (e: Exception) {
                phase = Phase.Failed(e.message ?: e.toString())
            }
        }
    }

    fun createAccount(name: String, password: String, remember: Boolean) = open(
        mapOf("password" to password, "displayName" to name, "encryptLocalFiles" to true, "saveAccount" to true),
        password, remember, quiet = false,
    )

    fun unlock(password: String, remember: Boolean, quiet: Boolean = false) = open(
        mapOf(
            "password" to password,
            "displayName" to (info?.displayName ?: ""),
            "encryptLocalFiles" to (info?.encryptLocalFiles ?: true),
            "saveAccount" to !(info?.setupComplete ?: false),
        ),
        password, remember, quiet,
    )

    private fun open(params: Map<String, Any?>, password: String, remember: Boolean, quiet: Boolean) {
        if (busy) return
        busy = true
        viewModelScope.launch {
            try {
                val libsDir = getApplication<Application>().applicationInfo.nativeLibraryDir
                user = Core.call("open", params + ("libsDir" to libsDir))
                if (remember) passwords.save(password) else if (!quiet) passwords.delete()
                info = runCatching { Core.call<AppInfo>("appInfo") }.getOrNull()
                phase = Phase.Ready
                syncBackground()
                reloadChats()
            } catch (e: Exception) {
                val text = e.message ?: e.toString()
                val wrongKey = listOf("wrong", "passphrase", "key").any { text.lowercase().contains(it) }
                when {
                    quiet -> if (wrongKey) passwords.delete()
                    wrongKey -> error = "Wrong password.\n\n$text"
                    else -> error = text
                }
            } finally {
                busy = false
            }
        }
    }

    fun lock() {
        VoicePlayer.stop()
        discardRecording()
        discardVoiceDraft()
        plainFiles.clear()
        gallery = null
        busy = true
        viewModelScope.launch {
            runCatching { Core.run("close") }
            busy = false
        }
        selection = null
        chats.clear()
        messages.clear()
        user = null
        draft = ""
        drafts.clear()
        composeMode = ComposeMode.New
        phase = Phase.Unlock
        syncBackground()
    }

    /** Forget everything on this phone (forgotten password). */
    fun wipeAll() {
        viewModelScope.launch {
            try {
                Core.run("wipeLocalData")
                passwords.delete()
                selection = null
                chats.clear()
                messages.clear()
                user = null
                info = runCatching { Core.call<AppInfo>("appInfo") }.getOrNull()
                phase = Phase.Welcome
            } catch (e: Exception) {
                error = e.message
            }
        }
    }

    /** The background service runs exactly while unlocked with the setting on. */
    fun syncBackground() {
        val app = getApplication<Application>()
        if (phase == Phase.Ready && prefs.background) CursusService.start(app) else CursusService.stop(app)
    }

    val rememberedPassword: Boolean get() = passwords.load() != null

    fun forgetPassword() = passwords.delete()

    // endregion

    // region Events

    private fun startPump() {
        if (pumpStarted) return
        pumpStarted = true
        Thread({
            while (true) {
                val raw = Native.nextEvent(500) ?: continue
                val event = runCatching { CoreJson.decodeFromString<CoreEvent>(raw) }.getOrNull() ?: continue
                viewModelScope.launch { handle(event) }
            }
        }, "cursus-events").apply { isDaemon = true }.start()
    }

    private fun handle(e: CoreEvent) {
        if (phase != Phase.Ready) return
        if (e.refreshChats) scheduleChatsReload()
        val open = selection
        if (e.refreshMessages && open != null && (e.chat == null || e.chat == open || e.chat.startsWith("$open("))) {
            scheduleMessagesReload()
        }
        val foreground = ProcessLifecycleOwner.get().lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)
        for (msg in e.incoming) {
            val chat = chats.firstOrNull { it.ref == msg.chat }
            if (chat?.muted == true) continue
            if (foreground) continue // the open chat or the list badge shows it
            Notifier.show(getApplication(), msg, prefs.hideNotificationText)
        }
    }

    // endregion

    // region Chats

    fun reloadChats() {
        viewModelScope.launch {
            try {
                val list: List<Chat> = Core.call("listChats")
                chats.clear()
                chats.addAll(list)
            } catch (e: Exception) {
                notice = e.message
            }
        }
    }

    private fun scheduleChatsReload() {
        chatsReload?.cancel()
        chatsReload = viewModelScope.launch {
            delay(150)
            reloadChats()
        }
    }

    fun select(ref: String?) {
        val old = selection
        if (old == ref) return
        if (old != null) drafts[old] = draft
        selection = ref
        draft = ref?.let { drafts[it] } ?: ""
        composeMode = ComposeMode.New
        discardRecording()
        discardVoiceDraft()
        VoicePlayer.stop()
        messages.clear()
        messagesFor = null
        if (ref != null) {
            reloadMessages(ref)
            markRead(ref)
            Notifier.clear(getApplication(), ref)
        }
    }

    fun reloadMessages(ref: String) {
        viewModelScope.launch {
            try {
                val list: List<Message> = Core.call("listMessages", mapOf("chat" to ref, "count" to 200))
                if (selection != ref) return@launch
                messages.clear()
                messages.addAll(list)
                messagesFor = ref
                loadImages(list)
            } catch (e: Exception) {
                notice = e.message
            }
        }
    }

    private fun scheduleMessagesReload() {
        val ref = selection ?: return
        messagesReload?.cancel()
        messagesReload = viewModelScope.launch {
            delay(120)
            reloadMessages(ref)
            if (ProcessLifecycleOwner.get().lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) markRead(ref)
        }
    }

    private fun loadImages(list: List<Message>) {
        for (m in list) {
            val f = m.file ?: continue
            if (f.kind !in setOf("image", "voice") || f.state != "complete" || f.path == null || plainFiles.containsKey(f.id)) continue
            viewModelScope.launch {
                runCatching {
                    val path: String = Core.call("plainFile", mapOf("path" to f.path, "key" to f.key, "nonce" to f.nonce))
                    plainFiles[f.id] = path
                }
            }
        }
    }

    private fun markRead(ref: String) {
        viewModelScope.launch {
            val chat = chats.firstOrNull { it.ref == ref }
            if (chat != null && chat.unread == 0L && !chat.unreadChat) return@launch
            runCatching { Core.run("markRead", mapOf("chat" to ref)) }
            reloadChats()
        }
    }

    // The core sends no event for these: reload the chats so the change shows.
    fun markUnread(chat: Chat) = change(chat, "markUnread", mapOf("chat" to chat.ref))

    fun setFavorite(chat: Chat, on: Boolean) =
        change(chat, "setChatSettings", mapOf("chat" to chat.ref, "favorite" to on))

    fun setMuted(chat: Chat, muted: Boolean) =
        change(chat, "setChatSettings", mapOf("chat" to chat.ref, "notifications" to if (muted) "none" else "all"))

    fun clear(chat: Chat) = act {
        Core.run("clearChat", mapOf("chat" to chat.ref))
        if (selection == chat.ref) reloadMessages(chat.ref)
    }

    fun delete(chat: Chat) = act {
        Core.run("deleteChat", mapOf("chat" to chat.ref))
        if (selection == chat.ref) select(null)
    }

    fun acceptRequest(chat: Chat) = act { Core.run("acceptRequest", mapOf("id" to chat.pendingRequestId)) }

    fun rejectRequest(chat: Chat) = act {
        Core.run("rejectRequest", mapOf("id" to chat.pendingRequestId))
        if (selection == chat.ref) select(null)
    }

    fun joinGroup(groupId: Long) = act { Core.run("joinGroup", mapOf("groupId" to groupId)) }

    fun createGroup(name: String, channel: Boolean, done: (String) -> Unit) = act {
        val ref = if (channel) {
            Core.raw("createChannel", mapOf("name" to name)).let {
                CoreJson.decodeFromJsonElement(kotlinx.serialization.json.JsonObject.serializer(), it)["ref"]
                    ?.let { r -> (r as kotlinx.serialization.json.JsonPrimitive).content }
            }
        } else {
            Core.call<String>("createGroup", mapOf("name" to name))
        }
        reloadChats()
        if (ref != null) done(ref)
    }

    // endregion

    // region Messages

    fun send() {
        val ref = selection ?: return
        val text = draft.trim()
        if (text.isEmpty()) return
        val mode = composeMode
        draft = ""
        composeMode = ComposeMode.New
        drafts.remove(ref)
        viewModelScope.launch {
            try {
                when (mode) {
                    is ComposeMode.Edit -> Core.run(
                        "updateText",
                        mapOf("chat" to ref, "itemId" to mode.message.id, "text" to text, "cleanLinks" to prefs.cleanLinks),
                    )
                    is ComposeMode.Reply -> Core.run(
                        "sendText",
                        mapOf("chat" to ref, "text" to text, "quote" to mode.message.id, "cleanLinks" to prefs.cleanLinks),
                    )
                    ComposeMode.New -> Core.run(
                        "sendText",
                        mapOf("chat" to ref, "text" to text, "cleanLinks" to prefs.cleanLinks),
                    )
                }
                reloadMessages(ref)
            } catch (e: Exception) {
                draft = text
                composeMode = mode
                error = e.message
            }
        }
    }

    fun react(message: Message, emoji: String, add: Boolean) = act {
        Core.run("react", mapOf("chat" to selection, "itemId" to message.id, "emoji" to emoji, "add" to add))
        selection?.let { reloadMessages(it) }
    }

    fun deleteMessage(message: Message, forEveryone: Boolean) = act {
        Core.run("deleteItems", mapOf("chat" to selection, "itemIds" to listOf(message.id), "forEveryone" to forEveryone))
        selection?.let { reloadMessages(it) }
    }

    fun startEdit(message: Message) {
        composeMode = ComposeMode.Edit(message)
        draft = message.text
    }

    fun cancelCompose() {
        if (composeMode is ComposeMode.Edit) draft = ""
        composeMode = ComposeMode.New
    }

    // endregion

    // region Composer helpers (core rules: chat::emoji)

    val emojiSuggestions = mutableStateListOf<chat.cursus.core.EmojiHit>()

    /** After each edit: expand completed `:shortcodes:` and refresh the suggestions. */
    fun draftChanged(text: String) {
        draft = text
        viewModelScope.launch {
            val expanded = runCatching { Core.call<String>("expandShortcodes", mapOf("text" to text)) }.getOrNull()
            if (expanded != null && expanded != text && draft == text) draft = expanded
            val hits = runCatching { Core.call<List<chat.cursus.core.EmojiHit>>("emojiSuggest", mapOf("text" to draft)) }
                .getOrDefault(emptyList())
            emojiSuggestions.clear()
            emojiSuggestions.addAll(hits)
        }
    }

    fun acceptEmoji(hit: chat.cursus.core.EmojiHit) {
        viewModelScope.launch {
            runCatching { Core.call<String>("acceptShortcode", mapOf("text" to draft, "glyph" to hit.glyph)) }
                .getOrNull()?.let { draft = it }
            emojiSuggestions.clear()
        }
    }

    // endregion

    // region Message actions

    suspend fun messageInfo(m: Message): chat.cursus.core.MessageInfo? = runCatching {
        Core.call<chat.cursus.core.MessageInfo>("messageInfo", mapOf("chat" to selection, "itemId" to m.id))
    }.onFailure { error = it.message }.getOrNull()

    fun forward(m: Message, to: Chat) = act {
        Core.run("forward", mapOf("chat" to selection, "itemIds" to listOf(m.id), "to" to to.ref))
        notice = "Forwarded to ${to.title}"
        reloadChats()
    }

    suspend fun reactionMembers(m: Message, emoji: String): List<String> = runCatching {
        Core.call<List<String>>("reactionMembers", mapOf("chat" to selection, "itemId" to m.id, "emoji" to emoji))
    }.getOrDefault(emptyList())

    /** Save a received / sent file where the user picked (decrypted copy). */
    fun saveFile(file: FileInfo, target: Uri) = act {
        val path = plainPath(file) ?: return@act run { notice = "The file isn't on this phone" }
        withContext(Dispatchers.IO) {
            getApplication<Application>().contentResolver.openOutputStream(target)!!.use { out ->
                File(path).inputStream().use { it.copyTo(out) }
            }
        }
        notice = "Saved ${file.name}"
    }

    // endregion

    // region Attachments and voice

    private fun tmp(sub: String): File = File(info?.dataDir ?: getApplication<Application>().filesDir.path, "tmp/$sub")

    /** A picked document: copied in, then sent (images go as images — core rule on the name). */
    fun sendFile(uri: Uri) {
        val ref = selection ?: return
        act {
            val file = withContext(Dispatchers.IO) { Files.copyIn(getApplication(), uri, tmp("out")) }
            try {
                Core.run("sendFile", mapOf("chat" to ref, "path" to file.absolutePath))
            } finally {
                // The core keeps its own (encrypted) copy in files/.
                withContext(Dispatchers.IO) { file.parentFile?.deleteRecursively() }
            }
            reloadMessages(ref)
        }
    }

    fun startRecording() {
        if (recording) return
        VoicePlayer.stop()
        try {
            recorder.start(tmp("rec"))
        } catch (e: Exception) {
            error = "The microphone could not start: ${e.message}"
            return
        }
        recording = true
        viewModelScope.launch {
            while (recording) {
                recordLevel = recorder.sample()
                recordSecs = recorder.elapsedSecs
                delay(100)
            }
        }
    }

    /** ■: stop to a draft you can review (then [then], e.g. send). */
    fun stopRecording(then: (() -> Unit)? = null) {
        if (!recording) return
        recording = false
        voicePreparing = true
        viewModelScope.launch {
            val draft = withContext(Dispatchers.IO) { recorder.stop() }
            voicePreparing = false
            voiceDraft = draft
            if (draft == null) notice = "Too short to send" else then?.invoke()
        }
    }

    fun discardRecording() {
        recording = false
        recorder.discard()
    }

    fun discardVoiceDraft() {
        voiceDraft?.file?.delete()
        voiceDraft = null
    }

    /** Send while recording: stop and send in one tap. */
    fun sendVoiceNow() = stopRecording { sendVoiceDraft() }

    fun sendVoiceDraft() {
        val ref = selection ?: return
        val draft = voiceDraft ?: return
        voiceDraft = null
        act {
            try {
                Core.run("sendVoice", mapOf("chat" to ref, "path" to draft.file.absolutePath, "duration" to draft.durationSecs))
            } finally {
                withContext(Dispatchers.IO) { draft.file.delete() }
            }
            reloadMessages(ref)
        }
    }

    /** Download an offered file; unknown relays ask first (core rule). */
    fun receive(file: FileInfo, approved: Boolean = false) = act {
        val reply = Core.raw("receiveFile", mapOf("fileId" to file.id, "approved" to approved))
        val obj = reply as? kotlinx.serialization.json.JsonObject
        val accepted = (obj?.get("accepted") as? kotlinx.serialization.json.JsonPrimitive)?.content == "true"
        if (!accepted) {
            val servers = (obj?.get("servers") as? kotlinx.serialization.json.JsonArray)
                ?.mapNotNull { (it as? kotlinx.serialization.json.JsonPrimitive)?.content }.orEmpty()
            fileApproval = file to servers
        }
        selection?.let { reloadMessages(it) }
    }

    /** Readable path of a complete file (decrypted into the private cache). */
    suspend fun plainPath(file: FileInfo): String? {
        plainFiles[file.id]?.let { if (File(it).isFile) return it }
        val path = file.path ?: return null
        return runCatching {
            Core.call<String>("plainFile", mapOf("path" to path, "key" to file.key, "nonce" to file.nonce))
        }.getOrNull()?.also { plainFiles[file.id] = it }
    }

    fun openFile(file: FileInfo) = act {
        val path = plainPath(file) ?: return@act run { notice = "The file isn't on this phone" }
        if (Core.call<Boolean>("riskyFile", mapOf("path" to file.name))) {
            error = "Cursus doesn't open apps or scripts someone sent (${file.name})."
            return@act
        }
        if (!Files.open(getApplication(), path, file.name)) notice = "No app on this phone opens ${file.name}"
    }

    fun playVoice(file: FileInfo) = act {
        val path = plainPath(file) ?: return@act
        VoicePlayer.toggle(file.id, path)
    }

    // endregion

    // region Contacts and groups

    /** Members of the group whose page is open. */
    val members = mutableStateListOf<Member>()

    fun loadMembers(chat: Chat) {
        viewModelScope.launch {
            try {
                val list: List<Member> = Core.call("listMembers", mapOf("groupId" to chat.apiId))
                members.clear()
                // Linux order: you first, then by role (owners first), then name.
                members.addAll(
                    list.sortedWith(
                        compareByDescending<Member> { it.isMe }
                            .thenByDescending { roleRank(it.role) }
                            .thenBy { it.name.lowercase() },
                    ),
                )
            } catch (e: Exception) {
                notice = e.message
            }
        }
    }

    private fun roleRank(role: String) = listOf("observer", "member", "moderator", "admin", "owner").indexOf(role)

    /** Change something, then refresh the chat (and its members for groups). */
    private fun change(chat: Chat, method: String, params: Map<String, Any?>, members: Boolean = false) = act {
        Core.run(method, params)
        reloadChats()
        if (members) loadMembers(chat)
    }

    fun setAlias(chat: Chat, alias: String) = change(chat, "setAlias", mapOf("chat" to chat.ref, "alias" to alias.trim()))

    fun setSendReceipts(chat: Chat, on: Boolean?) =
        change(chat, "setChatSettings", mapOf("chat" to chat.ref, "sendReceipts" to on))

    fun setContactFeature(chat: Chat, key: String, allow: String) =
        change(chat, "setContactPrefs", mapOf("chat" to chat.ref, "changes" to mapOf(key to allow)))

    fun setContactTimedTtl(chat: Chat, secs: Long) =
        change(chat, "setContactPrefs", mapOf("chat" to chat.ref, "changes" to mapOf("timedMessages" to "yes", "timedTtl" to secs)))

    /** A picked picture → SimpleX profile data-URL (core rule), then [use] it. */
    fun withProfileImage(uri: Uri, use: (String) -> Unit) = act {
        val file = withContext(Dispatchers.IO) { Files.copyIn(getApplication(), uri, tmp("out")) }
        try {
            use(Core.call<String>("encodeProfileImage", mapOf("path" to file.absolutePath)))
        } finally {
            withContext(Dispatchers.IO) { file.parentFile?.deleteRecursively() }
        }
    }

    fun setOwnImage(dataUrl: String) = act { user = Core.call("updateProfile", mapOf("image" to dataUrl)) }

    fun setGroupImage(chat: Chat, dataUrl: String) =
        change(chat, "updateGroupProfile", mapOf("groupId" to chat.apiId, "image" to dataUrl))

    fun updateGroupProfile(chat: Chat, name: String? = null, shortDescr: String? = null, welcome: String? = null) =
        change(
            chat, "updateGroupProfile",
            buildMap {
                put("groupId", chat.apiId)
                name?.let { put("name", it.trim()) }
                shortDescr?.let { put("shortDescr", it.trim()) }
                welcome?.let { put("welcome", it.trim()) }
            },
        )

    fun setGroupFeature(chat: Chat, key: String, on: Boolean, ttl: Long? = null) =
        change(chat, "setGroupPref", mapOf("groupId" to chat.apiId, "key" to key, "on" to on, "ttl" to ttl))

    fun setMemberReview(chat: Chat, on: Boolean) = change(chat, "setMemberReview", mapOf("groupId" to chat.apiId, "on" to on))

    suspend fun groupLink(chat: Chat, role: String = "member", quiet: Boolean = false): ConnLink? = try {
        Core.call("groupLink", mapOf("groupId" to chat.apiId, "role" to role))
    } catch (e: Exception) {
        if (!quiet) error = e.message
        null
    }

    fun setGroupLinkRole(chat: Chat, role: String) = change(chat, "setGroupLinkRole", mapOf("groupId" to chat.apiId, "role" to role))

    fun deleteGroupLink(chat: Chat) = change(chat, "deleteGroupLink", mapOf("groupId" to chat.apiId))

    fun addMember(chat: Chat, contact: Chat, role: String) =
        change(chat, "addMember", mapOf("groupId" to chat.apiId, "contactId" to contact.apiId, "role" to role), members = true)

    fun setMemberRole(chat: Chat, m: Member, role: String) =
        change(chat, "setMemberRole", mapOf("groupId" to chat.apiId, "memberId" to m.id, "role" to role), members = true)

    fun removeMember(chat: Chat, m: Member, withMessages: Boolean) =
        change(chat, "removeMember", mapOf("groupId" to chat.apiId, "memberId" to m.id, "withMessages" to withMessages), members = true)

    fun blockMember(chat: Chat, m: Member, blocked: Boolean) =
        change(chat, "blockMember", mapOf("groupId" to chat.apiId, "memberId" to m.id, "blocked" to blocked), members = true)

    fun setMemberShowMessages(chat: Chat, m: Member, show: Boolean) =
        change(chat, "setMemberShowMessages", mapOf("groupId" to chat.apiId, "memberId" to m.id, "show" to show), members = true)

    fun acceptMember(chat: Chat, m: Member, role: String) =
        change(chat, "acceptMember", mapOf("groupId" to chat.apiId, "memberId" to m.id, "role" to role), members = true)

    /** Open (or create) the direct chat with a member. */
    fun messageMember(chat: Chat, m: Member, opened: (String) -> Unit) = act {
        val ref = m.contactId?.let { "@$it" } ?: Core.call<String>("memberContact", mapOf("groupId" to chat.apiId, "memberId" to m.id))
        reloadChats()
        opened(ref)
    }

    fun leaveGroup(chat: Chat) = act {
        Core.run("leaveGroup", mapOf("groupId" to chat.apiId))
        reloadChats()
    }

    fun updateOwnProfile(displayName: String? = null, shortDescr: String? = null, description: String? = null) = act {
        user = Core.call(
            "updateProfile",
            buildMap {
                displayName?.let { put("displayName", it.trim()) }
                shortDescr?.let { put("shortDescr", it.trim()) }
                description?.let { put("description", it.trim()) }
            },
        )
    }

    // endregion

    // region Connect

    suspend fun createInvitation(): ConnLink? = try {
        Core.call("createInvitation")
    } catch (e: Exception) {
        error = e.message; null
    }

    suspend fun addressLink(): ConnLink? = try {
        Core.call("addressLink")
    } catch (e: Exception) {
        error = e.message; null
    }

    fun connect(link: String, done: () -> Unit = {}) = act {
        Core.run("connect", mapOf("link" to link))
        notice = "Connecting… the chat appears in your list."
        reloadChats()
        done()
    }

    /** A link tapped in a message: SimpleX links connect, others open if safe. */
    suspend fun prepareLink(url: String): PreparedLink =
        runCatching { Core.call<PreparedLink>("prepareLink", mapOf("url" to url, "clean" to prefs.cleanLinks)) }
            .getOrDefault(PreparedLink())

    // endregion

    private fun act(block: suspend () -> Unit) {
        viewModelScope.launch {
            try {
                block()
            } catch (e: Exception) {
                error = e.message ?: e.toString()
            }
        }
    }
}
