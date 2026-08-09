#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]

extern crate alloc;

use alloc::{boxed::Box, format, vec::Vec};
use bootloader_api::{BootInfo, entry_point};
use core::panic::PanicInfo;

use crate::sched::{circular_queue::CircularQueue, fifo::SchedType, task::TaskContext};

mod interrupts;
mod memory;
mod pic;
mod pit;
mod qemu;
mod sched;
mod serial;

entry_point!(kernel_main);

/// Context of the idle loop (the main loop below). Used to save the
/// current context when the timer preempts the idle loop itself.
pub static mut IDLE_CTX: TaskContext = TaskContext { sp: 0 };

/// Shared task body: each spawned task prints its name in a loop and
/// spins until the timer preempts it.
fn task_loop(name: &str) -> ! {
    // The first switch into a task reaches the entry via `ret`, so the
    // IF flag (cleared by the IRQ0 interrupt gate) must be re-enabled
    // here; from then on the task runs with interrupts on.
    x86_64::instructions::interrupts::enable();
    loop {
        serial_println!("[{name}] running");
        for _ in 0..200_000 {
            core::hint::spin_loop();
        }
    }
}

fn task_a() -> ! {
    task_loop("TASK-A")
}

fn task_b() -> ! {
    task_loop("TASK-B")
}

fn task_c() -> ! {
    task_loop("TASK-C")
}

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

    // Initialize the global scheduler and spawn three tasks. Interrupts
    // stay off here: a timer IRQ arriving mid-lock would spin forever on
    // the same scheduler mutex inside `schedule`.
    x86_64::instructions::interrupts::without_interrupts(|| {
        *crate::sched::SCHEDULER.lock() =
            Some(crate::sched::fifo::Scheduler::init(SchedType::FIFO));
        *crate::sched::WAIT_QUEUE.lock() = Some(CircularQueue::new());
        crate::sched::spawn(task_a);
        crate::sched::spawn(task_b);
        crate::sched::spawn(task_c);
    });

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
