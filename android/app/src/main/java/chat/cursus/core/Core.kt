package chat.cursus.core

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.JsonArray

/** Error text from the Rust core (already user-readable). */
class CoreException(message: String) : Exception(message)

val CoreJson = Json {
    ignoreUnknownKeys = true
    explicitNulls = false
    coerceInputValues = true
}

/** Calls into cursus-core. Every call blocks inside libsimplex, so it runs on IO. */
object Core {
    suspend fun raw(method: String, params: Map<String, Any?> = emptyMap()): JsonElement =
        withContext(Dispatchers.IO) { callBlocking(method, params) }

    suspend inline fun <reified T> call(method: String, params: Map<String, Any?> = emptyMap()): T =
        CoreJson.decodeFromJsonElement(raw(method, params))

    /** A method whose result is not needed. */
    suspend fun run(method: String, params: Map<String, Any?> = emptyMap()) {
        raw(method, params)
    }

    fun callBlocking(method: String, params: Map<String, Any?>): JsonElement {
        val request = buildJsonObject {
            put("method", method)
            put("params", toJson(params))
        }
        val reply = CoreJson.parseToJsonElement(Native.call(request.toString())).jsonObject
        reply["error"]?.let { if (it is JsonPrimitive) throw CoreException(it.content) }
        return reply["ok"] ?: JsonNull
    }

    private fun toJson(value: Any?): JsonElement = when (value) {
        null -> JsonNull
        is JsonElement -> value
        is String -> JsonPrimitive(value)
        is Number -> JsonPrimitive(value)
        is Boolean -> JsonPrimitive(value)
        is Map<*, *> -> JsonObject(value.entries.associate { (k, v) -> k.toString() to toJson(v) })
        is Iterable<*> -> JsonArray(value.map { toJson(it) })
        else -> JsonPrimitive(value.toString())
    }
}
