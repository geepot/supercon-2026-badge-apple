#!/usr/bin/env python3
"""Convert a video into 1-bit run-length frames for the Bad Apple app.

    python3 scripts/bad-apple.py VIDEO [--fps 4]

Scales to cover the 960x672 panel, centre-crops, thresholds to black/white and
writes assets/video/bad-apple.bin (git-ignored: bring your own video). Format,
little-endian: b"BAV1", width u16, height u16, period_ms u16, count u16,
(count + 1) u32 data offsets, then per frame LEB128 run lengths over row-major
pixels, alternating white/black and starting with white. Pixel runs are ~2.4x
smaller than PackBits over the packed planes (edges split bytes); the badge
decodes them straight into its plane bytes. Needs ffmpeg.
"""
import argparse
import pathlib
import re
import struct
import subprocess

W, H = 960, 672
OUT = pathlib.Path(__file__).resolve().parent.parent / "assets/video/bad-apple.bin"
BLACK = bytes(1 if v < 128 else 0 for v in range(256))


def varint(n):
    out = bytearray()
    while True:
        out.append(n & 0x7F | (0x80 if n > 0x7F else 0))
        n >>= 7
        if not n:
            return out


def encode(frame):
    bits = frame.translate(BLACK)  # 1 = black; regex keeps the run scan in C.
    out = bytearray(varint(0) if bits[0] else b"")  # Streams start with white.
    for run in re.finditer(rb"\x00+|\x01+", bits):
        out += varint(run.end() - run.start())
    return bytes(out)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("video")
    parser.add_argument("--fps", type=float, default=4.0)
    args = parser.parse_args()
    scale = f"fps={args.fps},scale={W}:{H}:force_original_aspect_ratio=increase,crop={W}:{H},format=gray"
    raw = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", args.video, "-vf", scale, "-f", "rawvideo", "-"],
        check=True, capture_output=True).stdout
    frames = [encode(raw[i:i + W * H]) for i in range(0, len(raw) - W * H + 1, W * H)]
    assert 0 < len(frames) < 65536, "no frames decoded"
    offsets = [0]
    for f in frames:
        offsets.append(offsets[-1] + len(f))
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_bytes(b"BAV1" + struct.pack("<4H", W, H, round(1000 / args.fps), len(frames))
                    + struct.pack(f"<{len(offsets)}I", *offsets) + b"".join(frames))
    print(f"{OUT}: {len(frames)} frames, {OUT.stat().st_size / 1024:.0f} KiB")


if __name__ == "__main__":
    main()
