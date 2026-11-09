# Hardware

Hackaday Supercon 2026 Billboard Badge: ESP32-S3-WROOM-1 (8 MB flash, 8 MB octal PSRAM) and a Pervasive Displays **E2969JS0B1** panel (960×672, black/white/red, OTP layout "B1"). Two chip-on-glass controllers each drive 480 columns: master on the left, slave on the right. GPIO numbers below are from the badge's original firmware and PCB sources.

| Signal | GPIO | Use in Badge Apple |
| --- | --- | --- |
| EPD SCK / MOSI | 21 / 47 | SPI2, mode 0, 26.7 MHz, write-only, double-buffered DMA |
| EPD CS left / right | 13 / 3 | Active low, selected per controller |
| EPD BUSY left / right | 12 / 8 | Active low; a pull is done when both are high |
| EPD DC / RESET | 10 / 11 | Data high / reset low |
| EPD power control | 48 | Held at the original firmware's level |
| Buttons − / + | 40 / 15 | Pause/resume (pull-up, active low) |
| User A / B | 39 / 16 | Pause/resume |
| BOOT | 0 | Pause/resume; hold at power-up for download mode |
| USB D− / D+ | 19 / 20 | Native USB serial/JTAG (flashing, logs) |

## Panel initialization

At startup the firmware reads the master controller's 128-byte OTP block twice over three-wire SPI at 50 kHz. It refuses to drive the panel unless both reads match the expected B1 identity and window layout. Display control, data and refresh windows, RAM pointer, TCON, STV direction, MS_SYNC, BVSS, VCOM and the four-stage DC/DC soft-start ramp all come from the OTP, following Pervasive's PDLS DRIVER_B sequence. The B1 booster configuration `0x70 = [71, 36]` is written after reset. The oscillator register is left at its reset value; writing it changed nothing measurable.

With the OTP addressing mode, each controller's RAM runs left to right in screen bytes, and the most significant bit of each byte is the leftmost pixel. The frame buffer is built in exactly that order, so uploads are plain memory copies. BUSY rising edges raise a GPIO interrupt that wakes the waiting task, instead of polling.

## Notes

- Fast (change-only) sessions load waveform bank `0x8E` (31 °C), which scans ~18% faster than the 26 °C bank. Factory refreshes use the stock 25 °C bank `0x82`. The badge has no temperature sensor.
- Do not toggle the panel supply or reset mid-pull. If the controller faults, the firmware stops driving and needs a reset.
- The original firmware's docs mention 8, 16 and 32 MB flash; this build uses an 8 MB layout. Check `espflash board-info` before changing it.
