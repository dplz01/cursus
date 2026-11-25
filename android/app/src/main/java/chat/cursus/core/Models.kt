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

/** A one-time link the user created and nobody used yet
 *  (`cursus_core::Invitation`); [incognito] = they'll see a random profile. */
@Serializable
data class Invitation(
    val link: ConnLink = ConnLink(),
    val connId: Long = 0,
    val incognito: Boolean = false,
    val alias: String = "",
    val createdAt: String = "",
)

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
    val memberRequest: Boolean = false,
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
    /** Seconds after which this chat's messages are deleted here (null: the default). */
    val chatItemTtl: Long? = null,
    /** Lists this chat is on (`ChatTag.id`). */
    val tagIds: List<Long> = emptyList(),
    val contactPrefs: ContactPrefs? = null,
    val groupPrefs: GroupPrefs? = null,
    val memberReview: Boolean = false,
    val canEditProfile: Boolean = false,
    val canManageMembers: Boolean = false,
    val canModerate: Boolean = false,
    /** Unread support messages: members needing attention (moderators) /
     *  your own "Chat with admins" (members). */
    val membersRequireAttention: Long = 0,
    val supportUnread: Long = 0,
    val assignableRoles: List<String> = emptyList(),
) {
    val isContact get() = type == "direct"
    val isGroup get() = type == "group"
    val isNotes get() = type == "local"
    val muted get() = notifications == "none"
    val isRequest get() = pendingRequestId != null
    /** The yellow support counter (Linux `support_badge_count`). */
    val supportBadge get() = if (canModerate) membersRequireAttention else supportUnread
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
    /** Small inline picture the sender embedded (shown until downloaded). */
    val preview: String? = null,
)

/** A chat list ("Add to list"). */
@Serializable
data class ChatTag(val id: Long, val text: String = "", val emoji: String? = null)

/** Live transfer progress of one file. */
@Serializable
data class FileProgress(val fileId: Long, val transferred: Long = 0, val total: Long = 0)

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
    /** Tombstone text from core ("marked deleted", "hidden", …). */
    val deletedLabel: String? = null,
    /** Text still stored: "Reveal" applies (marked, hidden, blocked). */
    val revealable: Boolean = false,
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
    val progress: FileProgress? = null,
    val incoming: List<Incoming> = emptyList(),
)

/** Reply of `createChannel`. */
@Serializable
data class CreatedChannel(val ref: String, val link: String? = null)

/** Reply of `receiveFile`: not accepted = its [servers] need the user's OK. */
@Serializable
data class ReceiveReply(val accepted: Boolean = true, val servers: List<String> = emptyList())

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
data class ContactFeature(
    val mine: String = "no",
    val contact: String = "no",
    val enabled: Boolean = false,
    /** Shared wording (cursus_core::preferences): `Off` / `On` / `Always`. */
    val mineLabel: String? = null,
    /** e.g. "Alice allows this" — replaces the coloured peer glyph. */
    val contactSummary: String? = null,
    /** Choice-sheet footer: summary + "It works only when you both allow it." */
    val contactNote: String? = null,
)

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
data class MessageInfo(
    val time: String? = null,
    val status: String? = null,
    /** Plain words for [status] from core ("Delivered"). */
    val statusLabel: String? = null,
    val text: String? = null,
    /** Your group message: who received it, sorted and headed by core. */
    val receipts: List<MemberReceipt> = emptyList(),
)

@Serializable
data class MemberReceipt(
    val memberId: Long,
    val name: String = "Member",
    val image: String? = null,
    /** `received` / `pending` / `failed`. */
    val state: String = "pending",
    /** Section heading for [state] ("Received by"). */
    val heading: String = "",
)

@Serializable
data class EmojiHit(val shortcode: String, val glyph: String)

@Serializable
data class CallRecord(
    val itemId: Long, val contactId: Long, val peerName: String = "Contact",
    val outgoing: Boolean = false, val status: String = "", val durationSecs: Long = 0, val ts: String? = null,
) {
    val missed: Boolean get() = !outgoing && status in setOf("missed", "pending")
}
