# Third-party notices

Badge Apple's own code and documentation are licensed under the MIT License (see [LICENSE](LICENSE)). The components below keep their own terms, which take precedence for those files and portions.

## Pervasive Displays panel driver

Source: [PervasiveDisplays/EPD_Driver_GU_large](https://github.com/PervasiveDisplays/EPD_Driver_GU_large), revision `d007460bfd1172caacb2ce4a52710cfeadee5a23`. Vendored under `firmware/components/pdi/vendor/`:

- `EPD_Driver.cpp`, `EPD_Driver.h`: Pervasive Displays' redistribution and disclaimer notice, retained in each file.
- `EPD_Configuration.h`: edition-specific **CC BY-NC-SA** / commercial terms, retained in the file. This file is non-commercial; review its terms before any commercial use.

All three files are unmodified copies of that revision. Badge-specific behaviour (B1 booster setup, OTP initialization, change-only sessions) lives in `differential_panel.h`, and the Arduino-style SPI/GPIO calls are implemented by `bridge.cpp` and `compat/`.

## Pervasive Displays PDLS (OTP initialization)

The B1 OTP read sequence (`firmware/components/pdi/bridge.cpp`), the OTP-driven DRIVER_B initialization and DC/DC ramp (`otp_profile.h`, parts of `differential_panel.h`) and reset timing are adapted from **Rei Vilo / Pervasive Displays**, [PDLS_EXT3_Basic_Global `Screen_EPD_EXT3.cpp`](https://github.com/PervasiveDisplays/PDLS_EXT3_Basic_Global/blob/ca37a598a80e6d77ffb978bdcd2142d808b1269b/src/Screen_EPD_EXT3.cpp), revision `ca37a598a80e6d77ffb978bdcd2142d808b1269b`. Copyright (c) Rei Vilo, 2010–2025; portions (c) Pervasive Displays, 2010–2025. The source declares [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/) and "For exclusive use with Pervasive Displays screens." These portions keep those terms. Changes: the read uses ESP-IDF three-wire SPI, and the observed E2969JS0B1 layout is validated before use.

## Community badge firmware

These projects established the techniques Badge Apple builds on. None had a repository license at the inspected revision. Badge Apple's implementations are its own; attribution and upstream ownership are retained, and no license is asserted over their work.

- [yigitabi5444/supercon26-badge-os](https://github.com/yigitabi5444/supercon26-badge-os), revision `c7b84ca03f51bcaad503441ea75e5b359a5a8f59` (`main/main.c`): the change-only session sequence (quick setup, waveform load at `0xB0`, `0x15` pulls), the old/new memory table layout and window writes. The project has since adopted GPL-2.0.
- [phthallo/supercon](https://github.com/phthallo/supercon), revision `d8a8a07cebd9cb8663cde8e7de280fc5aaf9abcb` (`firmware/lib/luts.py`, `firmware/lib/epd.py`): the opposite-rail preparation frame, two-frame group layout and persistent-booster updates. Badge Apple removed its one-directional unchanged-pixel pulses for DC balance.
- [TheESTest/Supercon_2026_Badge_Firmware](https://github.com/TheESTest/Supercon_2026_Badge_Firmware): measured per-row scan time and narrowed data/refresh windows (documented in `docs/optimizations.md`; not used for full-screen video).
- [Hack-a-Day/2026-Billboard_Badge](https://github.com/Hack-a-Day/2026-Billboard_Badge): the badge hardware, pin mapping and original firmware.

## Rust crates and ESP-IDF

Rust dependencies are resolved in `Cargo.lock` and keep their upstream licenses. ESP-IDF is fetched by `esp-idf-sys` at the version pinned in `.cargo/config.toml`.

## Bad Apple!!

The video is not distributed with this repository. `scripts/bad-apple.py` converts a user-supplied copy into git-ignored frame data. *Bad Apple!!* (feat. nomico) and its shadow-art video belong to their respective creators.
