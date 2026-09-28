use crossbeam_deque::{Injector, Steal, Worker};
use parking_lot::Mutex;
use std::sync::Arc;
use tracing::debug;

pub struct MeshTask {
    pub id: u64,
    pub description: String,
}

/// Work-Stealing Multi-Agent Mesh Coordinator.
pub struct ActorMesh {
    injector: Arc<Injector<MeshTask>>,
    worker: Mutex<Worker<MeshTask>>,
}

impl ActorMesh {
    pub fn new() -> Self {
        let injector = Arc::new(Injector::new());
        let worker = Mutex::new(Worker::new_fifo());

        Self { injector, worker }
    }

    /// Submits a task to the shared multi-agent injector queue.
    pub fn submit_task(&self, id: u64, description: impl Into<String>) {
        let task = MeshTask {
            id,
            description: description.into(),
        };
        self.injector.push(task);
        debug!("Submitted task to multi-agent mesh injector");
    }

    /// Pulls the next available task via local queue or work-stealing.
    pub fn pull_task(&self) -> Option<MeshTask> {
        let worker = self.worker.lock();
        // 1. Try local worker queue
        if let Some(task) = worker.pop() {
            return Some(task);
        }

        // 2. Steal from shared injector
        loop {
            match self.injector.steal() {
                Steal::Success(task) => return Some(task),
                Steal::Empty => return None,
                Steal::Retry => continue,
            }
        }
    }
}

impl Default for ActorMesh {
    fn default() -> Self {
        Self::new()
    }
}
