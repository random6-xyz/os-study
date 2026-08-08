// kernel/src/interrupts.rs

use core::sync::atomic::{AtomicU64, Ordering};

use spin::Once;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

use crate::{pic, pit};

static IDT: Once<InterruptDescriptorTable> = Once::new();

/// Number of timer ticks since boot (incremented by the PIT IRQ0 handler).
static TICKS: AtomicU64 = AtomicU64::new(0);

pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

pub fn init() {
    let idt = IDT.call_once(|| {
        let mut idt = InterruptDescriptorTable::new();

        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt.page_fault.set_handler_fn(page_fault_handler);
        idt[32].set_handler_fn(timer_interrupt_handler); // IRQ0

        idt
    });

    idt.load();

    pit::init();
    pic::init();
    pic::unmask(0); // unmask the timer IRQ only

    // 인터럽트 플래그(IF)는 커널 초기화가 끝난 뒤 main에서 켠다.
}

extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    TICKS.fetch_add(1, Ordering::Relaxed);
    pic::send_eoi(0);
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    crate::serial_println!("EXCEPTION: BREAKPOINT");
    crate::serial_println!("{stack_frame:#?}");
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;

    crate::serial_println!("EXCEPTION: PAGE FAULT");
    crate::serial_println!("address: {:?}", Cr2::read());
    crate::serial_println!("error: {error_code:?}");
    crate::serial_println!("{stack_frame:#?}");

    loop {
        x86_64::instructions::hlt();
    }
}
