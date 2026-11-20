package chat.cursus.core

internal fun latestReadAnchor(messages: List<Message>, unread: Long): Long? {
    val incoming = messages.filter { !it.outgoing && !it.announcement }
    val firstUnread = if (unread > 0) incoming.takeLast(unread.coerceAtMost(Int.MAX_VALUE.toLong()).toInt()).firstOrNull()?.id else null
    val index = firstUnread?.let { id -> messages.indexOfFirst { it.id == id } } ?: -1
    return if (index >= 0) messages[(index - 1).coerceAtLeast(0)].id else messages.lastOrNull()?.id
}
