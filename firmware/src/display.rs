use anyhow::Result;
use badge_apple_core::{
    frame::Frame,
    refresh::{cleanup_lut, dc_balanced, Window},
};
use esp_idf_svc::sys;
use std::time::Instant;

// firmware/components/pdi/bridge.h
extern "C" {
    fn badge_display_init() -> i32;
    fn badge_display_read_otp(raw128: *mut u8) -> i32;
    fn badge_display_use_otp(raw128: *const u8) -> i32;
    fn badge_display_refresh(bl: *const u8, rl: *const u8, br: *const u8, rr: *const u8) -> i32;
    fn badge_display_open(left: *const u8, right: *const u8, lut: *const u8) -> i32;
    fn badge_display_window(cog: i32, plane: *const u8, x0: i32, x1: i32, y0: i32, y1: i32) -> i32;
    fn badge_display_pull_start() -> i32;
    fn badge_display_pull_wait(pull_ms: *mut u32) -> i32;
    fn badge_display_sleep() -> i32;
}
fn checked(code: i32) -> Result<()> {
    sys::esp!(code).map_err(Into::into)
}

struct History {
    shown: [Vec<u8>; 2],
    /// The frame shown before `shown`: controller 0x10 holds it if a pull
    /// swaps memories, or `shown` if it copies; plans cover both.
    ram: [Vec<u8>; 2],
}

/// B1 panel: one factory refresh to establish a known frame, then change-only
/// pulls in a single controller session that stays open until `sleep`.
pub struct Display {
    history: Option<History>,
    session: Option<[u8; 400]>,
    pub stats: Stats,
}

/// Upload windows per controller half.
pub struct Plan {
    windows: [Vec<Window>; 2],
}

/// Window overhead in byte-equivalents: ~0.2 ms of commands per window, at
/// ~0.44 us per data byte (measured).
const WINDOW_BYTES: usize = 450;

fn make_plan(next: &Frame, shown: [&[u8]; 2], previous: [&[u8]; 2]) -> Plan {
    let p = next.planes();
    Plan {
        windows: std::array::from_fn(|side| {
            Window::plan(p[side * 2], &[shown[side], previous[side]], WINDOW_BYTES)
        }),
    }
}

/// Per-frame timings, summarized and reset by `report`.
#[derive(Default)]
pub struct Stats {
    frames: u32,
    plan_us: u64,
    upload_us: u64,
    upload_max_us: u64,
    windows: usize,
    pull_ms: u32,
    pull_max_ms: u32,
}

impl Stats {
    fn add(&mut self, planned: std::time::Duration, total: std::time::Duration, windows: usize) {
        let upload = (total - planned).as_micros() as u64;
        self.frames += 1;
        self.plan_us += planned.as_micros() as u64;
        self.upload_us += upload;
        self.upload_max_us = self.upload_max_us.max(upload);
        self.windows += windows;
    }
    fn pull(&mut self, ms: u32) {
        self.pull_ms += ms;
        self.pull_max_ms = self.pull_max_ms.max(ms);
    }
    pub fn report(&mut self, label: &str) {
        let n = self.frames.max(1);
        log::info!(
            "{label}: {} pulls, plan {} us, upload mean {} / max {} us, {} windows, pull mean {} / max {} ms",
            self.frames,
            self.plan_us / u64::from(n),
            self.upload_us / u64::from(n),
            self.upload_max_us,
            self.windows / n as usize,
            self.pull_ms / n,
            self.pull_max_ms
        );
        *self = Self::default();
    }
}

impl Display {
    pub fn new() -> Result<Self> {
        checked(unsafe { badge_display_init() })?;
        // Read OTP twice and refuse to drive the panel unless both agree.
        let mut samples = [[0u8; 128]; 2];
        for raw in &mut samples {
            checked(unsafe { badge_display_read_otp(raw.as_mut_ptr()) })?;
        }
        anyhow::ensure!(
            samples[0] == samples[1],
            "OTP reads differ; refusing panel initialization"
        );
        checked(unsafe { badge_display_use_otp(samples[0].as_ptr()) })?;
        log::info!("display: OTP profile accepted");
        Ok(Self {
            history: None,
            session: None,
            stats: Stats::default(),
        })
    }

    /// Close the controller session (booster off); the shown frame is kept.
    pub fn sleep(&mut self) -> Result<()> {
        if self.session.take().is_some() {
            checked(unsafe { badge_display_sleep() })?;
        }
        Ok(())
    }

    /// Show `frame` with the panel's own (slow, DC-balanced) waveform and
    /// record it as the known state for change-only updates.
    pub fn synchronize(&mut self, frame: &Frame) -> Result<()> {
        self.sleep()?;
        let p = frame.planes();
        let started = Instant::now();
        checked(unsafe {
            badge_display_refresh(p[0].as_ptr(), p[1].as_ptr(), p[2].as_ptr(), p[3].as_ptr())
        })?;
        log::info!("factory refresh: {:?}", started.elapsed());
        self.history = Some(History {
            shown: [p[0].to_vec(), p[2].to_vec()],
            ram: [p[0].to_vec(), p[2].to_vec()],
        });
        Ok(())
    }

    /// Upload `frame` and trigger a change-only pull. `plan` must have been
    /// made by `plan` for this exact frame (or None to plan now). Returns
    /// whether a pull is in flight; then call `finish`. The first frame is
    /// established with a factory refresh instead, and nothing is in flight.
    pub fn begin(&mut self, frame: &Frame, lut: &[u8; 400], plan: Option<Plan>) -> Result<bool> {
        anyhow::ensure!(dc_balanced(lut), "refusing a DC-unbalanced waveform");
        let Some(history) = self.history.as_mut() else {
            self.synchronize(frame)?;
            return Ok(false);
        };
        let p = frame.planes();
        if p[0] == history.shown[0] && p[2] == history.shown[1] {
            return Ok(false);
        }
        let started = Instant::now();
        if self.session != Some(*lut) {
            if self.session.take().is_some() {
                checked(unsafe { badge_display_sleep() })?;
            }
            // Both RAMs start from the confirmed shown frame. A plan made
            // against {shown, previous} still covers this (a superset).
            checked(unsafe {
                badge_display_open(
                    history.shown[0].as_ptr(),
                    history.shown[1].as_ptr(),
                    lut.as_ptr(),
                )
            })?;
            for side in 0..2 {
                history.ram[side].copy_from_slice(&history.shown[side]);
            }
            self.session = Some(*lut);
        }
        let plan = match plan {
            Some(plan) => plan,
            None => {
                let shown = [history.shown[0].as_slice(), history.shown[1].as_slice()];
                let ram = [history.ram[0].as_slice(), history.ram[1].as_slice()];
                make_plan(frame, shown, ram)
            }
        };
        let planned = started.elapsed();
        for (side, windows) in plan.windows.iter().enumerate() {
            for window in windows {
                checked(unsafe {
                    badge_display_window(
                        side as i32 + 1,
                        p[side * 2].as_ptr(),
                        window.x0.into(),
                        window.x1.into(),
                        window.y0.into(),
                        window.y1.into(),
                    )
                })?;
            }
        }
        // Always a full-height scan; see BadgePanel::start_pull.
        checked(unsafe { badge_display_pull_start() })?;
        self.stats.add(
            planned,
            started.elapsed(),
            plan.windows.iter().map(Vec::len).sum(),
        );
        Ok(true)
    }

    /// While the pull of `in_flight` runs: plan `next` against the RAM state
    /// that pull will leave (in_flight shown; current shown one frame back).
    /// Pure CPU work, no display access.
    pub fn plan(&self, next: &Frame, in_flight: &Frame) -> Option<Plan> {
        let history = self.history.as_ref()?;
        let p = in_flight.planes();
        Some(make_plan(
            next,
            [p[0], p[2]],
            [history.shown[0].as_slice(), history.shown[1].as_slice()],
        ))
    }

    /// Wait for the pull of `frame` and record it as shown.
    pub fn finish(&mut self, frame: &Frame) -> Result<()> {
        let mut pull = 0;
        checked(unsafe { badge_display_pull_wait(&mut pull) })?;
        let history = self.history.as_mut().expect("begin established a frame");
        let p = frame.planes();
        // Keep the frame before the shown one: 0x10 holds it if a pull swaps
        // memories (otherwise 0x10 holds the shown frame; plans cover both).
        for side in 0..2 {
            std::mem::swap(&mut history.ram[side], &mut history.shown[side]);
            history.shown[side].copy_from_slice(p[side * 2]);
        }
        self.stats.pull(pull);
        Ok(())
    }

    /// One complete change-only update (begin + finish).
    pub fn update(&mut self, frame: &Frame, lut: &[u8; 400]) -> Result<()> {
        if self.begin(frame, lut, None)? {
            self.finish(frame)?;
        }
        Ok(())
    }

    /// Two balanced black/white rounds back to a clean white, then sleep.
    pub fn erase(&mut self, frame: &mut Frame) -> Result<()> {
        let lut = cleanup_lut();
        for black in [true, false, true, false] {
            frame.fill(black);
            self.update(frame, &lut)?;
        }
        self.sleep()
    }
}
