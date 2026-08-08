const TASK_NICE_MAX: i8 = i8::MAX;
const TASK_NICE_MIN: i8 = i8::MIN;
const TASK_NICE_DEFAULT: i8 = 0;

// include all types of error caused by task
pub enum TaskError {}

// normal task state
#[derive(PartialEq, Eq)]
pub enum TaskStatus {
    Ready,
    Running,
    Blocked,
    Terminated,
}

// contains context for context switching
pub struct TaskContext {}

impl TaskContext {
    pub fn new() -> Self {
        Self {}
    }
}

// get a new pid
fn task_get_new_pid() -> u16 {
    todo!();
}

// general task struct
pub struct TaskStruct {
    pub status: TaskStatus,
    pub pid: u16,
    pub nice: i8,
    pub context: TaskContext,
}

impl TaskStruct {
    pub fn new() -> Self {
        Self {
            status: TaskStatus::Ready,
            pid: task_get_new_pid(),
            nice: TASK_NICE_DEFAULT,
            context: TaskContext::new(),
        }
    }

    pub fn destory(&self) -> Result<u32, TaskError> {
        todo!();
    }

    pub fn is_runnable(&self) -> bool {
        if self.status == TaskStatus::Ready {
            return true;
        }
        false
    }

    pub fn change_state(&self, status: TaskStatus) -> Result<u32, TaskError> {
        todo!();
    }
}
