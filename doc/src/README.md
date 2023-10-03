# Hangman

<p align="center">
  <img src ="../../boards/proto1_0/assembled.jpg" width="600" alt="Assembled prototype P0.0 unit">
</p>

Hangman is a Bluetooth-enabled crane scale compatible with the custom [Tindeq Progressor Bluetooth service][API],
which allows it to be used with compatible tools like the Tindeq mobile app.

The hardware retrofits a cheap (~$23) 150kg crane scale from [Amazon][Amazon scale] with a custom
PCB based around a Nordic nRF52 microcontroller and a differential ADC. The firmware uses [Embassy][Embassy],
an embedded async framework written in Rust, as well as Nordic's SoftDevice Bluetooth stack.

## Why?

Crane scales have become popular in the climbing community as a means to train and rehab fingers.
This is a fun project to learn and practice various concepts I was unfamiliar or rusty with: BLE
101, async Rust on embedded, nRF52 development, SMT soldering and PCB design, etc. Maybe it'll even
help my fingers get stronger.

## Status

The scale is feature-complete. Weight measurement works great with the Tindeq mobile app. Battery
life is guesstimated to be in the range of several months to a couple of years depending on usage.

## Disclaimer

This is not an officially supported Google product. Wouldn't that be funny though?

This has no affiliation with Tindeq.

[Amazon scale]: https://www.amazon.com/dp/B07MTFXSJW
[API]: https://tindeq.com/progressor_api/
[Embassy]: https://embassy.dev/
