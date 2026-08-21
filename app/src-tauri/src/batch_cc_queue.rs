use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct BatchCcTask {
    pub message_id: i32,
    pub folder_id: Option<i64>,
    pub file_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchCcQueueStatus {
    pub total_queued: usize,
    pub completed_count: usize,
    pub current_file: Option<String>,
    pub is_running: bool,
}

struct BatchCcInnerState {
    queue: VecDeque<BatchCcTask>,
    is_running: bool,
    current_file: Option<String>,
    completed_count: usize,
}

pub struct BatchCcQueueManager {
    state: Arc<Mutex<BatchCcInnerState>>,
}

impl BatchCcQueueManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(BatchCcInnerState {
                queue: VecDeque::new(),
                is_running: false,
                current_file: None,
                completed_count: 0,
            })),
        }
    }

    pub async fn enqueue_tasks(&self, tasks: Vec<BatchCcTask>) {
        let mut s = self.state.lock().await;
        for t in tasks {
            s.queue.push_back(t);
        }
    }

    pub async fn get_status(&self) -> BatchCcQueueStatus {
        let s = self.state.lock().await;
        BatchCcQueueStatus {
            total_queued: s.queue.len(),
            completed_count: s.completed_count,
            current_file: s.current_file.clone(),
            is_running: s.is_running,
        }
    }

    pub async fn clear_queue(&self) {
        let mut s = self.state.lock().await;
        s.queue.clear();
        s.current_file = None;
        s.is_running = false;
    }
}
