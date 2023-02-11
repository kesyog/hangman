#!/usr/bin/sh 

TMPFILE=$(mktemp /tmp/hangman.XXXXXX.zip)

# We'll assume the SoftDevice S113 is already flashed

cargo objcopy --release -- -O ihex out.hex

# Use SWD
nrfjprog --program out.hex --verify --sectorerase --reset

# Use DFU via bootloader
# nrfutil pkg generate --debug-mode --sd-req 0x00 --sd-id 0x0102 --hw-version 52 \
#   --application out.hex $TMPFILE
# 
# # If you need to re-flash the SoftDevice
# # nrfutil pkg generate --debug-mode --sd-req 0x00 --sd-id 0x0102 --hw-version 52 \
# #  --application out.hex --softdevice <path to softdevice.hex> $TMPFILE
# 
# # USB port is hard-coded 😬
# nrfutil dfu usb-serial -p /dev/ttyACM0 -pkg $TMPFILE -b 115200
rm $TMPFILE out.hex
