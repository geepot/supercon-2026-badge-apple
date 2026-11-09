#pragma once
#include <algorithm>
#include <array>
#include <cstdint>
#include <cstring>

// B1 field layout and ramp expansion adapted from Rei Vilo / Pervasive Displays,
// PDLS_EXT3_Basic_Global ca37a598, Screen_EPD_EXT3.cpp, copyright 2010-2025.
// CC BY-SA 4.0; for exclusive use with Pervasive Displays screens.
// Changes: checked E2969JS0B1 geometry, ramp and transfer adapters.
struct B1OtpProfile {
    std::array<uint8_t, 128> raw{};

    bool load(const uint8_t *data) {
        // Only enable the observed B1 layout. An unknown panel/configuration must
        // not silently drive with an untested addressing or booster sequence.
        constexpr uint8_t duw[] = {0, 0x3b, 0, 0, 0x9f, 2};
        constexpr uint8_t cursor[] = {0, 0, 0x10};
        if (std::memcmp(data + 0x68, "TA2969JS0B1", 11) ||
            std::memcmp(data + 0x15, duw, sizeof(duw)) ||
            std::memcmp(data + 0x12, cursor, sizeof(cursor)) ||
            data[0x0c] != 0 || data[0x0d] != 0x3b || data[0x0e] != 0) return false;
        for (int stage = 0; stage < 4; ++stage) {
            const auto *r = data + 0x28 + stage * 8;
            if (!(r[0] & 0x7f) || (r[0] & 0x7f) > 32) return false;
            if (r[0] & 0x80) {
                if (!delay_us(r[7]) || delay_us(r[7]) > 10000) return false;
            } else if (delay_us(r[3]) > 10000 || delay_us(r[4]) > 10000 ||
                       (!delay_us(r[3]) && !delay_us(r[4]))) return false;
        }
        std::copy(data, data + raw.size(), raw.begin());
        return true;
    }

    static uint32_t delay_us(uint8_t encoded) {
        return (encoded & 0x7f) * (encoded & 0x80 ? 1000u : 10u);
    }

    // With the OTP DCTL/RAM mode, RAM x runs left to right in screen bytes
    // and each byte is MSB = leftmost pixel (the frame layout matches).
    static std::array<uint8_t, 6> window(int x0, int x1, int y0, int y1) {
        return {static_cast<uint8_t>(x0), static_cast<uint8_t>(x1),
                static_cast<uint8_t>(y0), static_cast<uint8_t>(y0 >> 8),
                static_cast<uint8_t>(y1), static_cast<uint8_t>(y1 >> 8)};
    }

    std::array<uint8_t, 3> cursor(int ram_x, int y) const {
        return {static_cast<uint8_t>(ram_x), static_cast<uint8_t>(y),
                static_cast<uint8_t>((raw[0x14] & 0xfc) | (y >> 8))};
    }

    template<class Write, class Wait>
    void ramp(Write write, Wait wait) const {
        for (int stage = 0; stage < 4; ++stage) {
            const auto *r = raw.data() + 0x28 + stage * 8;
            uint8_t phl = r[1], phh = r[2];
            for (int i = 0; i < (r[0] & 0x7f); ++i) {
                if (r[0] & 0x80) {
                    phl += r[3]; phh += r[4]; // uint8_t wrap implements negative deltas.
                    write(0x09, r[5], 0, 1);
                    write(0x51, phl, phh, 2);
                    write(0x09, r[6], 0, 1);
                    wait(delay_us(r[7]));
                } else {
                    write(0x09, r[1], 0, 1); wait(delay_us(r[3]));
                    write(0x09, r[2], 0, 1); wait(delay_us(r[4]));
                }
            }
        }
    }
};
