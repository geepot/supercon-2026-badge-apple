#!/usr/bin/env python3
"""Capture the badge's USB serial log without resetting it into ROM mode.

Requires pyserial.
"""
import argparse
import sys
import time

import serial
from serial.tools import list_ports

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--port", help="USB port; auto-detects a single ESP32 USB Serial/JTAG device if omitted")
parser.add_argument("--seconds", type=float, default=20)
parser.add_argument("--log", help="append captured serial bytes to this file")
args = parser.parse_args()
if not args.port:
    devices = [port.device for port in list_ports.comports()
               if (port.vid, port.pid) == (0x303A, 0x1001)]
    if len(devices) != 1:
        parser.error("could not select one ESP32 USB device; specify --port")
    args.port = devices[0]
    print(f"Using {args.port}")

connection = serial.Serial(port=None, baudrate=115200, timeout=0.2)
connection.port = args.port
# Open without the DTR/RTS pattern that resets the chip into download mode.
connection.dtr = True
connection.rts = False
connection.open()
connection.dtr = False
log = open(args.log, "ab") if args.log else None
try:
    deadline = time.monotonic() + args.seconds
    while time.monotonic() < deadline:
        data = connection.read(4096)
        if data:
            if log:
                log.write(data)
                log.flush()
            sys.stdout.write(data.decode(errors="replace"))
            sys.stdout.flush()
finally:
    if log:
        log.close()
    connection.close()
