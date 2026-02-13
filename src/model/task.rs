use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskGraph {
    pub waves: Vec<Wave>,
    pub total_tasks: usize,
    pub completed_tasks: usize,
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
    pub id: String,
    pub description: String,
    #[serde(default)]
    pub agent_id: Option<String>,
    pub status: TaskStatus,
    #[serde(default)]
    pub review_status: ReviewStatus,
    #[serde(default)]
    pub files_modified: Vec<String>,
    #[serde(default)]
    pub tests_passed: Option<bool>,
}

impl Task {
    pub fn new(id: String, description: String, status: TaskStatus) -> Self {
        Self {
            id,
            description,
            agent_id: None,
            status,
            review_status: ReviewStatus::Pending,
            files_modified: Vec::new(),
            tests_passed: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Running,
    Implemented,
    Completed,
    Failed {
        reason: String,
        retry_count: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ReviewStatus {
    Pending,
    Passed,
    Blocked {
        critical: Vec<String>,
        advisory: Vec<String>,
    },
}

impl Default for TaskStatus {
    fn default() -> Self {
        Self::Pending
    }
}

impl Default for ReviewStatus {
    fn default() -> Self {
        Self::Pending
    }
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
                    Task::new("T1".into(), "Task 1".into(), TaskStatus::Completed),
                    Task::new("T2".into(), "Task 2".into(), TaskStatus::Running),
                ],
            ),
            Wave::new(
                2,
                vec![Task::new("T3".into(), "Task 3".into(), TaskStatus::Pending)],
            ),
        ];

        let graph = TaskGraph::new(waves);
        assert_eq!(graph.total_tasks, 3);
        assert_eq!(graph.completed_tasks, 1);
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
}
