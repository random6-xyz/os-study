#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]

extern crate alloc;

use alloc::{boxed::Box, format, vec::Vec};
use bootloader_api::{BootInfo, BootloaderConfig, entry_point};
use core::panic::PanicInfo;

use crate::sched::{circular_queue::CircularQueue, fifo::SchedType, task::TaskContext};

mod interrupts;
mod memory;
mod pic;
mod pit;
mod qemu;
mod sched;
mod serial;

/// Bootloader configuration: map the whole physical memory into the
/// virtual address space so the frame allocator can access arbitrary
/// physical frames (Linux-style `page_offset_base` linear mapping).
static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(bootloader_api::config::Mapping::new_default());
    config
};

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

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
    mem_test();

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

/// Exercise the frame allocator: alloc/free, double free, and OOM.
/// All failure paths are logged but never panic.
fn mem_test() {
    use memory::frame::{Frame, FrameError};

    serial_println!(
        "[MEM-TEST] free frames before: {}",
        memory::free_frames()
    );

    // 1. Normal alloc: three frames in a row.
    let f1 = memory::alloc_frame().expect("alloc f1");
    let f2 = memory::alloc_frame().expect("alloc f2");
    let f3 = memory::alloc_frame().expect("alloc f3");
    serial_println!(
        "[MEM-TEST] alloc ok: {:#x}, {:#x}, {:#x} ({} free left)",
        f1.addr,
        f2.addr,
        f3.addr,
        memory::free_frames()
    );

    // Frames must be distinct.
    assert_ne!(f1, f2);
    assert_ne!(f2, f3);

    // 2. Normal free: release all three.
    memory::dealloc_frame(f1).expect("free f1");
    memory::dealloc_frame(f2).expect("free f2");
    memory::dealloc_frame(f3).expect("free f3");
    serial_println!(
        "[MEM-TEST] free ok, {} free left (restored)",
        memory::free_frames()
    );

    // 3. Double free: freeing f1 again must fail with DoubleFree.
    match memory::dealloc_frame(f1) {
        Err(FrameError::DoubleFree) => {
            serial_println!("[MEM-TEST] double free ok: rejected");
        }
        other => panic!("expected DoubleFree, got {other:?}"),
    }

    // 4. Out-of-range free: a frame address outside the managed range.
    match memory::dealloc_frame(Frame { addr: 0xdead_beef }) {
        Err(FrameError::OutOfRange) => {
            serial_println!("[MEM-TEST] out-of-range free ok: rejected");
        }
        other => panic!("expected OutOfRange, got {other:?}"),
    }

    // 5. OOM: allocate until the allocator runs out.
    let mut count = 0usize;
    loop {
        match memory::alloc_frame() {
            Ok(_) => count += 1,
            Err(FrameError::OutOfMemory) => break,
            Err(e) => panic!("unexpected alloc error: {e:?}"),
        }
    }
    serial_println!("[MEM-TEST] OOM ok: exhausted after {count} frames");

    serial_println!("[MEM-TEST] all frame allocator tests passed");
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
