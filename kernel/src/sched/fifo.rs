extern crate alloc;

pub enum SchedError {
    NotRunnable,
    EmptyRq,
    PopRq,
}

pub enum SchedType {
    FIFO,
    RR,
    CFS,
}

use crate::sched::task::*;
use alloc::collections::VecDeque;

pub struct Scheduler {
    pub method: SchedType,
    pub run_queue: VecDeque<TaskStruct>,
}

impl Scheduler {
    pub fn init(sched_type: SchedType) -> Self {
        Self {
            method: sched_type,
            run_queue: VecDeque::new(),
        }
    }

    fn enqueue_task(&mut self, new_task: TaskStruct) -> Result<(), SchedError> {
        if !new_task.is_runnable() {
            return Err(SchedError::NotRunnable);
        }

        self.run_queue.push_back(new_task);

        Ok(())
    }

    fn sched_dequeue_task(&mut self) -> Result<u16, SchedError> {
        if self.run_queue.is_empty() {
            return Err(SchedError::EmptyRq);
        }
        if let Some(mut cur) = self.run_queue.pop_front() {
            cur.status = TaskStatus::Terminated;
            return Ok(cur.pid);
        } else {
            return Err(SchedError::PopRq);
        }
    }
}
