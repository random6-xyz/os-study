// kernel/src/pic.rs
// 8259 Programmable Interrupt Controller: remap IRQ 0-15 to IDT 32-47.
//
// The x86_64 crate no longer ships PIC support, so this is a direct
// port-I/O implementation.

use x86_64::instructions::port::Port;

const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_COMMAND: u16 = 0xa0;
const PIC2_DATA: u16 = 0xa1;

const PIC1_OFFSET: u8 = 0x20; // IRQ 0-7  -> IDT 32-39
const PIC2_OFFSET: u8 = 0x28; // IRQ 8-15 -> IDT 40-47

const ICW1_INIT: u8 = 0x11; // ICW4 needed, cascade mode
const PIC_EOI: u8 = 0x20;

pub fn init() {
    unsafe {
        let mut cmd1 = Port::<u8>::new(PIC1_COMMAND);
        let mut data1 = Port::<u8>::new(PIC1_DATA);
        let mut cmd2 = Port::<u8>::new(PIC2_COMMAND);
        let mut data2 = Port::<u8>::new(PIC2_DATA);

        // ICW1: start initialization on both controllers.
        cmd1.write(ICW1_INIT);
        cmd2.write(ICW1_INIT);
        // ICW2: remap the vector offsets.
        data1.write(PIC1_OFFSET);
        data2.write(PIC2_OFFSET);
        // ICW3: cascade wiring (slave is connected to master IRQ2).
        data1.write(0x04);
        data2.write(0x02);
        // ICW4: 8086 mode.
        data1.write(0x01);
        data2.write(0x01);

        // Mask all IRQs; individual lines are unmasked on demand.
        data1.write(0xff);
        data2.write(0xff);
    }
}

/// Unmask a single IRQ line.
pub fn unmask(irq: u8) {
    let (port, bit) = if irq < 8 {
        (PIC1_DATA, 1u8 << irq)
    } else {
        (PIC2_DATA, 1u8 << (irq - 8))
    };
    unsafe {
        let mut data = Port::<u8>::new(port);
        let mask = data.read() & !bit;
        data.write(mask);
    }
}

/// Send the end-of-interrupt command for an IRQ.
pub fn send_eoi(irq: u8) {
    unsafe {
        if irq >= 8 {
            let mut cmd2 = Port::<u8>::new(PIC2_COMMAND);
            cmd2.write(PIC_EOI);
        }
        let mut cmd1 = Port::<u8>::new(PIC1_COMMAND);
        cmd1.write(PIC_EOI);
    }
}
