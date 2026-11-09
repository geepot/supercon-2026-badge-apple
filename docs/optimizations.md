# Making a 33-second e-paper panel play video

How Badge Apple gets full-screen *Bad Apple!!* to 4 fps on the Supercon 2026 Billboard Badge. All numbers were measured on one badge (ESP32-S3, Pervasive Displays E2969JS0B1, 960×672 black/white/red, two COGs) at room temperature.

| Stage | Full-screen update | Video rate |
| --- | --- | --- |
| Panel's own (OTP) waveform | ~33 s | — |
| Community change-only table, 8 MHz SPI | ~0.6 s | ~1.5 fps possible |
| Badge Apple, first video build | ~0.7 s | 1 fps |
| Badge Apple now | **236 ms drive + ~11–22 ms upload** | **4 fps, ~1.5% skipped** |

Every frame costs a fixed **panel drive** (the controller running the waveform) plus whatever the CPU and SPI do outside it. The drive is set by the waveform and the controller's scan rate. Everything else is overhead to hide or remove.

## 1. Change-only waveforms instead of the panel's own

The panel's built-in waveform redraws every pixel through a long flashing sequence that includes the slow red pigment. The controller accepts a custom 400-byte waveform table at register `0xB0`. It has five tables × ten groups × `[level, on, off, ph2, on, off, ph2, repeat]`, and each pixel selects its table from the bits in the controller's two image memories.

Following [supercon26-badge-os](https://github.com/yigitabi5444/supercon26-badge-os) and [phthallo/supercon](https://github.com/phthallo/supercon), memory `0x10` holds the new frame and `0x11` the old one. A pixel's table is then `(old << 1) | new`: white→white, white→black, black→white or black→black. Only the two changing tables drive. Unchanged pixels and VCOM stay at ground, so a frame redraws only what moved, with no flashing, in black and white.

**The video table.** A changing pixel gets one frame on the opposite rail (a "preparation" kick that loosens the particles), then nine frames on its target rail. Drawing nets +8 frames toward black and erasing −8 toward white.

## 2. DC balance, enforced, because a pattern burned in once

An early waveform (phthallo's "maintenance" table) also nudged *unchanged* pixels toward their own color on every update: black cells got VDH and the white background got VDL, with nothing pushing back. After a few hundred generations of Conway's Life, the board was burned into the film. It survived factory refreshes and showed red tint, and it faded only after rest.

Electrophoretic film must see equal drive in both directions over time. Badge Apple computes each table's net drive from its bytes: on, off and repeat counts per half and per group, +1 per VDH frame and −1 per VDL frame. It refuses to load any table unless unchanged pixels and VCOM net zero and erasing exactly cancels drawing (`refresh::dc_balanced`). Each pixel's charge is then bounded by one transition however long the video runs. Each pass of the video ends with two balanced black/white rounds back to white.

## 3. The waveform floor: ten one-frame groups

Each of the ten groups must last at least one frame. A group with zero length does not skip; it hung the controller (BUSY never released within 30 s). Other projects use two-frame groups (`[1,0,1,1,0,1]`, two one-frame halves), so 20 frames per update.

Each group has two halves, and each half has its own repeat count (`ph2`). Our older table already skipped its first half with `ph2 = 0`, and the two-frame layout uses one-frame halves. Combining them, `[1,1,0,1,0,1]`, gives **one frame per group: 10 frames instead of 20**. Measured: 290 ms per update instead of 578 ms. This is the shortest table the controller accepts.

## 4. Faster frames: the temperature bank

A frame is one scan of the panel's rows. At the default settings it takes about 43 µs per row, so 29 ms for all 672 rows. Several registers that sound like frame-rate controls were timed with the panel safely idle. Both RAMs held the same image and every level byte was cleared, so the scan ran without driving any pixel:

| Register | Values tried | Effect on frame time |
| --- | --- | --- |
| Oscillator `0x03` (never written on B1) | 10 values, `0x00`–`0x81` | none |
| TCON `0x60` (OTP `0x25`) | 8 values | none |
| DCTL `0x01` bits | every single-bit change | none, except bit 1: it "scans" fast but draws nothing |
| **Temperature bank `0x45`** = `0x50` + 2 × °C | 0–87 °C | **yes** |

The bank selects the controller's temperature-compensated settings. Warm film responds faster, so the controller scans faster:

| Bank temperature | 0 °C | 10 °C | 15–20 °C | 25–30 °C | **31 °C and up** |
| --- | --- | --- | --- | --- | --- |
| 10-frame full-screen drive | 390 ms | 340 ms | 314 ms | 288 ms | **236 ms** |

The speed-up is one step at 31 °C. Hotter banks (up to `0xFE`, ~87 °C) are no faster, so Badge Apple uses `0x8E` (31 °C), the mildest bank that has it. Our own table still replaces the bank's pulses.

## 5. One long session

The controller session, booster included, opens once with the waveform loaded and stays open: each frame is a few window writes and a bare `0x15` trigger. The reference driver reopened its session every 60 updates, which cost ~0.7 s each time and produced a visible hitch. Badge Apple runs the whole video in one session and closes it only on pause or between passes.

## 6. Uploads: only the rows that changed

**Bus.** The original firmware used software SPI. Badge Apple uses hardware SPI with DMA, at **26.7 MHz** (80 MHz ÷ 3, the ESP32-S3's next step above Pervasive's 20 MHz write rating; clean on this badge, 40 MHz not tried). Two 4 KB DMA buffers alternate, one transmitting while the other fills, and every CS/DC edge waits for the data to finish. A full 161 KB image upload went from 186 ms at 8 MHz to 105 ms at 16 MHz, 69 ms at 20 MHz double-buffered, and **52 ms** at 26.7 MHz.

**Which rows.** Only rows that differ are sent, in windows of consecutive changed rows. The obvious model is that each pull swaps memories `0x10`/`0x11`, so `0x10` holds the frame shown before the current one. That turned out wrong: uploads trimmed to that model left glitches at the bottom of the screen. The controller most likely copies new into old instead. Uploads now cover every row that differs from **either** possible state. A host test models both behaviours, including A → B → A pixel flips.

**Window planning.** Each window costs ~0.2 ms of commands, about 450 bytes' worth of wire time at these speeds. A small dynamic program groups changed rows into windows, trading that overhead against sending some unchanged bytes. In simulation over the whole video it cut upload time ~22% against simple bands.

**Bytes in the panel's order.** The panel's OTP addressing mode wants the leftmost pixel in each byte's most significant bit. The frame buffer used to store it in the least significant bit and reversed every byte on its way out, one byte per call. The decoder now writes bits in the panel's order, so each window row is a single `memcpy` into the DMA buffer. Uploads got 13–18% faster on the same frames (calm stretches 13.0 → 11.3 ms, busiest 26.7 → 21.8 ms).

## 7. Decoding: 227 ms to 7 ms

**Format.** The video is stored as LEB128 run lengths over row-major pixels, alternating white and black. Storing PackBits-compressed panel bytes instead came out 2.4× larger (7.3 MB versus 3 MB at 3 fps), because silhouette edges split bytes. At 4 fps the run-length data is 1296 frames in ~4 MB.

**Decoder.** The first decoder drew each black run as a rectangle through a graphics library: 227 ms per frame. It now writes runs straight into the panel's plane layout, filling whole bytes and masking only the two edge bytes of each row segment: **~7 ms**.

## 8. Pipelining: work while the panel drives

The 236 ms drive is idle CPU time. The pull is split into *start* (trigger) and *wait* (BUSY). Between them, Badge Apple decodes the next frame and plans its upload windows. That moves ~9 ms of row comparison over ~240 KB of external RAM, plus the 7 ms decode, entirely off the critical path. The next frame then only has to send bytes.

An earlier version spawned a decode thread per frame instead. Each spawn leaked ~150 bytes of internal RAM, and after a few thousand frames the display worker stopped (red LEDs, frozen screen). The start/wait split needs no threads at all.

## 9. Waking on BUSY

The end of a pull used to be noticed by polling BUSY with 1 ms sleeps, about 1–2 ms late. A GPIO interrupt on the rising edge now wakes the waiting task directly (with a 10 ms fallback in case an edge is missed). Pulls measure a steady 236 ms instead of 237 ms with spikes to 239 ms.

## 10. Scheduling

Frames are due at fixed times (every 250 ms at 4 fps). A frame that is a little late is shown immediately, and frames are skipped only when playback falls a whole period behind. Small overruns therefore turn into rare skips instead of one skip per late frame. Per-frame logging was replaced by a summary every 40 frames, because USB logging costs time and can stall when nothing is reading.

## Where the time goes now

| Per full-screen frame | Time |
| --- | --- |
| Panel drive (10 frames at the 31 °C bank) | 236 ms |
| Upload, calm stretches / busy stretches (mean) | ~11 / ~16–22 ms |
| Upload, busiest frames | up to ~30 ms |
| Decode + window planning | hidden in the drive |

The busiest frames run slightly over 250 ms. The scheduler absorbs short bursts of them; over the first 400 frames, which include the busiest scenes, 6 frames (1.5%) were skipped. Faster still would need a shorter drive (no lever found) or a faster bus (40 MHz is twice the rating).

## Things that did not help

- **Narrow scans for full-screen video.** Narrowing the refresh window `0x90` *and* the data window `0x13` to rows *first*–*last* makes a pull proportionally shorter. But each waveform frame clocks the panel's row-select chain (*last* + 1) times and never clears it. Leftover tokens therefore drive copies of the window (*last* + 1) rows lower: windows starting at row 0 tile the whole screen, and narrowing only the end does the same. It is clean only when **first + last + 1 ≥ 672**, i.e. for windows in the lower half of the screen. That makes it useful for small UIs, but Bad Apple's silhouettes reach the bottom row in most frames, and the copies were exactly the stale patches seen at the bottom. Full-screen frames always scan all 672 rows.
- **Scanning from the first changed row.** A scan that ends at the bottom row always satisfies that rule, so a pull could start at the first row that changed. Simulated over the whole video, it cuts the mean drive only from 236 to 216 ms, and the median stays at 234 ms, because most frames change something near the top. Not worth reintroducing narrowed scans for.
- **Hysteresis in the encoder** (only flip a pixel when it clearly crosses the threshold): <1% fewer bytes. *Bad Apple* is already nearly binary.
- **Uploading during the drive.** It would hide uploads completely, but the controller selects each pixel's transition from RAM as it scans. A write landing mid-scan could drive a pixel toward a color it already has, which is one-sided drive. Not attempted.
- **Hotter temperature banks, oscillator and TCON:** see section 4.

## Credits

Built on the work of the badge community: [Hack-a-Day/2026-Billboard_Badge](https://github.com/Hack-a-Day/2026-Billboard_Badge), [yigitabi5444/supercon26-badge-os](https://github.com/yigitabi5444/supercon26-badge-os) (change-only session and old/new table layout), [phthallo/supercon](https://github.com/phthallo/supercon) (preparation frame, persistent booster), and [TheESTest/Supercon_2026_Badge_Firmware](https://github.com/TheESTest/Supercon_2026_Badge_Firmware) (scan-time measurements and narrowed data windows). Panel initialization comes from Pervasive Displays' PDLS library. Details and terms are in [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md).
