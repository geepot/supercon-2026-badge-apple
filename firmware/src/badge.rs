//! Bad Apple player: boots straight into looping full-screen playback.
use crate::display::Display;
use anyhow::Result;
use badge_apple_core::{
    frame::Frame,
    refresh::fast_lut,
    video::{Video, DATA},
};
use esp_idf_svc::{
    hal::{
        gpio::{PinDriver, Pull},
        peripherals::Peripherals,
    },
    sys,
};
use std::{
    thread,
    time::{Duration, Instant},
};

pub fn run() -> Result<()> {
    // esp_reset_reason_t: 1 power-on, 4 panic, 5-7 watchdog, 9 brownout, 11 USB/flasher.
    log::info!("reset reason={}", unsafe { sys::esp_reset_reason() });
    let video = Video::parse(DATA)?;
    let pins = Peripherals::take()?.pins;
    // All five buttons (-, +, A, B, BOOT): active low. Any press toggles pause.
    let buttons = [
        PinDriver::input(pins.gpio40, Pull::Up)?,
        PinDriver::input(pins.gpio15, Pull::Up)?,
        PinDriver::input(pins.gpio39, Pull::Up)?,
        PinDriver::input(pins.gpio16, Pull::Up)?,
        PinDriver::input(pins.gpio0, Pull::Up)?,
    ];
    let mut was_down = false;
    let mut pressed = move || {
        let down = buttons.iter().any(|b| b.is_low());
        let edge = down && !was_down;
        was_down = down;
        edge
    };
    let mut display = Display::new()?;
    let mut frame = Frame::default();
    let mut next = Frame::default();
    let lut = fast_lut();
    let period = Duration::from_millis(video.period_ms);
    log::info!("bad apple: {} frames, {:?} each", video.len, period);
    loop {
        video.draw(&mut frame, 0)?;
        let mut plan = None;
        let mut skipped = 0;
        let mut start = Instant::now();
        let mut i = 0;
        while i < video.len {
            if pressed() {
                log::info!("paused at frame {i}");
                display.sleep()?; // Booster off while paused.
                while !pressed() {
                    thread::sleep(Duration::from_millis(20));
                }
                start = Instant::now() - period * i as u32;
            }
            let in_flight = display.begin(&frame, &lut, plan.take())?;
            // While the panel drives (~236 ms): decode the likely next frame
            // (~7 ms) and plan its upload windows, so the next begin only
            // sends bytes.
            let guess = (i + 1 < video.len).then_some(i + 1);
            if let Some(guess) = guess {
                video.draw(&mut next, guess)?;
            }
            let mut next_plan = match (in_flight, guess) {
                (true, Some(_)) => display.plan(&next, &frame),
                _ => None,
            };
            if in_flight {
                display.finish(&frame)?;
            }
            if i <= 1 {
                // Frame 0 may be a 33 s factory sync and frame 1 opens the
                // session: start the clock once both are on screen.
                start = Instant::now() - period * i as u32;
            }
            // Show a late frame at once; skip only when a whole period behind.
            let due = (start.elapsed().as_millis() / period.as_millis()) as usize;
            let n = (i + 1).max(due);
            if n >= video.len {
                break;
            }
            if Some(n) != guess {
                skipped += n - i - 1;
                video.draw(&mut next, n)?;
                next_plan = None; // Planned for a different frame.
            }
            std::mem::swap(&mut frame, &mut next);
            plan = next_plan;
            i = n;
            if i % 40 == 0 {
                display
                    .stats
                    .report(&format!("frames to {i}, {skipped} skipped"));
            }
            thread::sleep((start + period * i as u32).saturating_duration_since(Instant::now()));
        }
        log::info!(
            "pass done; balanced cleanup to white; free internal heap={}",
            unsafe { sys::heap_caps_get_free_size(sys::MALLOC_CAP_INTERNAL) }
        );
        display.erase(&mut frame)?;
    }
}
