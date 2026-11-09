//! Bad Apple: run-length 1-bit frames from scripts/bad-apple.py. The data
//! file is git-ignored and embedded by build.rs. Black runs are written
//! straight into the B1 plane bytes: whole bytes are filled, only the two
//! edge bytes of a row segment masked.
use crate::frame::{Frame, HALF_ROW, HEIGHT, WIDTH};
use anyhow::{ensure, Context, Result};

pub static DATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/bad-apple.bin"));

pub struct Video<'a> {
    data: &'a [u8],
    pub period_ms: u64,
    pub len: usize,
}

fn u16_at(data: &[u8], at: usize) -> usize {
    u16::from_le_bytes([data[at], data[at + 1]]).into()
}

impl<'a> Video<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        ensure!(
            data.len() >= 12 && &data[..4] == b"BAV1",
            "no Bad Apple data; run scripts/bad-apple.py VIDEO and rebuild"
        );
        ensure!(
            (u16_at(data, 4), u16_at(data, 6)) == (WIDTH, HEIGHT),
            "video must be {WIDTH}x{HEIGHT}"
        );
        let video = Self {
            data,
            period_ms: u16_at(data, 8) as u64,
            len: u16_at(data, 10),
        };
        ensure!(video.len > 0, "video has no frames");
        ensure!(data.len() >= video.base(), "truncated frame table");
        Ok(video)
    }

    fn base(&self) -> usize {
        12 + 4 * (self.len + 1)
    }

    fn offset(&self, i: usize) -> usize {
        let at = 12 + 4 * i;
        u32::from_le_bytes(self.data[at..at + 4].try_into().unwrap()) as usize + self.base()
    }

    pub fn draw(&self, frame: &mut Frame, index: usize) -> Result<()> {
        ensure!(index < self.len, "frame {index} out of range");
        let runs = self
            .data
            .get(self.offset(index)..self.offset(index + 1))
            .context("truncated frame")?;
        let [black_left, red_left, black_right, red_right] = frame.planes_mut();
        for plane in [&mut *black_left, red_left, &mut *black_right, red_right] {
            plane.fill(0);
        }
        let (mut pos, mut black, mut bytes) = (0, false, runs.iter());
        while let Some(run) = varint(&mut bytes)? {
            ensure!(pos + run <= WIDTH * HEIGHT, "frame {index} overruns");
            let end = pos + run;
            while black && pos < end {
                // One segment per row half: [x0, x1) within one controller.
                let (y, x) = (pos / WIDTH, pos % WIDTH);
                let half = WIDTH / 2;
                let x1 = (x / half * half + half).min(x + end - pos);
                let plane = if x < half {
                    &mut *black_left
                } else {
                    &mut *black_right
                };
                let row = &mut plane[y * HALF_ROW..(y + 1) * HALF_ROW];
                set_black(row, x % half, (x1 - 1) % half + 1);
                pos += x1 - x;
            }
            pos = end;
            black = !black;
        }
        ensure!(pos == WIDTH * HEIGHT, "frame {index} is short");
        Ok(())
    }
}

/// Set pixels [x0, x1) in one B1 half-row (MSB-first, see `frame`).
fn set_black(row: &mut [u8], x0: usize, x1: usize) {
    let (first, last) = (x0 / 8, (x1 - 1) / 8);
    let head = 0xffu8 >> (x0 % 8);
    let tail = 0xffu8 << (7 - (x1 - 1) % 8);
    if first == last {
        row[first] |= head & tail;
    } else {
        row[first] |= head;
        row[first + 1..last].fill(0xff);
        row[last] |= tail;
    }
}

fn varint(bytes: &mut std::slice::Iter<u8>) -> Result<Option<usize>> {
    let mut value = 0;
    for shift in (0..35).step_by(7) {
        let Some(&b) = bytes.next() else {
            ensure!(shift == 0, "truncated run");
            return Ok(None);
        };
        value |= usize::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Ok(Some(value));
        }
    }
    anyhow::bail!("run length too long")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference pixel setter, independent of `set_black`.
    fn set(frame: &mut Frame, x: usize, y: usize) {
        let side = usize::from(x >= WIDTH / 2) * 2;
        frame.planes_mut()[side][y * HALF_ROW + (x % (WIDTH / 2)) / 8] |= 0x80 >> (x % 8);
    }

    /// Same encoding as scripts/bad-apple.py, built from a frame.
    fn encode(frame: &Frame) -> Vec<u8> {
        let (mut out, mut black, mut run) = (Vec::new(), false, 0usize);
        let push = |mut n: usize, out: &mut Vec<u8>| loop {
            out.push((n & 0x7f) as u8 | if n > 0x7f { 0x80 } else { 0 });
            n >>= 7;
            if n == 0 {
                break;
            }
        };
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                if frame.is_black(x, y) != black {
                    push(run, &mut out);
                    (black, run) = (!black, 0);
                }
                run += 1;
            }
        }
        push(run, &mut out);
        out
    }

    #[test]
    fn decodes_runs_into_planes_like_the_renderer_and_rejects_bad_streams() {
        let mut source = Frame::default();
        // Edge cases: byte edges, single pixels, the COG boundary, runs
        // that wrap rows, the first and last pixel.
        for (x, y, w, h) in [
            (0, 0, 1, 1),
            (3, 2, 1, 1),
            (5, 3, 11, 4),
            (470, 10, 20, 3),
            (950, 20, 10, 2),
            (480, 30, 8, 1),
            (100, 100, 760, 200),
            (959, 671, 1, 1),
        ] {
            for py in y..y + h {
                for px in x..x + w {
                    set(&mut source, px, py);
                }
            }
        }
        let frames = [encode(&source), encode(&Frame::default())];
        let mut data = b"BAV1".to_vec();
        for v in [WIDTH, HEIGHT, 333, 2] {
            data.extend_from_slice(&(v as u16).to_le_bytes());
        }
        for offset in [0, frames[0].len(), frames[0].len() + frames[1].len()] {
            data.extend_from_slice(&(offset as u32).to_le_bytes());
        }
        data.extend(frames.concat());
        let video = Video::parse(&data).unwrap();
        assert_eq!((video.len, video.period_ms), (2, 333));
        let mut frame = Frame::default();
        frame.fill(true);
        frame.planes_mut()[1].fill(0xff); // Stale red must be cleared too.
        video.draw(&mut frame, 0).unwrap();
        assert_eq!(frame.planes(), source.planes());
        video.draw(&mut frame, 1).unwrap();
        assert!(!frame.is_black(959, 671));
        assert!(video.draw(&mut frame, 2).is_err());
        assert!(Video::parse(b"BAV2xxxxxxxxxxxx").is_err());
        // A truncated final run must be rejected rather than drawn partially.
        let mut bad = data.clone();
        bad.truncate(data.len() - 1);
        assert!(Video::parse(&bad).unwrap().draw(&mut frame, 1).is_err());
    }
}
