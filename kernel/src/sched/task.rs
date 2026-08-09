use alloc::boxed::Box;
use core::arch::naked_asm;
use core::sync::atomic::{AtomicU16, Ordering};

const TASK_NICE_MAX: i8 = i8::MAX;
const TASK_NICE_MIN: i8 = i8::MIN;
const TASK_NICE_DEFAULT: i8 = 0;

static NEXT_PID: AtomicU16 = AtomicU16::new(1);

// include all types of error caused by task
#[derive(Debug)]
pub enum TaskError {
    IncorrectStatus,
}

// normal task state
#[derive(Debug, PartialEq, Eq)]
pub enum TaskStatus {
    Ready,
    Running,
    Blocked,
    Terminated,
}

pub fn task_main() -> ! {
    loop {
        crate::serial_print!("[TASK] task main");

        // syscall
    }
}

// contains context for context switching
#[repr(C)]
pub struct TaskContext {
    pub sp: usize,
}

#[unsafe(naked)]
pub extern "C" fn task_switch_to(prev: *mut TaskContext, next: *mut TaskContext) {
    naked_asm!(
        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "mov [rdi], rsp",
        "mov rsp, [rsi]",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        "ret",
    )
}

impl TaskContext {
    pub fn new(entry: fn() -> !) -> Self {
        const STACK_SIZE: usize = 16 * 1024;
        let stack = Box::new([0u8; STACK_SIZE]);
        let base = stack.as_ptr() as usize;
        core::mem::forget(stack);

        let sp = (base + STACK_SIZE - 56) & !15;
        let frame = sp as *mut usize;
        unsafe {
            *frame.add(0) = 0;
            *frame.add(1) = 0;
            *frame.add(2) = 0;
            *frame.add(3) = 0;
            *frame.add(4) = 0;
            *frame.add(5) = 0;
            *frame.add(6) = entry as usize;
        }
        Self { sp }
    }
}
// get a new pid
fn task_get_new_pid() -> u16 {
    NEXT_PID.fetch_add(1, Ordering::Relaxed)
}

// general task struct
pub struct TaskStruct {
    pub status: TaskStatus,
    pub pid: u16,
    pub nice: i8,
    pub context: TaskContext,
}

impl TaskStruct {
    pub fn new(entry: fn() -> !) -> Self {
        Self {
            status: TaskStatus::Ready,
            pid: task_get_new_pid(),
            nice: TASK_NICE_DEFAULT,
            context: TaskContext::new(entry),
        }
    }

    pub fn destory(&mut self) -> Result<(), TaskError> {
        self.status = TaskStatus::Terminated;

        return Ok(());
    }

    pub fn is_runnable(&self) -> bool {
        if self.status == TaskStatus::Ready {
            return true;
        }
        false
    }

    pub fn change_state(&mut self, status: TaskStatus) -> Result<(), TaskError> {
        crate::serial_println!("[TASK] pid {}: {:?} -> {:?}", self.pid, self.status, status);
        self.status = status;
        Ok(())
    }
}
