/* C ABI of bridge/ (Rust). See bridge/src/lib.rs. */
#ifndef CURSUS_BRIDGE_H
#define CURSUS_BRIDGE_H

#include <stdint.h>

/* Logging + process environment for libsimplex. Call first in main(). */
void cursus_init(void);

/* {"method": "...", "params": {...}} -> {"ok": ...} | {"error": "..."}.
 * Blocks; call off the main thread. Free the result with cursus_free. */
char *cursus_call(const char *request);

/* Next core event as JSON, or NULL after timeout_ms without one. */
char *cursus_next_event(int32_t timeout_ms);

void cursus_free(char *s);

/* 48 kHz mono signed PCM, exactly 960 samples (20 ms).
 * Return 0 on success, -1 on invalid input or codec failure. These run on
 * audio threads; they do not send JSON or wait for signaling. */
int32_t cursus_call_capture(const int16_t *samples, int32_t count);
int32_t cursus_call_playback(int16_t *samples, int32_t count);

#endif
