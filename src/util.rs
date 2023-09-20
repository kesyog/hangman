use crate::pac;

pub unsafe fn disable_all_gpio_sense() {
    #[cfg(feature = "nrf52840")]
    {
        let p1 = unsafe { &(*pac::P1::ptr()) };
        for cnf in &p1.pin_cnf {
            cnf.modify(|_, w| w.sense().disabled());
        }
    }
    let p0 = unsafe { &(*pac::P0::ptr()) };
    for cnf in &p0.pin_cnf {
        cnf.modify(|_, w| w.sense().disabled());
    }
}

/// Convert a signed integer in a u32 container to a signed integer
pub const fn convert_signed_to_i32<const BITS: u32>(mut input: u32) -> i32 {
    assert!(input < (1 << BITS), "Out of range");
    // Extend sign bits if negative
    if input & (1 << (BITS - 1)) != 0 {
        input |= u32::MAX & !((1 << BITS) - 1);
    }
    input as i32
}

// TODO: figure out how to actually run these tests on host
// I promise I ran them in the playground.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_20() {
        assert_eq!(convert_signed_to_i32::<20>(0x00000), 0);
        assert_eq!(convert_signed_to_i32::<20>(0x00001), 1);
        assert_eq!(convert_signed_to_i32::<20>(0x00002), 2);
        assert_eq!(convert_signed_to_i32::<20>(0x7FFFE), 524286);
        assert_eq!(convert_signed_to_i32::<20>(0x7FFFF), 524287);
        assert_eq!(convert_signed_to_i32::<20>(0x80000), -524288);
        assert_eq!(convert_signed_to_i32::<20>(0x80001), -524287);
        assert_eq!(convert_signed_to_i32::<20>(0xFFFFF), -1);
        assert_eq!(convert_signed_to_i32::<20>(0xFFFFE), -2);
    }

    #[test]
    fn bits_24() {
        assert_eq!(convert_signed_to_i32::<24>(0x000000), 0);
        assert_eq!(convert_signed_to_i32::<24>(0x000001), 1);
        assert_eq!(convert_signed_to_i32::<24>(0x000002), 2);
        assert_eq!(convert_signed_to_i32::<24>(0x7FFFFE), 8388606);
        assert_eq!(convert_signed_to_i32::<24>(0x7FFFFF), 8388607);
        assert_eq!(convert_signed_to_i32::<24>(0x800000), -8388608);
        assert_eq!(convert_signed_to_i32::<24>(0x800001), -8388607);
        assert_eq!(convert_signed_to_i32::<24>(0xFFFFFF), -1);
        assert_eq!(convert_signed_to_i32::<24>(0xFFFFFE), -2);
    }

    // TODO: add unit tests for out-of-range
}
