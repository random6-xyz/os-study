// kernel/src/memory/frame.rs
// Bitmap-based frame allocator (PLAN.md 2.2).
//
// The allocator manages physical memory in fixed-size frames (4 KiB)
// and tracks used/free state in a bitmap. The bitmap itself lives in
// the first managed frame, so no separate static buffer is needed.
// Only `Usable` regions from the bootloader are collected; everything
// else (kernel, bootloader, MMIO, reserved memory) is never handed out.

use alloc::vec::Vec;
use bootloader_api::info::{MemoryRegion, MemoryRegionKind};

/// Physical frame size in bytes.
pub const FRAME_SIZE: usize = 4096;

/// End of the legacy low-memory area. Everything below 1 MiB (IVT, BDA,
/// EBDA, VGA, BIOS ROM) is owned by the system and never managed by the
/// frame allocator, matching how real x86 kernels treat low memory.
pub const LOW_MEM_END: u64 = 0x100000;

fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

/// A physical frame identified by its physical start address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub addr: usize,
}

/// Errors returned by the frame allocator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    /// Not enough free frames left.
    OutOfMemory,
    /// The frame lies outside the managed range.
    OutOfRange,
    /// The frame is already free (e.g. double free).
    DoubleFree,
}

/// Bitmap over the managed frames. Bit 0 corresponds to the frame at
/// `frame_start`, bit 1 to `frame_start + FRAME_SIZE`, and so on.
/// A set bit (1) means the frame is in use.
///
/// `bits` is a plain mutable slice into physical memory (the first
/// managed frame, see `FrameAllocator::init`). All bit access is
/// implemented by hand (no bitvec crate) as part of PLAN 2.5.
pub struct Bitmap<'a> {
    bits: &'a mut [usize],
    frame_start: usize,
}

impl<'a> Bitmap<'a> {
    /// Creates a bitmap over `bits`. `frame_start` is the physical
    /// address of the frame that bit 0 refers to.
    fn new(bits: &'a mut [usize], frame_start: usize) -> Self {
        Self { bits, frame_start }
    }

    /// Number of bits (frames) this bitmap can track.
    fn bit_len(&self) -> usize {
        self.bits.len() * usize::BITS as usize
    }

    /// Returns the index of the bit for a frame's physical address.
    fn bit_index(&self, addr: usize) -> Option<usize> {
        if addr < self.frame_start || addr % FRAME_SIZE != 0 {
            return None;
        }
        let idx = (addr - self.frame_start) / FRAME_SIZE;
        (idx < self.bit_len()).then_some(idx)
    }

    /// Returns the physical address of the frame for a bit index.
    fn bit_addr(&self, idx: usize) -> usize {
        self.frame_start + idx * FRAME_SIZE
    }

    fn get_bit(&self, idx: usize) -> bool {
        self.bits[idx / usize::BITS as usize] & (1 << (idx % usize::BITS as usize)) != 0
    }

    fn set_bit(&mut self, idx: usize) {
        self.bits[idx / usize::BITS as usize] |= 1 << (idx % usize::BITS as usize);
    }

    fn clear_bit(&mut self, idx: usize) {
        self.bits[idx / usize::BITS as usize] &= !(1 << (idx % usize::BITS as usize));
    }

    /// Index of the first clear (free) bit, or `None` if the bitmap is
    /// fully set.
    fn first_free_bit(&self) -> Option<usize> {
        for (word, word_val) in self.bits.iter().enumerate() {
            let inv = !*word_val;
            if inv != 0 {
                return Some(word * usize::BITS as usize + inv.trailing_zeros() as usize);
            }
        }
        None
    }
}

/// Bitmap-based physical frame allocator.
pub struct FrameAllocator<'a> {
    /// Bitmap over the managed frames.
    bitmap: Bitmap<'a>,
    /// Physical address of the first managed frame (index 0). Used by
    /// the bitmap for address ↔ index mapping; kept here for reference.
    #[allow(dead_code)]
    frame_start: usize,
    /// Physical address where the bitmap storage lives. Kept separate
    /// from `frame_start`: the first usable region may overlap kernel
    /// code (bootloader loads the ELF there), so the bitmap is placed in
    /// the largest usable region instead.
    #[allow(dead_code)]
    bitmap_base: usize,
    /// Total number of managed frames.
    total_frames: usize,
    /// Number of currently free frames.
    free_frames: usize,
}

impl<'a> FrameAllocator<'a> {
    /// Creates an allocator over one or more usable memory regions.
    ///
    /// All regions are merged into a single linear frame space starting
    /// at the first region's start address. The bitmap lives in the
    /// first frame of that first region; gaps between regions are
    /// covered by protected (used) frames, so alloc/free stays a simple
    /// "frame index = (addr - frame_start) / FRAME_SIZE" mapping.
    ///
    /// `regions` is the physical memory map; only `Usable` regions are
    /// collected. `phys_offset` is the bootloader's physical memory
    /// mapping offset (`BootInfo.physical_memory_offset`): physical
    /// address + offset = virtual address, used to access the bitmap.
    /// Returns `None` when there is nothing to manage.
    pub fn init(regions: &[MemoryRegion], phys_offset: usize) -> Option<Self> {
        let mut usable: Vec<MemoryRegion> = regions
            .iter()
            .filter(|r| r.kind == MemoryRegionKind::Usable)
            .copied()
            .collect();
        // Work on frame-aligned boundaries only and clip everything
        // below the legacy low-memory end (1 MiB): the low area belongs
        // to the system (IVT/BDA/EBDA/VGA/BIOS ROM) and must never be
        // handed out as a free frame.
        for region in usable.iter_mut() {
            region.start = align_up(region.start as usize, FRAME_SIZE).max(LOW_MEM_END as usize) as u64;
            region.end = (region.end as usize & !(FRAME_SIZE - 1)) as u64;
        }
        usable.retain(|r| r.end > r.start);

        let first = usable.first()?;
        let last = usable.last().unwrap();
        let frame_start = first.start as usize;
        let total_frames = (last.end - first.start) as usize / FRAME_SIZE;
        if total_frames == 0 {
            return None;
        }

        // The bitmap occupies ceil(total_frames / bits-per-word) words.
        // 64 frames per word, 4096 frames per frame → a single frame
        // covers 16 MiB of managed memory; assert that invariant.
        let words_needed = total_frames.div_ceil(usize::BITS as usize);
        debug_assert!(words_needed * core::mem::size_of::<usize>() <= FRAME_SIZE);

        // Place the bitmap in the largest usable region instead of the
        // first one: the bootloader loads the kernel ELF right after
        // 1 MiB (the first usable region), so overwriting that area
        // would corrupt the running kernel.
        let largest = usable.iter().max_by_key(|r| r.end - r.start).unwrap();
        let bitmap_base = largest.start as usize;
        // Physical memory is accessed through the bootloader's linear
        // mapping: phys + offset = virt.
        let bitmap_virt = bitmap_base + phys_offset;

        // SAFETY: `bitmap_virt` points into the bootloader's physical
        // memory mapping (all `Usable` memory is mapped there) and the
        // bitmap fits in one frame. It is marked used below and never
        // handed out.
        let bitmap = unsafe {
            core::slice::from_raw_parts_mut(bitmap_virt as *mut usize, words_needed)
        };
        bitmap.fill(0);

        let mut allocator = Self {
            bitmap: Bitmap::new(bitmap, frame_start),
            frame_start,
            bitmap_base,
            total_frames,
            free_frames: total_frames,
        };

        // Mark the bitmap's own frame as used: it must never be handed
        // out as a free frame.
        allocator.mark_used(bitmap_base);

        crate::serial_println!(
            "[FRAME] init: {} usable regions, {:#x}-{:#x}, {} frames ({} KiB), bitmap at {:#x}",
            usable.len(),
            frame_start,
            last.end,
            total_frames,
            (last.end as usize - frame_start) / 1024,
            bitmap_base,
        );

        Some(allocator)
    }

    /// Allocates one free frame and marks it used. Returns `Err` on OOM.
    pub fn alloc_frame(&mut self) -> Result<Frame, FrameError> {
        let idx = match self.bitmap.first_free_bit() {
            Some(idx) => idx,
            None => {
                crate::serial_println!(
                    "[FRAME] OOM: no free frames ({} total, 0 free)",
                    self.total_frames
                );
                return Err(FrameError::OutOfMemory);
            }
        };
        if idx >= self.total_frames {
            crate::serial_println!(
                "[FRAME] OOM: bitmap exhausted at index {idx} ({} total)",
                self.total_frames
            );
            return Err(FrameError::OutOfMemory);
        }

        self.bitmap.set_bit(idx);
        self.free_frames -= 1;
        let frame = Frame {
            addr: self.bitmap.bit_addr(idx),
        };
        crate::serial_println!(
            "[FRAME] alloc: {:#x} ({} free left)",
            frame.addr,
            self.free_frames
        );
        Ok(frame)
    }

    /// Frees a frame. Returns `Err` if the frame is outside the managed
    /// range or already free (double free).
    pub fn dealloc_frame(&mut self, frame: Frame) -> Result<(), FrameError> {
        let idx = match self.bitmap.bit_index(frame.addr) {
            Some(idx) => idx,
            None => {
                crate::serial_println!("[FRAME] invalid free: {:#x} (out of range)", frame.addr);
                return Err(FrameError::OutOfRange);
            }
        };
        if idx >= self.total_frames {
            crate::serial_println!("[FRAME] invalid free: {:#x} (out of range)", frame.addr);
            return Err(FrameError::OutOfRange);
        }

        if !self.bitmap.get_bit(idx) {
            crate::serial_println!("[FRAME] invalid free: {:#x} (double free)", frame.addr);
            return Err(FrameError::DoubleFree);
        }

        self.bitmap.clear_bit(idx);
        self.free_frames += 1;
        crate::serial_println!(
            "[FRAME] free: {:#x} ({} free left)",
            frame.addr,
            self.free_frames
        );
        Ok(())
    }

    /// Marks a frame as used without handing it out (e.g. the bitmap
    /// frame itself). The address must be within the managed range.
    fn mark_used(&mut self, addr: usize) {
        let idx = self.bitmap.bit_index(addr).expect("frame in range");
        if !self.bitmap.get_bit(idx) {
            self.bitmap.set_bit(idx);
            self.free_frames -= 1;
        }
    }

    /// Marks a range of frames as used (for protecting the kernel and
    /// bootloader regions and gaps between usable regions).
    pub fn mark_range_used(&mut self, start: usize, end: usize) {
        let mut addr = align_up(start, FRAME_SIZE);
        while addr < end {
            if let Some(idx) = self.bitmap.bit_index(addr) {
                if idx < self.total_frames && !self.bitmap.get_bit(idx) {
                    self.bitmap.set_bit(idx);
                    self.free_frames -= 1;
                }
            }
            addr += FRAME_SIZE;
        }
    }

    pub fn free_frames(&self) -> usize {
        self.free_frames
    }

    pub fn total_frames(&self) -> usize {
        self.total_frames
    }
}
