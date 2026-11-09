// C ABI over BadgePanel, plus the Arduino-style SPI/GPIO shim the vendored
// driver expects, on ESP-IDF hardware SPI with double-buffered DMA.
#include "bridge.h"
#include "differential_panel.h"
#include "driver/gpio.h"
#include "driver/spi_master.h"
#include "esp_attr.h"
#include "esp_rom_sys.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include <algorithm>
#include <cstring>
#include <new>

namespace {
constexpr gpio_num_t gpio(uint8_t n) { return static_cast<gpio_num_t>(n); }
constexpr int64_t FACTORY_TIMEOUT_US = 120'000'000, PULL_TIMEOUT_US = 3'000'000;
spi_device_handle_t device = nullptr;
BadgePanel *panel = nullptr;
esp_err_t fault = ESP_OK;
int64_t busy_since = 0, busy_limit_us = FACTORY_TIMEOUT_US, pull_started = 0;
// Double-buffered DMA: one buffer transmits while the other fills.
DMA_ATTR uint8_t tx[2][4096];
int filling = 0;
size_t used = 0;
bool in_flight = false;
spi_transaction_t queued = {};
// One dummy byte, then 128 OTP bytes; padded to a whole word for DMA.
constexpr size_t OTP_READ = 129;
DMA_ATTR uint8_t otp_rx[132];
// Task waiting for a BUSY rising edge, woken by the GPIO interrupt.
TaskHandle_t volatile busy_waiter = nullptr;

void IRAM_ATTR busy_edge(void *) {
    BaseType_t woken = pdFALSE;
    if (busy_waiter) vTaskNotifyGiveFromISR(busy_waiter, &woken);
    portYIELD_FROM_ISR(woken);
}

void remember(esp_err_t result) { if (fault == ESP_OK) fault = result; }
void wait_sent() {
    if (!in_flight) return;
    spi_transaction_t *done = nullptr;
    remember(spi_device_get_trans_result(device, &done, portMAX_DELAY));
    in_flight = false;
}
// Queue the filled buffer and switch to the other one; at most one in flight.
void send_async() {
    if (used && fault == ESP_OK) {
        wait_sent();
        queued = {};
        queued.length = used * 8;
        queued.tx_buffer = tx[filling];
        remember(spi_device_queue_trans(device, &queued, portMAX_DELAY));
        in_flight = fault == ESP_OK;
        filling ^= 1;
    }
    used = 0;
}
// All data out before any CS/DC/RST edge, delay or BUSY read.
void flush() { send_async(); wait_sent(); }
void idle() {
    wait_sent();
    used = 0;
    gpio_set_level(gpio(pin::CS_M), 1); gpio_set_level(gpio(pin::CS_S), 1);
    gpio_set_level(gpio(pin::DC), 0); gpio_set_level(gpio(pin::RESET), 0);
}
// Common entry check; also arms the BUSY timeout for this call.
esp_err_t usable(int64_t busy_limit) {
    if (!panel || !panel->is_configured()) return ESP_ERR_INVALID_STATE;
    busy_since = 0;
    busy_limit_us = busy_limit;
    return fault;
}
}

BadgeSPI SPI;
void BadgeSPI::transfer(uint8_t byte) {
    if (fault != ESP_OK) return;
    tx[filling][used++] = byte;
    if (used == sizeof(tx[0])) send_async();
}
void BadgeSPI::write(const uint8_t *data, size_t len) {
    while (len && fault == ESP_OK) {
        const size_t n = std::min(len, sizeof(tx[0]) - used);
        std::memcpy(tx[filling] + used, data, n);
        used += n; data += n; len -= n;
        if (used == sizeof(tx[0])) send_async();
    }
}
void pinMode(uint8_t pin, int mode) {
    if (fault != ESP_OK || pin == 0xff) return;
    remember(gpio_set_direction(gpio(pin), mode == OUTPUT ? GPIO_MODE_OUTPUT : GPIO_MODE_INPUT));
}
void digitalWrite(uint8_t pin, int level) {
    flush(); // CS stays low across DMA chunks; edges wait for the data.
    if (fault == ESP_OK) remember(gpio_set_level(gpio(pin), level));
}
int digitalRead(uint8_t) {
    flush();
    if (fault != ESP_OK) return HIGH; // Unwinds driver wait loops; writes are suppressed.
    // Register before sampling, so an edge in between leaves a pending notification.
    busy_waiter = xTaskGetCurrentTaskHandle();
    if (gpio_get_level(gpio(pin::BUSY_M)) && gpio_get_level(gpio(pin::BUSY_S))) {
        busy_waiter = nullptr;
        busy_since = 0;
        return HIGH;
    }
    const auto now = esp_timer_get_time();
    if (!busy_since) busy_since = now;
    if (now - busy_since >= busy_limit_us) {
        busy_waiter = nullptr;
        remember(ESP_ERR_TIMEOUT);
        return HIGH;
    }
    // Sleep until a BUSY line rises; 10 ms fallback in case an edge is missed.
    ulTaskNotifyTake(pdTRUE, pdMS_TO_TICKS(10));
    busy_waiter = nullptr;
    return LOW;
}
void delay(uint32_t ms) {
    flush();
    if (fault == ESP_OK) vTaskDelay(std::max<TickType_t>(1, pdMS_TO_TICKS(ms)));
}
void badge_delay_us(uint32_t us) {
    flush();
    if (fault != ESP_OK || !us) return;
    if (us >= 1000) delay(us / 1000);
    if (us % 1000) esp_rom_delay_us(us % 1000);
}

extern "C" int badge_display_init(void) {
    if (panel) return ESP_ERR_INVALID_STATE;
    gpio_config_t outputs = {};
    outputs.pin_bit_mask = (1ULL << pin::CS_M) | (1ULL << pin::CS_S) | (1ULL << pin::DC) |
                           (1ULL << pin::RESET) | (1ULL << pin::POWER);
    outputs.mode = GPIO_MODE_OUTPUT;
    remember(gpio_config(&outputs));
    gpio_set_level(gpio(pin::CS_M), 1); gpio_set_level(gpio(pin::CS_S), 1);
    gpio_set_level(gpio(pin::POWER), 0); // The original firmware's level.
    gpio_set_level(gpio(pin::RESET), 0);
    gpio_config_t inputs = {};
    inputs.pin_bit_mask = (1ULL << pin::BUSY_M) | (1ULL << pin::BUSY_S);
    inputs.mode = GPIO_MODE_INPUT;
    inputs.intr_type = GPIO_INTR_POSEDGE;
    remember(gpio_config(&inputs));
    remember(gpio_install_isr_service(0));
    remember(gpio_isr_handler_add(gpio(pin::BUSY_M), busy_edge, nullptr));
    remember(gpio_isr_handler_add(gpio(pin::BUSY_S), busy_edge, nullptr));
    spi_bus_config_t bus = {};
    bus.mosi_io_num = 47; bus.miso_io_num = -1; bus.sclk_io_num = 21;
    bus.quadwp_io_num = -1; bus.quadhd_io_num = -1;
    bus.max_transfer_sz = sizeof(tx[0]);
    remember(spi_bus_initialize(SPI2_HOST, &bus, SPI_DMA_CH_AUTO));
    spi_device_interface_config_t config = {};
    // 80 MHz / 3: above Pervasive's 20 MHz write rating, clean on this badge.
    config.clock_speed_hz = 26'666'667;
    config.mode = 0; config.spics_io_num = -1; config.queue_size = 2;
    remember(spi_bus_add_device(SPI2_HOST, &config, &device));
    if (fault != ESP_OK) { idle(); return fault; }
    const pins_t pins = {pin::BUSY_M, pin::DC, pin::RESET, pin::CS_M, pin::CS_S, 0xff, 0xff, 0xff, 0xff};
    panel = new (std::nothrow) BadgePanel(eScreen_EPD_969, pins);
    if (!panel) remember(ESP_ERR_NO_MEM);
    return fault;
}

// PDLS_EXT3_Basic_Global ca37a598 COG_LargeCJ_getDataOTP, DRIVER_B: master CS
// only, command B9, wait 5 ms, data high, one dummy byte, 128 data bytes.
// ESP-IDF's 3-wire mode turns MOSI around for input on a temporary device.
extern "C" int badge_display_read_otp(uint8_t *raw128) {
    if (!panel || !raw128) return ESP_ERR_INVALID_ARG;
    if (fault != ESP_OK) return fault;
    panel->otp_reset(); flush();
    if (fault != ESP_OK) { idle(); return fault; }

    spi_device_handle_t probe = nullptr;
    spi_device_interface_config_t config = {};
    config.clock_speed_hz = 50'000;
    config.mode = 0; config.spics_io_num = -1; config.queue_size = 1;
    config.flags = SPI_DEVICE_3WIRE | SPI_DEVICE_HALFDUPLEX | SPI_DEVICE_NO_DUMMY;
    remember(spi_bus_add_device(SPI2_HOST, &config, &probe));
    if (fault != ESP_OK) { idle(); return fault; }

    digitalWrite(pin::CS_S, HIGH); // Never let both COGs drive the shared data line.
    digitalWrite(pin::DC, LOW); digitalWrite(pin::CS_M, LOW);
    spi_transaction_t command = {};
    command.flags = SPI_TRANS_USE_TXDATA;
    command.length = 8; command.tx_data[0] = 0xb9;
    remember(spi_device_polling_transmit(probe, &command));
    delay(5);
    digitalWrite(pin::DC, HIGH);
    spi_transaction_t receive = {};
    receive.rxlength = OTP_READ * 8;
    receive.rx_buffer = otp_rx;
    if (fault == ESP_OK) remember(spi_device_polling_transmit(probe, &receive));
    digitalWrite(pin::CS_M, HIGH);
    remember(spi_bus_remove_device(probe));
    if (fault == ESP_OK) std::copy(otp_rx + 1, otp_rx + OTP_READ, raw128);
    idle();
    return fault;
}

extern "C" int badge_display_use_otp(const uint8_t *raw128) {
    if (!panel || !raw128) return ESP_ERR_INVALID_ARG;
    return panel->configure_otp(raw128) ? ESP_OK : ESP_ERR_NOT_SUPPORTED;
}

extern "C" int badge_display_refresh(const uint8_t *bl, const uint8_t *rl,
                                     const uint8_t *br, const uint8_t *rr) {
    if (!bl || !rl || !br || !rr) return ESP_ERR_INVALID_ARG;
    if (const auto err = usable(FACTORY_TIMEOUT_US)) return err;
    panel->close();
    panel->update(bl, rl, br, rr);
    flush(); idle();
    return fault;
}

extern "C" int badge_display_open(const uint8_t *left, const uint8_t *right, const uint8_t *lut) {
    if (!left || !right || !lut) return ESP_ERR_INVALID_ARG;
    if (const auto err = usable(PULL_TIMEOUT_US)) return err;
    panel->open(left, right, lut); flush();
    return fault;
}

extern "C" int badge_display_window(int cog, const uint8_t *plane, int x0, int x1, int y0, int y1) {
    if (!plane || (cog != 1 && cog != 2) ||
        x0 < 0 || x1 >= 60 || x0 > x1 || y0 < 0 || y1 >= 672 || y0 > y1) return ESP_ERR_INVALID_ARG;
    if (const auto err = usable(PULL_TIMEOUT_US)) return err;
    panel->window(cog, plane, x0, x1, y0, y1); flush();
    return fault;
}

extern "C" int badge_display_pull_start(void) {
    if (const auto err = usable(PULL_TIMEOUT_US)) return err;
    pull_started = esp_timer_get_time();
    panel->start_pull(); flush();
    return fault;
}

extern "C" int badge_display_pull_wait(uint32_t *pull_ms) {
    if (!pull_ms) return ESP_ERR_INVALID_ARG;
    if (const auto err = usable(PULL_TIMEOUT_US)) return err;
    panel->ready();
    *pull_ms = static_cast<uint32_t>((esp_timer_get_time() - pull_started) / 1000);
    return fault;
}

extern "C" int badge_display_sleep(void) {
    if (!panel) return ESP_ERR_INVALID_STATE;
    panel->close(); flush(); idle();
    return fault;
}
