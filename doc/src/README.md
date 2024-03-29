# Hangman

<p align="center">
  <img src ="assets/assembled.jpg" width="600" alt="Assembled prototype P1.0 unit">
</p>

Hangman is a Bluetooth-enabled crane scale. It's intended use is as a climbing training and rehab
tool, but it can be used anywhere that requires measuring force or weight.

The hardware retrofits a cheap (~$23) 150kg crane scale from [Amazon][Amazon scale] with a custom
PCB based around a Nordic nRF52 microcontroller and a differential ADC. The firmware uses [Embassy][Embassy],
an embedded async framework written in Rust, as well as Nordic's SoftDevice Bluetooth stack.

## Why?

Crane scales have become popular in the climbing community as a means to train and rehab fingers.
This is a fun project to learn and practice various concepts I was unfamiliar or rusty with: BLE
101, async Rust on embedded, nRF52 development, SMT soldering and PCB design, etc. Maybe it'll even
help my fingers get stronger.

## Status

The scale is feature-complete. Weight measurement works great with the [Progressor API][API] and
compatible tools. Battery life is guesstimated to be in the range of several months to a couple of
years depending on usage.

## Disclaimer

This is not an officially supported Google product. Wouldn't that be funny though?

This has no affiliation with Tindeq.

[Amazon scale]: https://www.amazon.com/dp/B07MTFXSJW
[API]: https://tindeq.com/progressor_api/
[Embassy]: https://embassy.dev/
