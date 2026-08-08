// kernel/src/memory/heap.rs
// Minimal bump allocator over a static array. It never frees memory and
// exists only so that `Box`/`Vec`/`format!` work in the kernel. It will be
// replaced by the real memory manager (PLAN.md 2.2).

use core::{
    alloc::{GlobalAlloc, Layout},
    ptr,
};

use spin::Mutex;

const HEAP_SIZE: usize = 8 * 1024 * 1024; // 8 MiB

// Accessed only through raw pointers (`addr_of_mut!`), never by reference.
#[repr(align(4096))]
#[allow(dead_code)] // the array is only addressed as a whole
struct HeapSpace([u8; HEAP_SIZE]);

static mut HEAP_SPACE: HeapSpace = HeapSpace([0; HEAP_SIZE]);

/// Bump allocator state: byte offsets from the base of `HEAP_SPACE`.
struct BumpAllocator {
    next: usize,
    end: usize,
}

impl BumpAllocator {
    const fn new() -> Self {
        Self {
            next: 0,
            end: HEAP_SIZE,
        }
    }
}

static ALLOCATOR: Mutex<BumpAllocator> = Mutex::new(BumpAllocator::new());

fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

struct GlobalBump;

unsafe impl GlobalAlloc for GlobalBump {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut allocator = ALLOCATOR.lock();

        let start = align_up(allocator.next, layout.align());
        let end = match start.checked_add(layout.size()) {
            Some(end) => end,
            None => return core::ptr::null_mut(),
        };
        if end > allocator.end {
            return core::ptr::null_mut();
        }

        allocator.next = end;
        (ptr::addr_of_mut!(HEAP_SPACE) as usize + start) as *mut u8
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // A bump allocator never frees.
    }
}

#[global_allocator]
static GLOBAL_BUMP: GlobalBump = GlobalBump;

#[alloc_error_handler]
fn alloc_error_handler(layout: Layout) -> ! {
    crate::serial_println!("[HEAP] allocation error: {layout:?}");
    crate::qemu::exit_failure()
}
