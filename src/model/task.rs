use serde::{Deserialize, Serialize};

use super::ids::{AgentId, TaskId};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskGraph {
    pub waves: Vec<Wave>,
    total_tasks: usize,
    completed_tasks: usize,
}

impl TaskGraph {
    pub fn new(waves: Vec<Wave>) -> Self {
        let total_tasks = waves.iter().map(|w| w.tasks.len()).sum();
        let completed_tasks = waves
            .iter()
            .flat_map(|w| &w.tasks)
            .filter(|t| matches!(t.status, TaskStatus::Completed))
            .count();

        Self {
            waves,
            total_tasks,
            completed_tasks,
        }
    }

    pub fn empty() -> Self {
        Self {
            waves: Vec::new(),
            total_tasks: 0,
            completed_tasks: 0,
        }
    }

    pub fn total_tasks(&self) -> usize {
        self.total_tasks
    }

    pub fn completed_tasks(&self) -> usize {
        self.completed_tasks
    }

    /// Returns an iterator over all tasks across all waves, flattened.
    pub fn flat_tasks(&self) -> impl Iterator<Item = &Task> {
        self.waves.iter().flat_map(|w| &w.tasks)
    }

    /// Calculate current wave number.
    /// Current wave = first wave with incomplete tasks, or last wave if all complete.
    pub fn current_wave(&self) -> u32 {
        for wave in &self.waves {
            let all_complete = wave
                .tasks
                .iter()
                .all(|t| matches!(t.status, TaskStatus::Completed));

            if !all_complete {
                return wave.number;
            }
        }

        // All waves complete, return last wave number
        self.waves.last().map(|w| w.number).unwrap_or(0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Wave {
    pub number: u32,
    pub tasks: Vec<Task>,
}

impl Wave {
    pub fn new(number: u32, tasks: Vec<Task>) -> Self {
        Self { number, tasks }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Task {
    pub id: TaskId,
    pub description: String,
    #[serde(default)]
    pub agent_id: Option<AgentId>,
    pub status: TaskStatus,
    #[serde(default)]
    pub review_status: ReviewStatus,
    #[serde(default)]
    pub files_modified: Vec<String>,
    #[serde(default)]
    pub tests_passed: Option<bool>,
}

impl Task {
    pub fn new(id: impl Into<TaskId>, description: String, status: TaskStatus) -> Self {
        Self {
            id: id.into(),
            description,
            agent_id: None,
            status,
            review_status: ReviewStatus::Pending,
            files_modified: Vec::new(),
            tests_passed: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    #[default]
    Pending,
    Running,
    Implemented,
    Completed,
    Failed {
        reason: String,
        retry_count: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ReviewStatus {
    #[default]
    Pending,
    Passed,
    Blocked {
        critical: Vec<String>,
        advisory: Vec<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_graph_calculates_totals() {
        let waves = vec![
            Wave::new(
                1,
                vec![
                    Task::new("T1", "Task 1".to_string(), TaskStatus::Completed),
                    Task::new("T2", "Task 2".to_string(), TaskStatus::Running),
                ],
            ),
            Wave::new(
                2,
                vec![Task::new("T3", "Task 3".to_string(), TaskStatus::Pending)],
            ),
        ];

        let graph = TaskGraph::new(waves);
        assert_eq!(graph.total_tasks(), 3);
        assert_eq!(graph.completed_tasks(), 1);
    }

    #[test]
    fn task_status_serializes_correctly() {
        let pending = TaskStatus::Pending;
        let json = serde_json::to_string(&pending).unwrap();
        assert_eq!(json, "\"pending\"");

        let failed = TaskStatus::Failed {
            reason: "error".into(),
            retry_count: 2,
        };
        let json = serde_json::to_string(&failed).unwrap();
        assert!(json.contains("\"failed\""));
        assert!(json.contains("\"retry_count\":2"));
    }

    #[test]
    fn current_wave_returns_first_incomplete() {
        let waves = vec![
            Wave::new(
                1,
                vec![Task::new("T1", "Task 1".to_string(), TaskStatus::Completed)],
            ),
            Wave::new(
                2,
                vec![Task::new("T2", "Task 2".to_string(), TaskStatus::Running)],
            ),
            Wave::new(
                3,
                vec![Task::new("T3", "Task 3".to_string(), TaskStatus::Pending)],
            ),
        ];

        let graph = TaskGraph::new(waves);
        assert_eq!(graph.current_wave(), 2);
    }

    #[test]
    fn current_wave_returns_last_if_all_complete() {
        let waves = vec![
            Wave::new(
                1,
                vec![Task::new("T1", "Task 1".to_string(), TaskStatus::Completed)],
            ),
            Wave::new(
                2,
                vec![Task::new("T2", "Task 2".to_string(), TaskStatus::Completed)],
            ),
        ];

        let graph = TaskGraph::new(waves);
        assert_eq!(graph.current_wave(), 2);
    }

    #[test]
    fn current_wave_returns_zero_for_empty_graph() {
        let graph = TaskGraph::empty();
        assert_eq!(graph.current_wave(), 0);
    }

    #[test]
    fn task_graph_new_with_empty_waves() {
        let graph = TaskGraph::new(vec![]);
        assert_eq!(graph.total_tasks(), 0);
        assert_eq!(graph.completed_tasks(), 0);
        assert_eq!(graph.waves.len(), 0);
    }

    #[test]
    fn task_graph_new_all_completed() {
        let waves = vec![
            Wave::new(
                1,
                vec![
                    Task::new("T1", "Task 1".to_string(), TaskStatus::Completed),
                    Task::new("T2", "Task 2".to_string(), TaskStatus::Completed),
                ],
            ),
            Wave::new(
                2,
                vec![
                    Task::new("T3", "Task 3".to_string(), TaskStatus::Completed),
                ],
            ),
        ];

        let graph = TaskGraph::new(waves);
        assert_eq!(graph.total_tasks(), 3);
        assert_eq!(graph.completed_tasks(), 3);
    }

    #[test]
    fn task_graph_new_all_pending() {
        let waves = vec![
            Wave::new(
                1,
                vec![
                    Task::new("T1", "Task 1".to_string(), TaskStatus::Pending),
                    Task::new("T2", "Task 2".to_string(), TaskStatus::Pending),
                ],
            ),
            Wave::new(
                2,
                vec![
                    Task::new("T3", "Task 3".to_string(), TaskStatus::Pending),
                ],
            ),
        ];

        let graph = TaskGraph::new(waves);
        assert_eq!(graph.total_tasks(), 3);
        assert_eq!(graph.completed_tasks(), 0);
    }

    #[test]
    fn task_graph_new_mixed_statuses() {
        let waves = vec![
            Wave::new(
                1,
                vec![
                    Task::new("T1", "Task 1".to_string(), TaskStatus::Completed),
                    Task::new("T2", "Task 2".to_string(), TaskStatus::Running),
                    Task::new("T3", "Task 3".to_string(), TaskStatus::Pending),
                ],
            ),
            Wave::new(
                2,
                vec![
                    Task::new("T4", "Task 4".to_string(), TaskStatus::Implemented),
                    Task::new("T5", "Task 5".to_string(), TaskStatus::Failed {
                        reason: "error".to_string(),
                        retry_count: 1,
                    }),
                ],
            ),
        ];

        let graph = TaskGraph::new(waves);
        assert_eq!(graph.total_tasks(), 5);
        assert_eq!(graph.completed_tasks(), 1); // Only T1 is completed
    }

    #[test]
    fn task_graph_new_computes_totals_correctly() {
        let waves = vec![
            Wave::new(
                1,
                vec![
                    Task::new("T1", "Task 1".to_string(), TaskStatus::Completed),
                ],
            ),
            Wave::new(
                2,
                vec![
                    Task::new("T2", "Task 2".to_string(), TaskStatus::Completed),
                    Task::new("T3", "Task 3".to_string(), TaskStatus::Completed),
                ],
            ),
            Wave::new(
                3,
                vec![
                    Task::new("T4", "Task 4".to_string(), TaskStatus::Running),
                    Task::new("T5", "Task 5".to_string(), TaskStatus::Pending),
                    Task::new("T6", "Task 6".to_string(), TaskStatus::Pending),
                ],
            ),
        ];

        let graph = TaskGraph::new(waves);
        assert_eq!(graph.total_tasks(), 6);
        assert_eq!(graph.completed_tasks(), 3);
    }

    #[test]
    fn task_graph_new_single_wave_empty_tasks() {
        let waves = vec![Wave::new(1, vec![])];

        let graph = TaskGraph::new(waves);
        assert_eq!(graph.total_tasks(), 0);
        assert_eq!(graph.completed_tasks(), 0);
        assert_eq!(graph.waves.len(), 1);
    }

    #[test]
    fn task_graph_new_multiple_waves_some_empty() {
        let waves = vec![
            Wave::new(
                1,
                vec![Task::new("T1", "Task 1".to_string(), TaskStatus::Completed)],
            ),
            Wave::new(2, vec![]), // Empty wave
            Wave::new(
                3,
                vec![Task::new("T2", "Task 2".to_string(), TaskStatus::Pending)],
            ),
        ];

        let graph = TaskGraph::new(waves);
        assert_eq!(graph.total_tasks(), 2);
        assert_eq!(graph.completed_tasks(), 1);
        assert_eq!(graph.waves.len(), 3);
    }
}
