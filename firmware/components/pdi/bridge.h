#pragma once
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
// All calls return an esp_err_t. After a fault (e.g. a BUSY timeout) every
// call returns it again: a failed panel needs a reset, not more HV cycles.
// Plane pointers are borrowed for the call; a plane is 60 x 672 bytes.
int badge_display_init(void);
// 128 raw OTP bytes from the master COG. No refresh.
int badge_display_read_otp(uint8_t *raw128);
// Validate and apply an OTP profile; required before anything below.
int badge_display_use_otp(const uint8_t *raw128);
// Factory refresh with the panel's own waveform (~33 s).
int badge_display_refresh(const uint8_t *black_left, const uint8_t *red_left,
                          const uint8_t *black_right, const uint8_t *red_right);
// Open a change-only session: load the 400-byte waveform, booster on, both
// RAMs set to the shown black planes.
int badge_display_open(const uint8_t *left, const uint8_t *right, const uint8_t *lut400);
// Upload screen bytes [x0, x1] (0..59) x rows [y0, y1] of one COG (1 = left).
int badge_display_window(int cog, const uint8_t *plane, int x0, int x1, int y0, int y1);
// Trigger one pull, then wait for both BUSY lines; pull_ms counts from the
// trigger. CPU work may run between the two calls.
int badge_display_pull_start(void);
int badge_display_pull_wait(uint32_t *pull_ms);
// Close the session (booster off); the image stays.
int badge_display_sleep(void);
#ifdef __cplusplus
}
#endif
