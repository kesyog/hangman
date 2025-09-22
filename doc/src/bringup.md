# Bringup 

How to get the board from being assembled to being functional, using Windows, WSL and a ST-Link

## Requirements on Windows ##
- Working WSL2
- ST-Link, with installed drivers (I used a ST-Link V2-1, borrowed from a Nucleo Board)
- Install usbipd-win on windows: run `winget install --interactive --exact dorssel.usbipd-win` in a windows terminal
- Install required packages in wsl: `sudo apt install build-essential`
- Install rust: https://rustup.rs/
- Install probe-rs: https://probe.rs/
- Install OpenOCD in WSL: `sudo apt install openocd` in wsl
- Download the SoftDevice for the chip from NRF (I used s113_nrf52_7.3.0_softdevice.hex)


## Flashing on Windows ##
- Connect the ST-Link
- Follow these instructions to get the ST-Link connected to WSL: https://learn.microsoft.com/de-de/windows/wsl/connect-usb
 - `usbipd list`
 - `usbipd bind --busid <busid>`
 - `usbipd attach --wsl --busid <busid>`
- Connect the board to the ST-Link correctly
- Power the board
- Run `openocd -f /usr/share/openocd/scripts/interface/stlink.cfg -f /usr/share/openocd/scripts/target/nrf52.cfg`
- In a seperate terminal run `telnet localhost 4444`
- Use `targets` you can check that you can see the nrf52 target
- Run `init`, `halt`, `nrf5 mass_erase`, `program /full/path/.../s113_nrf52_7.3.0_softdevice.hex verify`
- Run `exit`in telnet shell
- Exit OpenOCD with CTRL+C
- Build and flash the code with `cargo run --bin proto1_0 --release`
- Done! Further flashing etc. should not require reflashing the SoftDevice

### Notes:
- If you just want to flash the softdevice in one command you can do it like this: `openocd -f /usr/share/openocd/scripts/interface/stlink.cfg -f /usr/share/openocd/scripts/target/nrf52.cfg -c "init; halt; nrf5 mass_erase; program /full/path/.../s113_nrf52_7.3.0_softdevice.hex preverify verify; shutdown"`

### Sources:
- https://www.jentsch.io/nrf51822-flashen-mit-st-link-v2-und-openocd/
- https://github.com/seemoo-lab/openhaystack/wiki/Flashing-nRF-with-OpenOCD---ST-Link
- https://github.com/lupyuen/stm32bluepill-mynewt-sensor/blob/nrf52/scripts/nrf52/flash-boot.sh
- https://www.youtube.com/watch?v=R5wub5ywzTU&t=197s
- https://www.reddit.com/r/embedded/comments/1k9v4vr/stm32_cmake_stlinkv3pwr_on_windows_in_2025/?show=original
- https://github.com/dorssel/usbipd-win/wiki/WSL-support
- https://discuss.ardupilot.org/t/help-setting-up-openocd-with-st-link-in-wsl2/112271
- https://hackmd.io/@aeefs2Y8TMms-cjTDX4cfw/r1fqAa_Da
- https://learn.microsoft.com/de-de/windows/wsl/connect-usb


