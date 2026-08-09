extern crate alloc;

use spin::Mutex;
use x86_64::instructions::interrupts;

use crate::sched::circular_queue::CircularQueue;
use crate::sched::task::{task_switch_to, TaskContext, TaskStatus, TaskStruct};

#[derive(Debug)]
pub enum SchedError {
    NotRunnable,
    EmptyRq,
    PopRq,
    Full,
}

pub enum SchedType {
    FIFO,
    RR,
    CFS,
}

pub struct Scheduler {
    pub method: SchedType,
    pub run_queue: CircularQueue<TaskStruct, 64>,
}

/// Global scheduler instance, initialized once at boot.
pub static SCHEDULER: Mutex<Option<Scheduler>> = Mutex::new(None);

/// Task currently running on the CPU. `None` while the idle loop runs.
static CURRENT: Mutex<Option<TaskStruct>> = Mutex::new(None);

/// Tasks currently blocked (waiting for an event). Populated by
/// `block_current`, drained by `wake_up`.
pub static WAIT_QUEUE: Mutex<Option<CircularQueue<TaskStruct, 64>>> = Mutex::new(None);

impl Scheduler {
    pub fn init(sched_type: SchedType) -> Self {
        Self {
            method: sched_type,
            run_queue: CircularQueue::new(),
        }
    }

    pub fn enqueue_task(&mut self, new_task: TaskStruct) -> Result<(), SchedError> {
        if !new_task.is_runnable() {
            return Err(SchedError::NotRunnable);
        }

        self.run_queue
            .push_back(new_task)
            .map_err(|_| SchedError::Full)?;

        Ok(())
    }

    fn sched_dequeue_task(&mut self) -> Result<TaskStruct, SchedError> {
        self.run_queue.pop_front().ok_or(SchedError::EmptyRq)
    }
}

/// Create a task and put it on the ready queue.
pub fn spawn(entry: fn() -> !) {
    let task = TaskStruct::new(entry);
    crate::serial_println!("[TASK] pid {}: {:?}", task.pid, TaskStatus::Ready);

    interrupts::without_interrupts(|| {
        let mut sched = SCHEDULER.lock();
        sched
            .as_mut()
            .expect("scheduler not initialized")
            .enqueue_task(task)
            .unwrap();
    });
}

/// Block the currently running task: move it from `CURRENT` to the wait
/// queue (state `Blocked`) and switch to the next runnable task. If the
/// ready queue is empty, the idle loop resumes.
///
/// Called cooperatively by a task itself (e.g. waiting on a lock), so it
/// runs with interrupts disabled while `CURRENT`/queues are mutated; the
/// switched-to task re-enables interrupts on entry (or via `iretq` when
/// resuming).
pub fn block_current() {
    interrupts::without_interrupts(|| {
        let (prev_ptr, next_ptr) = {
            let mut sched = SCHEDULER.lock();
            let sched = match sched.as_mut() {
                Some(s) => s,
                None => return, // scheduler not initialized yet
            };
            let mut current = CURRENT.lock();
            let mut wait = WAIT_QUEUE.lock();

            let mut prev = match current.take() {
                Some(t) => t,
                None => return, // idle loop: nothing to block
            };
            prev.change_state(TaskStatus::Blocked).unwrap();
            let wait_q = wait.as_mut().unwrap();
            wait_q.push_back(prev).unwrap();
            let wait_back = wait_q.back_mut().unwrap();
            let prev_ptr = &mut wait_back.context as *mut TaskContext;

            let next_ptr = match sched.sched_dequeue_task() {
                Ok(mut task) => {
                    task.change_state(TaskStatus::Running).unwrap();
                    let ptr = &mut task.context as *mut TaskContext;
                    *current = Some(task);
                    ptr
                }
                Err(_) => core::ptr::addr_of_mut!(crate::IDLE_CTX),
            };

            (prev_ptr, next_ptr)
        };

        task_switch_to(prev_ptr, next_ptr);
    });
}

/// Wake a blocked task by pid: move it from the wait queue back to the
/// ready queue (state `Ready`). Does nothing if no task with `pid` is
/// blocked.
pub fn wake_up(pid: u16) {
    interrupts::without_interrupts(|| {
        let mut sched = SCHEDULER.lock();
        let sched = match sched.as_mut() {
            Some(s) => s,
            None => return,
        };
        let mut wait = WAIT_QUEUE.lock();
        let wait_q = match wait.as_mut() {
            Some(q) => q,
            None => return,
        };

        if let Some(mut task) = wait_q.remove_if(|t| t.pid == pid) {
            task.change_state(TaskStatus::Ready).unwrap();
            sched.enqueue_task(task).unwrap();
        }
    });
}

/// Pick the next task from the ready queue and switch to it.
///
/// Called from the IRQ0 handler, where interrupts are already disabled
/// (interrupt gate), so the scheduler state cannot be re-entered here.
/// `idle_ctx` is the idle loop's `TaskContext`, used to save the current
/// context when no task was running before this switch.
pub fn schedule(idle_ctx: *mut TaskContext) {
    // Take both context pointers under the locks, then switch with the
    // locks released: a lock held across `task_switch_to` would never be
    // released by the switched-to task.
    let (prev_ptr, next_ptr) = {
        let mut sched = SCHEDULER.lock();
        // Scheduler not initialized yet (early boot): stay on idle.
        let sched = match sched.as_mut() {
            Some(s) => s,
            None => return,
        };
        let mut current = CURRENT.lock();

        let mut next = match sched.sched_dequeue_task() {
            Ok(task) => task,
            Err(_) => return, // ready queue empty: stay on the idle loop
        };

        // The previously running task (if any) goes back to the queue.
        let prev_ptr = if let Some(mut prev) = current.take() {
            prev.change_state(TaskStatus::Ready).unwrap();
            // Cannot be full: it just handed us `next`.
            sched.run_queue.push_back(prev).unwrap();
            let back = sched.run_queue.back_mut().unwrap();
            &mut back.context as *mut TaskContext
        } else {
            idle_ctx
        };

        next.change_state(TaskStatus::Running).unwrap();
        let next_ptr = &mut next.context as *mut TaskContext;
        *current = Some(next);

        (prev_ptr, next_ptr)
    };

    task_switch_to(prev_ptr, next_ptr);
}
