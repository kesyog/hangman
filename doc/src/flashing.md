# Flashing the firmware

## Prereqs

* Obtain a Segger J-Link debugger
* Download the [nRF Command Line Tools](https://www.nordicsemi.com/Products/Development-tools/nRF-Command-Line-Tools)
* Download the SoftDevice s113 from <https://www.nordicsemi.com/Products/Development-software/S113>

## Instructions

1. Connect your debugger.
1. Flash the SoftDevice. This generally only needs to be done once.

   ```sh
   nrfjprog --family nrf52 --program s113_nrf52_7.2.0_softdevice.hex --chiperase --verify --reset
   ```

   Your .hex filename may differ. Note that Nordic encourages the use of the nRF Util over nrfjprog.
   I've found nrfjprog to be more reliable. YMMV.
1. Flash the hangman firmware:

   ```sh
   DEFMT_LOG=info cargo run --release --bin proto1_0 --features nrf52832
   ```
