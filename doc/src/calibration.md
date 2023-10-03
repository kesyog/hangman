# Calibration

Hangman uses a two-point calibration routine where one of the points is zero. Calibration needs to
be run once before use. The generated calibration constants are saved across power cycles so there
shouldn't be a need to be calibrate multiple times, but you can re-calibrate at any time if you feel
like the scale is inaccurate.

## Instructions

1. Install the nRF Connect app or any similar tool that can be used to connect to BLE devices and
write data to GATT characteristics.
1. Wake up Hangman by pressing the power button.
1. Connect to Hangman using nRF Connect. It'll be named something starting with `Progressor`.
1. Hang "zero" weight from the scale. It's okay if this isn't actually zero. What's important is
that you know the difference in weight between this stage and the second calibration point. Any
deviation from zero at this point will be tared out later.
1. Write the ByteArray `6900000000` to the `7e4e1703-1ea6-40c9-9dcc-13d34ffead57` GATT
characteristic. This should be the only writable characteristic. This sends the 0x69
(`AddCalibrationPoint`) opcode along with 0.0 as a 32-bit float.
1. Add a known reference weight to the scale, ideally something that's at or more than the expected
maximum weight but less than 150kg, the maximum capacity of the scale.
1. Convert the known weight, in kg, to a 32-bit floating point number in little-endian format. Write
`69 <your hex bytes here>` to the same characteristic as earlier. As an example, if your known
weight were 100.0 kg, you would send `690000f042`.
1. If you mess up entering in either meaurement, feel free to resend the corresponding command.
1. Once you're set, write `0x6A` to the same characteristic to save the calibration.
1. At this point, disconnect from Hangman and test it out using the Tindeq mobile app or something
compatible.

## Tips

* If the measurements are wildly off after calibration, try re-calibrating and using a big-endian
float. Different programs disagree on how these bytes should be entered 🤷
* The two calibration points can be written in any order. However, it's probably a little better to
write the zero point first, in case there is some hysteresis.
* 0x69 is the `AddCalibrationPoint` opcode.
* 0x6A is the `SaveCalibration` opcode.
