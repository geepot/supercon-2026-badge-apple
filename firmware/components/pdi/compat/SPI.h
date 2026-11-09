#pragma once
#include <cstddef>
#include <cstdint>
constexpr int MSBFIRST = 1;
constexpr int SPI_MODE0 = 0;
struct SPISettings {
    uint32_t frequency = 8000000;
    int order = MSBFIRST;
    int mode = SPI_MODE0;
};
struct BadgeSPI {
    void begin() {}
    void beginTransaction(SPISettings) {}
    void transfer(uint8_t byte);
    void write(const uint8_t *data, size_t len); // Bulk copy into the DMA buffers.
};
extern BadgeSPI SPI;
