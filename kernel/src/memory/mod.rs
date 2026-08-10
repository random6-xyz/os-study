// kernel/src/memory/mod.rs
// Memory management: frame allocator (PLAN.md 2.2) plus the temporary
// bump heap used by `Box`/`Vec` until the heap is reworked.

pub mod frame;
pub mod heap;

use bootloader_api::BootInfo;
use spin::Mutex;

/// Global frame allocator, initialized once at boot from the bootloader
/// memory map. Protected by a mutex so interrupts/context switches never
/// race on the bitmap.
static FRAME_ALLOCATOR: Mutex<Option<frame::FrameAllocator<'static>>> = Mutex::new(None);

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

    let phys_offset = boot_info
        .physical_memory_offset
        .into_option()
        .map(|o| o as usize)
        .unwrap_or(0);

    let allocator = frame::FrameAllocator::init(&boot_info.memory_regions, phys_offset);
    match allocator.as_ref() {
        Some(a) => {
            crate::serial_println!(
                "[FRAME] init ok: {} free of {} total",
                a.free_frames(),
                a.total_frames()
            );
            *FRAME_ALLOCATOR.lock() = allocator;
        }
        None => crate::serial_println!("[FRAME] init failed: no usable regions"),
    }

    // Protect bootloader-owned memory (kernel ELF, page tables, boot
    // info, kernel stack) from being handed out as free frames.
    for region in boot_info.memory_regions.iter() {
        if region.kind == bootloader_api::info::MemoryRegionKind::Bootloader {
            let start = region.start as usize;
            let end = region.end as usize;
            crate::serial_println!("[FRAME] protect bootloader {:#x}-{:#x}", start, end);
            mark_range_used(start, end);
        }
    }

    crate::serial_println!(
        "[FRAME] after protection: {} free frames",
        free_frames()
    );
}

/// Allocates one physical frame. Returns `Err` on OOM.
pub fn alloc_frame() -> Result<frame::Frame, frame::FrameError> {
    FRAME_ALLOCATOR
        .lock()
        .as_mut()
        .ok_or(frame::FrameError::OutOfMemory)?
        .alloc_frame()
}

/// Frees a physical frame. Returns `Err` on double free / out of range.
pub fn dealloc_frame(frame: frame::Frame) -> Result<(), frame::FrameError> {
    FRAME_ALLOCATOR
        .lock()
        .as_mut()
        .ok_or(frame::FrameError::OutOfRange)?
        .dealloc_frame(frame)
}

/// Number of currently free frames.
pub fn free_frames() -> usize {
    FRAME_ALLOCATOR.lock().as_ref().map_or(0, |a| a.free_frames())
}

/// Marks a physical range as used (protecting kernel/bootloader memory).
pub fn mark_range_used(start: usize, end: usize) {
    if let Some(a) = FRAME_ALLOCATOR.lock().as_mut() {
        a.mark_range_used(start, end);
    }
}
