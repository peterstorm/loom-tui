use ratatui::style::Color;

pub struct Theme;

impl Theme {
    // Task status colors
    pub const TASK_PENDING: Color = Color::Gray;
    pub const TASK_RUNNING: Color = Color::Yellow;
    pub const TASK_IMPLEMENTED: Color = Color::Blue;
    pub const TASK_COMPLETED: Color = Color::Green;
    pub const TASK_FAILED: Color = Color::Red;

    // Tool type colors
    pub const TOOL_BASH: Color = Color::Cyan;
    pub const TOOL_READ: Color = Color::Blue;
    pub const TOOL_WRITE: Color = Color::Green;
    pub const TOOL_EDIT: Color = Color::Yellow;
    pub const TOOL_GREP: Color = Color::Magenta;
    pub const TOOL_GLOB: Color = Color::White;
    pub const TOOL_TASK: Color = Color::Rgb(0, 255, 255); // bright cyan
    pub const TOOL_WEBFETCH: Color = Color::Rgb(255, 0, 255); // bright magenta

    // UI element colors (dark theme)
    pub const HEADER_BG: Color = Color::Rgb(30, 30, 40);
    pub const FOOTER_BG: Color = Color::Rgb(30, 30, 40);
    pub const PANEL_BORDER: Color = Color::Rgb(60, 60, 70);
    pub const ACTIVE_BORDER: Color = Color::Cyan;
    pub const MUTED_TEXT: Color = Color::Rgb(120, 120, 130);

    // General colors
    pub const BACKGROUND: Color = Color::Rgb(20, 20, 25);
    pub const TEXT: Color = Color::Rgb(220, 220, 230);
    pub const SUCCESS: Color = Color::Green;
    pub const WARNING: Color = Color::Yellow;
    pub const ERROR: Color = Color::Red;
    pub const INFO: Color = Color::Cyan;

    /// Get color for task status
    pub fn task_status_color(status: &crate::model::TaskStatus) -> Color {
        use crate::model::TaskStatus;
        match status {
            TaskStatus::Pending => Self::TASK_PENDING,
            TaskStatus::Running => Self::TASK_RUNNING,
            TaskStatus::Implemented => Self::TASK_IMPLEMENTED,
            TaskStatus::Completed => Self::TASK_COMPLETED,
            TaskStatus::Failed { .. } => Self::TASK_FAILED,
        }
    }

    /// Get color for tool name
    pub fn tool_color(tool_name: &str) -> Color {
        match tool_name {
            "Bash" => Self::TOOL_BASH,
            "Read" => Self::TOOL_READ,
            "Write" => Self::TOOL_WRITE,
            "Edit" => Self::TOOL_EDIT,
            "Grep" => Self::TOOL_GREP,
            "Glob" => Self::TOOL_GLOB,
            "Task" | "TaskCreate" | "TaskUpdate" | "TaskGet" | "TaskList" => Self::TOOL_TASK,
            "WebFetch" | "WebSearch" => Self::TOOL_WEBFETCH,
            _ => Color::White,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TaskStatus;

    #[test]
    fn task_status_colors_defined() {
        assert_eq!(
            Theme::task_status_color(&TaskStatus::Pending),
            Theme::TASK_PENDING
        );
        assert_eq!(
            Theme::task_status_color(&TaskStatus::Running),
            Theme::TASK_RUNNING
        );
        assert_eq!(
            Theme::task_status_color(&TaskStatus::Completed),
            Theme::TASK_COMPLETED
        );
        assert_eq!(
            Theme::task_status_color(&TaskStatus::Failed {
                reason: "test".into(),
                retry_count: 0
            }),
            Theme::TASK_FAILED
        );
    }

    #[test]
    fn tool_colors_defined() {
        assert_eq!(Theme::tool_color("Bash"), Theme::TOOL_BASH);
        assert_eq!(Theme::tool_color("Read"), Theme::TOOL_READ);
        assert_eq!(Theme::tool_color("TaskCreate"), Theme::TOOL_TASK);
        assert_eq!(Theme::tool_color("Unknown"), Color::White);
    }
}
