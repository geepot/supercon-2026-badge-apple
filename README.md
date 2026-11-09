# Badge Apple

*Bad Apple!!* on the [Hackaday Supercon 2026 Billboard Badge](https://github.com/Hack-a-Day/2026-Billboard_Badge), full screen, at 4 frames per second on a black/white/red e-paper panel whose own refresh takes 33 seconds.

The badge's panel is a Pervasive Displays **E2969JS0B1**: 960×672 pixels, black/white/red, driven by two chip-on-glass controllers. The stock waveform is built for static signage. Badge Apple drives it with custom DC-balanced change-only waveforms, tuned controller settings, and a firmware pipeline that does almost all of its work while the panel is busy, plus a few things learned the hard way.

**Result, measured on the badge:** each full-screen frame takes a 236 ms panel drive plus ~11–22 ms of upload. At 4 fps about 1.5% of frames are skipped, so the panel shows ~3.9 new frames per second. **[How it got there →](docs/optimizations.md)**

## What it does

- Boots straight into Bad Apple and loops forever, with a balanced black/white cleanup between passes.
- Any button pauses (the panel's high-voltage booster switches off) and resumes.
- Logs a timing summary every 40 frames over USB serial.

## Build

You need Rust, the ESP Rust toolchain, Python 3 with ffmpeg on the PATH, and a copy of the video. The video is not included; see [Third-party notices](THIRD_PARTY_NOTICES.md).

```sh
cargo install espup ldproxy espflash --locked
espup install --targets esp32s3
. "$HOME/export-esp.sh"

python3 scripts/bad-apple.py path/to/bad-apple.mp4   # writes assets/video/bad-apple.bin (git-ignored)
cargo test --locked                                    # host tests: waveforms, uploads, decoder
./scripts/firmware.sh build
./scripts/firmware.sh flash --port /dev/cu.usbmodemXXXX
```

The converter scales the video to cover 960×672, centre-crops it, thresholds it to black and white, and run-length codes it. At the default `--fps 4` that is 1296 frames in about 4 MB of flash. Without the data file the firmware still builds; on the badge it logs that no video is present and stops.

The first build downloads ESP-IDF v5.4.4 and needs several GB. Flashing replaces whatever firmware is on the badge (8 MB flash layout, octal PSRAM); to go back, flash the [original badge firmware](https://github.com/Hack-a-Day/2026-Billboard_Badge). If the badge does not respond, hold BOOT while plugging it in to enter download mode. `python3 scripts/console.py --port … --seconds 60` captures the serial log without resetting the badge.

## Panel care

Fast waveforms are hard on e-paper if done carelessly. Every table the firmware loads is checked for DC balance before use. An earlier, unbalanced waveform burned a pattern into this panel (it faded with rest). Each pass ends with a balanced cleanup to white. Running a normal full refresh (any stock firmware image) now and then keeps the film healthy, and so does storing the badge on a white screen.

## Layout

| Path | Contents |
| --- | --- |
| `crates/badge` | `badge-apple-core`: frame layout, waveforms and DC-balance check, upload-window planner, video decoder (host-tested) |
| `firmware/src` | `badge-apple`: playback loop and display driver (ESP-IDF, Rust std) |
| `firmware/components/pdi` | C++ bridge to the panel: SPI DMA, OTP initialization, session and pull control, Pervasive's vendored driver |
| `scripts` | `bad-apple.py` (video converter), `firmware.sh` (build/flash), `console.py` (serial log) |
| `docs` | [Optimizations](docs/optimizations.md), [hardware](docs/hardware.md) |

## License

Badge Apple's own code and documentation are [MIT](LICENSE). Vendored and adapted third-party code keeps its own terms; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). In particular, the OTP initialization adapted from Pervasive's PDLS stays CC BY-SA 4.0, and the vendored Pervasive configuration header is non-commercial.
