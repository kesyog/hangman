#!/usr/bin/sh 
# Copyright 2023 Google LLC
# 
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
# 
#     https://www.apache.org/licenses/LICENSE-2.0
# 
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# This is a helper script to flash the nrf52 USB stick developer module

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
