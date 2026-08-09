extern crate alloc;

use crate::sched::circular_queue::CircularQueue;
use crate::sched::task::TaskStruct;

#[derive(Debug)]
pub enum SchedError {
    NotRunnable,
    EmptyRq,
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

    /// Take the next runnable task off the ready queue.
    pub fn sched_dequeue_task(&mut self) -> Result<TaskStruct, SchedError> {
        self.run_queue.pop_front().ok_or(SchedError::EmptyRq)
    }
}
