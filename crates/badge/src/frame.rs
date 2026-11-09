//! The B1 panel's four wire-format planes: master black/red, slave black/red.
//! Each controller drives 480 columns; a row is 60 bytes, pixel x in bit
//! 7 - x % 8 of byte x / 8 (MSB = leftmost), set bit = ink. That is the
//! order the OTP addressing mode expects, so uploads are plain copies.

pub const WIDTH: usize = 960;
pub const HEIGHT: usize = 672;
pub const HALF_ROW: usize = WIDTH / 16;
pub const PLANE_LEN: usize = HALF_ROW * HEIGHT;

pub struct Frame {
    // Heap allocation avoids a 161 KB stack frame.
    planes: [Vec<u8>; 4],
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            planes: std::array::from_fn(|_| vec![0; PLANE_LEN]),
        }
    }
}

impl Frame {
    pub fn planes(&self) -> [&[u8]; 4] {
        std::array::from_fn(|i| self.planes[i].as_slice())
    }

    pub fn planes_mut(&mut self) -> [&mut [u8]; 4] {
        self.planes.each_mut().map(|plane| plane.as_mut_slice())
    }

    /// All black or all white; red planes are always clear.
    pub fn fill(&mut self, black: bool) {
        for (i, plane) in self.planes.iter_mut().enumerate() {
            plane.fill(if black && i % 2 == 0 { 0xff } else { 0 });
        }
    }

    pub fn is_black(&self, x: usize, y: usize) -> bool {
        assert!(x < WIDTH && y < HEIGHT);
        let side = usize::from(x >= WIDTH / 2) * 2;
        self.planes[side][y * HALF_ROW + (x % (WIDTH / 2)) / 8] & (0x80 >> (x % 8)) != 0
    }
}
