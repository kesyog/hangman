use crate::pac;

pub unsafe fn disable_all_gpio_sense() {
    let p1 = unsafe { &(*pac::P1::ptr()) };
    for cnf in &p1.pin_cnf {
        cnf.modify(|_, w| w.sense().disabled());
    }
    let p0 = unsafe { &(*pac::P0::ptr()) };
    for cnf in &p0.pin_cnf {
        cnf.modify(|_, w| w.sense().disabled());
    }
}
