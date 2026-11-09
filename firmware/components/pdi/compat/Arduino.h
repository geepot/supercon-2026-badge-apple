#pragma once
// Only the Wiring surface used by the vendored driver, implemented in bridge.cpp.
#include <cstdint>
constexpr int INPUT = 0;
constexpr int OUTPUT = 1;
constexpr int LOW = 0;
constexpr int HIGH = 1;
void pinMode(uint8_t pin, int mode);
void digitalWrite(uint8_t pin, int level);
int digitalRead(uint8_t pin);
void delay(uint32_t ms);
void badge_delay_us(uint32_t us);
