package chat.cursus.core

/**
 * The Rust side (`android/jni` → `bridge/` → `cursus-core` → libsimplex).
 * Same JSON API as the macOS app; see bridge/src/methods.rs for methods.
 */
object Native {
    init {
        System.loadLibrary("cursus_android")
    }

    /** Once per process, before any other call. */
    external fun init(dataDir: String)

    /** `{"method": …, "params": {…}}` → `{"ok": …}` or `{"error": "…"}`. Blocks. */
    external fun call(request: String): String

    /** Next core event as JSON, or null after [timeoutMs] without one. Blocks. */
    external fun nextEvent(timeoutMs: Int): String?
}
