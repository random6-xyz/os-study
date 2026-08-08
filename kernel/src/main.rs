#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]

extern crate alloc;

use alloc::{boxed::Box, format, vec::Vec};
use bootloader_api::{entry_point, BootInfo};
use core::panic::PanicInfo;

use crate::sched::{
    fifo::{SchedType, Scheduler},
    task::TaskStruct,
};

mod interrupts;
mod memory;
mod pic;
mod pit;
mod qemu;
mod sched;
mod serial;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    serial::init();

    serial_println!("[BOOT] entered Rust kernel");

    memory::init(boot_info);

    interrupts::init();

    serial_println!("[INIT] IDT loaded");

    x86_64::instructions::interrupts::enable();
    serial_println!("[INIT] interrupts enabled");

    x86_64::instructions::interrupts::int3();

    serial_println!("[TEST] breakpoint returned successfully");

    heap_test();

    let mut scheduler = Scheduler::init(SchedType::FIFO);
    let mut init_task = TaskStruct::new();

    // Second-based tick logger: the PIT fires at 100 Hz, so the
    // counter grows by 100 every second.
    let mut last_second = 0u64;
    loop {
        let ticks = interrupts::ticks();
        let second = ticks / 100;
        if second > last_second {
            last_second = second;
            serial_println!("[TIMER] tick {ticks}");
        }
        // TODO: run usermode init process
        x86_64::instructions::hlt();
    }
}

/// Exercise the global allocator: Box, Vec, and format! must work.
fn heap_test() {
    let x = Box::new(40u64);
    let y = Box::new(2u64);
    serial_println!("[TEST] box: {} + {} = {}", *x, *y, *x + *y);

    let mut v: Vec<u32> = Vec::new();
    for i in 0..1000 {
        v.push(i);
    }
    let sum: u32 = v.iter().sum();
    serial_println!("[TEST] vec sum 0..1000 = {sum}");

    let s = format!(
        "[TEST] format: {} bytes per u64",
        core::mem::size_of::<u64>()
    );
    serial_println!("{s}");

    serial_println!("[TEST] heap alloc ok");
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    serial_println!();
    serial_println!("[PANIC] {info}");

    qemu::exit_failure()
}
