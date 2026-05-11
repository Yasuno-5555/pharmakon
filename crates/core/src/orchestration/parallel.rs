//! Parallel Task Executor — DAG-based concurrent work orchestration.
//!
//! Enables the "fire-and-continue" pattern: while waiting for tool A,
//! start tool B and C. Feed A's result into D only when ready.
//!
//! Architecture:
//!   Task graph with named dependencies → topological sort
//!   → execute independent layers in parallel
//!   → feed results through dependency edges
//!
//! Example:
//!   read_config ──┐
//!   grep_src    ──┼── analyze ──┐
//!   list_files  ──┘               ├── report
//!   cargo_check ─────────────────┘

use anyhow::Result;
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::sync::Arc;
use tokio::sync::Mutex;

/// A unit of work in the parallel executor.
pub struct ParallelTask<F, T>
where
    F: Future<Output = Result<T>> + Send + 'static,
    T: Send + 'static,
{
    /// Unique name for this task (used in dependency references).
    pub name: String,
    /// Names of tasks this task depends on.
    pub dependencies: Vec<String>,
    /// The async function to execute.
    pub work: Box<dyn FnOnce(HashMap<String, serde_json::Value>) -> F + Send>,
    _phantom: std::marker::PhantomData<T>,
}

/// Result of a single task execution.
#[derive(Debug, Clone)]
pub struct TaskOutput {
    pub name: String,
    pub success: bool,
    pub result: Option<String>,
    pub error: Option<String>,
    pub duration_ms: u64,
}

/// Parallel executor that runs tasks respecting dependency order.
pub struct ParallelExecutor {
    tasks: Vec<Box<dyn ExecutableTask>>,
}

/// Internal trait to erase the type parameter.
#[async_trait::async_trait]
trait ExecutableTask: Send {
    fn name(&self) -> &str;
    fn dependencies(&self) -> &[String];
    async fn execute(
        self: Box<Self>,
        deps: HashMap<String, serde_json::Value>,
    ) -> TaskOutput;
}

struct ErasedTask<F, T>
where
    F: Future<Output = Result<T>> + Send + 'static,
    T: serde::Serialize + Send + 'static,
{
    name: String,
    dependencies: Vec<String>,
    work: Option<Box<dyn FnOnce(HashMap<String, serde_json::Value>) -> F + Send>>,
    _phantom: std::marker::PhantomData<T>,
}

#[async_trait::async_trait]
impl<F, T> ExecutableTask for ErasedTask<F, T>
where
    F: Future<Output = Result<T>> + Send + 'static,
    T: serde::Serialize + Send + 'static,
{
    fn name(&self) -> &str { &self.name }
    fn dependencies(&self) -> &[String] { &self.dependencies }

    async fn execute(
        mut self: Box<Self>,
        deps: HashMap<String, serde_json::Value>,
    ) -> TaskOutput {
        let start = std::time::Instant::now();
        let work = self.work.take().unwrap();
        let future = work(deps);
        match future.await {
            Ok(value) => {
                let result_str = serde_json::to_string(&value).unwrap_or_default();
                TaskOutput {
                    name: self.name,
                    success: true,
                    result: Some(result_str),
                    error: None,
                    duration_ms: start.elapsed().as_millis() as u64,
                }
            }
            Err(e) => TaskOutput {
                name: self.name,
                success: false,
                result: None,
                error: Some(e.to_string()),
                duration_ms: start.elapsed().as_millis() as u64,
            },
        }
    }
}

impl ParallelExecutor {
    pub fn new() -> Self {
        Self { tasks: Vec::new() }
    }

    /// Add a task with explicit dependencies.
    /// Dependencies are referenced by the task names of other added tasks.
    pub fn add_task<F, T>(
        &mut self,
        name: &str,
        dependencies: Vec<String>,
        work: impl FnOnce(HashMap<String, serde_json::Value>) -> F + Send + 'static,
    ) where
        F: Future<Output = Result<T>> + Send + 'static,
        T: serde::Serialize + Send + 'static,
    {
        self.tasks.push(Box::new(ErasedTask {
            name: name.to_string(),
            dependencies,
            work: Some(Box::new(work)),
            _phantom: std::marker::PhantomData,
        }));
    }

    /// Add a task with no dependencies.
    pub fn add_independent<F, T>(
        &mut self,
        name: &str,
        work: impl FnOnce() -> F + Send + 'static,
    ) where
        F: Future<Output = Result<T>> + Send + 'static,
        T: serde::Serialize + Send + 'static,
    {
        self.tasks.push(Box::new(ErasedTask {
            name: name.to_string(),
            dependencies: vec![],
            work: Some(Box::new(move |_deps: HashMap<String, serde_json::Value>| work())),
            _phantom: std::marker::PhantomData,
        }));
    }

    /// Execute all tasks, respecting dependency order.
    /// Independent tasks run in parallel within each layer.
    pub async fn execute(self) -> Vec<TaskOutput> {
        let task_count = self.tasks.len();
        if task_count == 0 {
            return vec![];
        }

        // Build dependency graph
        let name_to_idx: HashMap<String, usize> = self.tasks.iter()
            .enumerate()
            .map(|(i, t)| (t.name().to_string(), i))
            .collect();

        let mut in_degree = vec![0u32; task_count];
        let mut dependents: Vec<Vec<usize>> = vec![vec![]; task_count];

        for (i, task) in self.tasks.iter().enumerate() {
            for dep_name in task.dependencies() {
                if let Some(&dep_idx) = name_to_idx.get(dep_name) {
                    in_degree[i] += 1;
                    dependents[dep_idx].push(i);
                }
            }
        }

        // Topological sort into execution layers
        let mut layers: Vec<Vec<usize>> = Vec::new();
        let mut queue: VecDeque<usize> = in_degree.iter()
            .enumerate()
            .filter(|(_, d)| **d == 0)
            .map(|(i, _)| i)
            .collect();

        while !queue.is_empty() {
            let layer: Vec<usize> = queue.drain(..).collect();
            layers.push(layer.clone());

            for idx in &layer {
                for dep in &dependents[*idx] {
                    in_degree[*dep] -= 1;
                    if in_degree[*dep] == 0 {
                        queue.push_back(*dep);
                    }
                }
            }
        }

        // Execute layer by layer using name-based lookup
        let mut task_map: HashMap<String, Box<dyn ExecutableTask>> = HashMap::new();
        for task in self.tasks {
            task_map.insert(task.name().to_string(), task);
        }

        // Execute in dependency order, parallel within layers
        let completed: Arc<Mutex<HashMap<String, serde_json::Value>>> = Arc::new(Mutex::new(HashMap::new()));
        let mut outputs = Vec::new();

        for layer in layers {
            let mut handles = Vec::new();
            for idx in &layer {
                let task_name = name_to_idx.iter()
                    .find(|(_, v)| **v == *idx)
                    .map(|(k, _)| k.clone())
                    .unwrap();

                if let Some(task) = task_map.remove(&task_name) {
                    let completed_clone = completed.clone();
                    let handle = tokio::spawn(async move {
                        let deps = completed_clone.lock().await.clone();
                        let output = task.execute(deps).await;
                        (output.name.clone(), output)
                    });
                    handles.push(handle);
                }
            }

            for handle in handles {
                let (name, output) = handle.await.unwrap_or_else(|e| {
                    ("unknown".to_string(), TaskOutput {
                        name: "unknown".to_string(),
                        success: false,
                        result: None,
                        error: Some(format!("Task panicked: {}", e)),
                        duration_ms: 0,
                    })
                });

                if output.success {
                    if let Some(ref result) = output.result {
                        if let Ok(value) = serde_json::from_str::<serde_json::Value>(result) {
                            completed.lock().await.insert(name.clone(), value);
                        }
                    }
                }
                outputs.push(output);
            }
        }

        outputs
    }
}

/// Simplified parallel execution: fire multiple independent async operations
/// and collect all results.
pub async fn parallel_execute<F, T>(
    tasks: Vec<(&str, F)>,
) -> Vec<TaskOutput>
where
    F: Future<Output = Result<T>> + Send + 'static,
    T: serde::Serialize + Send + 'static,
{
    let handles: Vec<_> = tasks.into_iter().map(|(name, future)| {
        let name = name.to_string();
        tokio::spawn(async move {
            let start = std::time::Instant::now();
            match future.await {
                Ok(value) => TaskOutput {
                    name: name.clone(),
                    success: true,
                    result: Some(serde_json::to_string(&value).unwrap_or_default()),
                    error: None,
                    duration_ms: start.elapsed().as_millis() as u64,
                },
                Err(e) => TaskOutput {
                    name,
                    success: false,
                    result: None,
                    error: Some(e.to_string()),
                    duration_ms: start.elapsed().as_millis() as u64,
                },
            }
        })
    }).collect();

    let mut outputs = Vec::new();
    for handle in handles {
        if let Ok(output) = handle.await {
            outputs.push(output);
        }
    }
    outputs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parallel_execute_independent_tasks() {
        let mut executor = ParallelExecutor::new();
        executor.add_independent("task_a", || async {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            Ok::<_, anyhow::Error>("result_a".to_string())
        });
        executor.add_independent("task_b", || async {
            Ok::<_, anyhow::Error>("result_b".to_string())
        });

        let results = executor.execute().await;
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.success));
    }

    #[tokio::test]
    async fn test_parallel_executor_dag() {
        let mut executor = ParallelExecutor::new();

        executor.add_independent("read_config", || async {
            Ok::<_, anyhow::Error>(serde_json::json!({"key": "value"}))
        });

        executor.add_independent("grep_main", || async {
            Ok::<_, anyhow::Error>("fn main() { }".to_string())
        });

        executor.add_task("analyze", vec!["read_config".to_string(), "grep_main".to_string()],
            |deps: HashMap<String, serde_json::Value>| async move {
                let config = deps.get("read_config").unwrap();
                let code = deps.get("grep_main").unwrap();
                Ok::<_, anyhow::Error>(format!("analyzed: {:?} + {:?}", config, code))
            },
        );

        let outputs = executor.execute().await;
        assert_eq!(outputs.len(), 3);
        assert!(outputs.iter().all(|o| o.success));
    }

    #[tokio::test]
    async fn test_parallel_executor_failure_propagation() {
        let mut executor = ParallelExecutor::new();

        executor.add_independent("failing", || async {
            Err::<String, _>(anyhow::anyhow!("intentional failure"))
        });

        executor.add_task("depends_on_fail", vec!["failing".to_string()],
            |_deps: HashMap<String, serde_json::Value>| async move {
                // Should still execute even if dependency failed
                Ok::<_, anyhow::Error>("ran anyway".to_string())
            },
        );

        let outputs = executor.execute().await;
        assert_eq!(outputs.len(), 2);
        let failing = outputs.iter().find(|o| o.name == "failing").unwrap();
        assert!(!failing.success);
        let dependent = outputs.iter().find(|o| o.name == "depends_on_fail").unwrap();
        assert!(dependent.success);
    }
}
