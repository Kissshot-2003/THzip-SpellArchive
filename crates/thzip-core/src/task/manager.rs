use crate::task::event::{CancelToken, TaskId, TaskPhase};

#[derive(Debug)]
pub struct TaskRecord {
    pub id: TaskId,
    pub title: String,
    pub phase: TaskPhase,
    pub processed_compressed: u64,
    pub written: u64,
    pub total_bytes: u64,
    pub cancel: CancelToken,
}

impl TaskRecord {
    pub fn ratio(&self) -> f32 {
        if self.total_bytes == 0 {
            0.0
        } else {
            (self.written as f64 / self.total_bytes as f64).clamp(0.0, 1.0) as f32
        }
    }
}

#[derive(Debug, Default)]
pub struct TaskTable {
    tasks: Vec<TaskRecord>,
    next_id: TaskId,
}

impl TaskTable {
    pub fn add(&mut self, title: impl Into<String>) -> TaskId {
        let id = self.next_id;
        self.next_id += 1;
        self.tasks.push(TaskRecord {
            id,
            title: title.into(),
            phase: TaskPhase::Pending,
            processed_compressed: 0,
            written: 0,
            total_bytes: 0,
            cancel: CancelToken::new(),
        });
        id
    }

    pub fn get(&self, id: TaskId) -> Option<&TaskRecord> {
        self.tasks.iter().find(|task| task.id == id)
    }

    pub fn get_mut(&mut self, id: TaskId) -> Option<&mut TaskRecord> {
        self.tasks.iter_mut().find(|task| task.id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &TaskRecord> {
        self.tasks.iter()
    }

    pub fn set_phase(&mut self, id: TaskId, phase: TaskPhase) {
        if let Some(task) = self.get_mut(id) {
            task.phase = phase;
        }
    }

    pub fn remove_finished(&mut self) {
        self.tasks
            .retain(|task| !matches!(task.phase, TaskPhase::Done | TaskPhase::Failed));
    }
}
