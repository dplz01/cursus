package chat.cursus.core

import kotlinx.serialization.Serializable
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

// Mirrors the JSON built in bridge/src/dto.rs (the macOS Models.swift does too).

@Serializable
data class AppInfo(
    val setupComplete: Boolean = false,
    val databaseExists: Boolean = false,
    val displayName: String? = null,
    val encryptLocalFiles: Boolean = true,
    val dataDir: String = "",
    val version: String = "",
    val reactions: List<String> = emptyList(),
)

@Serializable
data class User(
    val userId: Long,
    val displayName: String = "",
    val fullName: String = "",
    val shortDescr: String = "",
    val description: String = "",
    val image: String? = null,
    val contactLink: String? = null,
    val sendRcptsContacts: Boolean = true,
    val sendRcptsSmallGroups: Boolean = true,
)

@Serializable
data class ConnLink(val display: String = "", val full: String = "", val short: String? = null)

/** The Connect card's one-time link (`cursus_core::Invitation`): kept by the
 *  core until someone uses it; [incognito] = they'll see a random profile. */
@Serializable
data class Invitation(val link: ConnLink = ConnLink(), val connId: Long = 0, val incognito: Boolean = false)

@Serializable
data class Chat(
    val ref: String,
    val type: String,
    val apiId: Long,
    val title: String = "",
    val preview: String = "",
    val unread: Long = 0,
    val unreadChat: Boolean = false,
    val notifications: String = "all",
    val favorite: Boolean = false,
    val pendingRequestId: Long? = null,
    val image: String? = null,
    val name: String = "",
    val alias: String = "",
    val fullName: String = "",
    val shortDescr: String = "",
    val description: String = "",
    val contactLink: String? = null,
    val isChannel: Boolean = false,
    val role: String? = null,
    val lastTs: String? = null,
    val sendReceipts: Boolean? = null,
    val contactPrefs: ContactPrefs? = null,
    val groupPrefs: GroupPrefs? = null,
    val memberReview: Boolean = false,
    val canEditProfile: Boolean = false,
    val canManageMembers: Boolean = false,
    val canModerate: Boolean = false,
    val assignableRoles: List<String> = emptyList(),
) {
    val isContact get() = type == "direct"
    val isGroup get() = type == "group"
    val isNotes get() = type == "local"
    val muted get() = notifications == "none"
    val isRequest get() = pendingRequestId != null
}

@Serializable
data class Segment(
    val text: String,
    val style: String = "plain",
    val color: String? = null,
    val link: String? = null,
    val simplex: String? = null,
)

@Serializable
data class FileInfo(
    val id: Long,
    val name: String = "",
    val size: Long = 0,
    val state: String = "other",
    val kind: String = "file",
    val duration: Long? = null,
    val path: String? = null,
    val key: String? = null,
    val nonce: String? = null,
    val progress: Double? = null,
    val error: String? = null,
    val autoReceive: Boolean = false,
)

@Serializable
data class Reaction(val emoji: String, val glyph: String, val mine: Boolean = false, val total: Long = 0)

@Serializable
data class Quote(val itemId: Long? = null, val text: String = "", val outgoing: Boolean = false)

@Serializable
data class Sender(
    val memberId: Long? = null,
    val contactId: Long? = null,
    val name: String = "",
    val image: String? = null,
    val role: String? = null,
)

@Serializable
data class GroupInvitation(
    val groupId: Long,
    val name: String = "",
    val role: String? = null,
    val status: String? = null,
    val pending: Boolean = false,
)

@Serializable
data class Message(
    val id: Long,
    val text: String = "",
    val caption: String = "",
    val segments: List<Segment> = emptyList(),
    val outgoing: Boolean = false,
    val createdAt: String? = null,
    val delivery: String = "sent",
    val file: FileInfo? = null,
    val reactions: List<Reaction> = emptyList(),
    val quote: Quote? = null,
    val deleted: String? = null,
    val sender: Sender? = null,
    val announcement: Boolean = false,
    val groupInvitation: GroupInvitation? = null,
    val edited: Boolean = false,
    val canEdit: Boolean = false,
    val emojiOnly: Boolean = false,
    /// Core `runs` rule: this message continues the previous run / the next one continues it.
    val joinedPrev: Boolean = false,
    val joinedNext: Boolean = false,
)

@Serializable
data class Incoming(val chat: String, val title: String = "Cursus", val sender: String? = null, val text: String = "")

@Serializable
data class CoreEvent(
    val type: String = "",
    val refreshChats: Boolean = false,
    val refreshMessages: Boolean = false,
    val chat: String? = null,
    val incoming: List<Incoming> = emptyList(),
)

@Serializable
data class PreparedLink(val url: String? = null, val simplex: String? = null)

/** Core timestamps are ISO-8601 UTC with up to nanoseconds. */
object Stamps {
    private val time = DateTimeFormatter.ofLocalizedTime(FormatStyle.SHORT)
    private val day = DateTimeFormatter.ofPattern("d MMM")
    private val fullDay = DateTimeFormatter.ofLocalizedDate(FormatStyle.MEDIUM)

    fun parse(raw: String?): Instant? = raw?.let { runCatching { Instant.parse(it) }.getOrNull() }

    fun time(raw: String?): String =
        parse(raw)?.atZone(ZoneId.systemDefault())?.format(time) ?: ""

    /** Chat list stamp: time today, else a short date. */
    fun list(raw: String?): String {
        val local = parse(raw)?.atZone(ZoneId.systemDefault()) ?: return ""
        return if (local.toLocalDate() == LocalDate.now()) local.format(time) else local.format(day)
    }

    fun day(raw: String?): LocalDate? = parse(raw)?.atZone(ZoneId.systemDefault())?.toLocalDate()

    fun dayLabel(date: LocalDate): String = when (date) {
        LocalDate.now() -> "Today"
        LocalDate.now().minusDays(1) -> "Yesterday"
        else -> date.format(fullDay)
    }
}

@Serializable
data class ContactFeature(val mine: String = "no", val contact: String = "no", val enabled: Boolean = false)

@Serializable
data class ContactPrefs(val features: Map<String, ContactFeature> = emptyMap(), val timedTtl: Long = 86400)

@Serializable
data class GroupFeature(val on: Boolean = false)

@Serializable
data class GroupPrefs(val features: Map<String, GroupFeature> = emptyMap(), val timedTtl: Long = 86400)

@Serializable
data class SupportInfo(val unread: Long = 0, val attention: Long = 0, val mentions: Long = 0)

@Serializable
data class Member(
    val id: Long,
    val contactId: Long? = null,
    val name: String = "",
    val publicName: String = "",
    val role: String = "member",
    val status: String = "",
    val image: String? = null,
    val shortDescr: String = "",
    val isMe: Boolean = false,
    val blocked: Boolean = false,
    val showMessages: Boolean = true,
    val invited: Boolean = false,
    val pending: Boolean = false,
    val support: SupportInfo? = null,
)

@Serializable
data class MessageInfo(val time: String? = null, val status: String? = null, val text: String? = null)

@Serializable
data class EmojiHit(val shortcode: String, val glyph: String)
