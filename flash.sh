#!/usr/bin/sh 

TMPFILE=$(mktemp /tmp/hangman.XXXXXX.zip)

# Assumes that SoftDevice S113 is already flashed
cargo objcopy --release -- -O ihex out.hex
nrfutil pkg generate --debug-mode --sd-req 0x00 --sd-id 0x0102 --hw-version 52 \
  --application out.hex $TMPFILE

# If you need to re-flash the SoftDevice
# nrfutil pkg generate --debug-mode --sd-req 0x00 --sd-id 0x0102 --hw-version 52 --application <path to hex> --softdevice <path to SoftDevice hex> $TMPFILE

# USB port is hard-coded 😬
nrfutil dfu usb-serial -p /dev/ttyACM0 -pkg $TMPFILE -b 115200
rm $TMPFILE
