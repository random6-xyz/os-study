// kernel/src/pit.rs
// 8254 Programmable Interval Timer on channel 0, used as the system tick.

use x86_64::instructions::port::Port;

pub const FREQUENCY_HZ: u32 = 100;

const PIT_COMMAND: u16 = 0x43;
const PIT_CHANNEL0: u16 = 0x40;
const PIT_BASE_FREQUENCY: u32 = 1_193_182; // 1.193182 MHz

pub fn init() {
    let divisor = (PIT_BASE_FREQUENCY / FREQUENCY_HZ) as u16;

    unsafe {
        let mut command = Port::<u8>::new(PIT_COMMAND);
        // channel 0 | lobyte/hibyte access | mode 2 (rate generator) | binary
        command.write(0b0011_0100);

        let mut channel0 = Port::<u8>::new(PIT_CHANNEL0);
        channel0.write((divisor & 0xff) as u8);
        channel0.write((divisor >> 8) as u8);
    }
}
