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

#endif
