#pragma once
#include "EPD_Driver.h"
#include "otp_profile.h"
#include <initializer_list>

// Badge wiring (GPIO numbers).
namespace pin {
constexpr uint8_t BUSY_M = 12, BUSY_S = 8, DC = 10, RESET = 11, CS_M = 13, CS_S = 3, POWER = 48;
}

// E2969JS0B1 ("B1") driver on top of Pervasive's vendored EPD_Driver, which
// supplies SPI framing and DC/DC shutdown. Initialization, addressing and the
// DC/DC ramp come from the panel's own OTP (adapted from PDLS ca37a598); the
// change-only session sequence follows supercon26-badge-os c7b84ca. The bridge
// drives nothing until configure_otp has accepted a profile.
class BadgePanel : public EPD_Driver {
public:
    using EPD_Driver::EPD_Driver;
    bool configure_otp(const uint8_t *raw) { return configured = otp.load(raw); }
    bool is_configured() const { return configured; }
    // PDLS COG_LargeCJ_reset timing; no DC/DC start, so nothing is driven.
    void otp_reset() { close(); reset(200, 20, 200, 200, 5); }
    // The bridge's digitalRead reports HIGH only when both BUSY lines are
    // high, and blocks until a BUSY edge (or 10 ms) when they are not.
    void ready() { while (digitalRead(pin::BUSY_M) != HIGH) {} }
    void close() {
        if (session) { ready(); _DCDC_softShutdown(); session = false; }
    }
    // Factory refresh: the panel's own full-colour waveform (~33 s).
    void update(const uint8_t *bl, const uint8_t *rl, const uint8_t *br, const uint8_t *rr) {
        reset(200, 20, 200, 200, 5); dctl();
        images(bl, rl, br, rr);
        start(200, 10, 100, 100, 0x82); // 25 C bank.
        soft_start(); _displayRefresh(); _DCDC_softShutdown();
    }
    // Open a change-only session with `lut` loaded; both RAMs start at the
    // shown frame. The session (booster on) stays open until close().
    void open(const uint8_t *left, const uint8_t *right, const uint8_t *lut) {
        close();
        reset(0, 2, 4, 20, 5); dctl();
        // Bank = 0x50 + 2 * degC selects the OTP temperature settings. 31 C
        // and up scan ~18% faster (236 vs 288 ms per 10-frame pull at 26 C);
        // our table replaces the bank's pulses either way.
        start(50, 1, 2, 10, 0x8e);
        _sendIndexData(0xb0, lut, 400); // After the strobes, before the booster.
        soft_start(); delay(90); ready();
        session = true;
        images(left, left, right, right);
    }
    // Upload screen bytes [x0, x1] x rows [y0, y1] of one COG's black plane.
    void window(int cog, const uint8_t *plane, int x0, int x1, int y0, int y1) {
        ready();
        write_ram(cog, 0x10, plane, x0, x1, y0, y1);
    }
    // The controller drives on its own after 0x15, so the caller can do CPU
    // work before waiting for BUSY. Always the full-height data window:
    // narrowed scans leave copies lower on the panel (docs/optimizations.md).
    void start_pull() {
        ready();
        _sendIndexData(0x13, otp.raw.data() + 0x15, 6);
        both(0x15, {0x3c}); delay(5);
    }

private:
    bool session = false;
    bool configured = false;
    B1OtpProfile otp;

    void reset(uint32_t ms1, uint32_t ms2, uint32_t ms3, uint32_t ms4, uint32_t ms5) {
        _reset(ms1, ms2, ms3, ms4, ms5);
        both(0x70, {0x71, 0x36}); // B1 booster configuration (badge errata).
    }
    void dctl() { both(0x01, {otp.raw[0x10]}); } // Before any image upload.
    void start(int reset_wait, int settle, int strobe_on, int strobe_off, uint8_t bank) {
        both(0x05, {0x7d}); delay(reset_wait); both(0x05, {0}); delay(settle);
        both(0xd8, {otp.raw[0x1c]});
        both(0xd6, {otp.raw[0x1d]});
        both(0xa7, {0x10}); delay(strobe_on); both(0xa7, {0}); delay(strobe_off);
        // DRIVER_B leaves the oscillator (0x03) at its reset value.
        for (int cog = 1; cog <= 2; ++cog) {
            send(cog, 0x44, {0}); send(cog, 0x45, {0x80});
            send(cog, 0xa7, {0x10}); delay(strobe_on);
            send(cog, 0xa7, {0}); delay(strobe_off);
            send(cog, 0x44, {6}); send(cog, 0x45, {bank});
            send(cog, 0xa7, {0x10}); delay(strobe_on);
            send(cog, 0xa7, {0}); delay(strobe_off);
        }
        both(0x60, {otp.raw[0x0b]});
        send(1, 0x61, {otp.raw[0x1b]});
        both(0x02, {otp.raw[0x11]});
    }
    void soft_start() {
        otp.ramp([this](uint8_t reg, uint8_t a, uint8_t b, int size) {
            const uint8_t data[] = {a, b}; _sendIndexData(reg, data, size);
        }, [](uint32_t us) { badge_delay_us(us); });
    }
    // Whole planes: RAM 0x10 gets black (new), 0x11 red (old).
    void images(const uint8_t *bl, const uint8_t *rl, const uint8_t *br, const uint8_t *rr) {
        _sendIndexData(0x90, otp.raw.data() + 0x0c, 4);
        const uint8_t *planes[] = {bl, rl, br, rr};
        for (int i = 0; i < 4; ++i) write_ram(i / 2 + 1, 0x10 + i % 2, planes[i], 0, 59, 0, 671);
    }
    void write_ram(int cog, uint8_t ram, const uint8_t *plane, int x0, int x1, int y0, int y1) {
        const auto duw = B1OtpProfile::window(x0, x1, y0, y1);
        const auto cursor = otp.cursor(x0, y0);
        send(cog, 0x13, {duw[0], duw[1], duw[2], duw[3], duw[4], duw[5]});
        send(cog, 0x12, {cursor[0], cursor[1], cursor[2]});
        const uint8_t cs = cog == 1 ? pin::CS_M : pin::CS_S;
        digitalWrite(pin::CS_M, HIGH); digitalWrite(pin::CS_S, HIGH);
        digitalWrite(pin::DC, LOW); digitalWrite(cs, LOW); SPI.transfer(ram);
        digitalWrite(cs, HIGH); digitalWrite(pin::DC, HIGH); digitalWrite(cs, LOW);
        for (int y = y0; y <= y1; ++y) SPI.write(plane + y * 60 + x0, x1 - x0 + 1);
        digitalWrite(cs, HIGH);
    }
    void both(uint8_t index, std::initializer_list<uint8_t> data) {
        _sendIndexData(index, data.begin(), data.size());
    }
    void send(int cog, uint8_t index, std::initializer_list<uint8_t> data) {
        if (cog == 1) _sendIndexDataM(index, data.begin(), data.size());
        else _sendIndexDataS(index, data.begin(), data.size());
    }
};
