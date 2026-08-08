// kernel/src/memory/mod.rs
// Minimal memory infrastructure: expose the boot memory map so the real
// memory manager (PLAN.md 2.2) can be built on top of it later.

pub mod heap;

use bootloader_api::BootInfo;

pub fn init(boot_info: &'static BootInfo) {
    crate::serial_println!(
        "[MEM] physical_memory_offset: {:?}",
        boot_info.physical_memory_offset
    );

    for (i, region) in boot_info.memory_regions.iter().enumerate() {
        crate::serial_println!(
            "[MEM] region {i}: {:?} 0x{:x}-0x{:x} ({} KiB)",
            region.kind,
            region.start,
            region.end,
            (region.end - region.start) / 1024
        );
    }
}
