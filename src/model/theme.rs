use ratatui::style::Color;

pub struct Theme;

impl Theme {
    // ── Base palette ────────────────────────────────────────────
    pub const BACKGROUND: Color = Color::Rgb(18, 18, 24);
    pub const SURFACE: Color = Color::Rgb(28, 28, 38);
    pub const TEXT: Color = Color::Rgb(225, 225, 235);
    pub const MUTED_TEXT: Color = Color::Rgb(100, 105, 120);
    pub const SEPARATOR: Color = Color::Rgb(45, 45, 58);

    // ── Accent colors ───────────────────────────────────────────
    pub const ACCENT: Color = Color::Rgb(80, 200, 200);       // teal — primary accent
    pub const ACCENT_WARM: Color = Color::Rgb(230, 160, 60);  // amber — active/running
    pub const ACCENT_PURPLE: Color = Color::Rgb(170, 130, 255); // soft purple — agents

    // ── Semantic colors ─────────────────────────────────────────
    pub const SUCCESS: Color = Color::Rgb(80, 210, 120);
    pub const WARNING: Color = Color::Rgb(230, 180, 60);
    pub const ERROR: Color = Color::Rgb(230, 80, 80);
    pub const INFO: Color = Color::Rgb(80, 180, 220);

    // ── UI chrome ───────────────────────────────────────────────
    pub const HEADER_BG: Color = Color::Rgb(24, 24, 32);
    pub const FOOTER_BG: Color = Color::Rgb(24, 24, 32);
    pub const PANEL_BORDER: Color = Color::Rgb(50, 50, 65);
    pub const ACTIVE_BORDER: Color = Color::Rgb(80, 200, 200); // = ACCENT
    pub const SELECTION_BG: Color = Color::Rgb(40, 55, 75);
    pub const AGENT_LABEL: Color = Color::Rgb(170, 130, 255);  // = ACCENT_PURPLE

    // ── Task status ─────────────────────────────────────────────
    pub const TASK_PENDING: Color = Color::Rgb(100, 105, 120); // = MUTED_TEXT
    pub const TASK_RUNNING: Color = Color::Rgb(230, 160, 60);  // = ACCENT_WARM
    pub const TASK_IMPLEMENTED: Color = Color::Rgb(80, 180, 220); // = INFO
    pub const TASK_COMPLETED: Color = Color::Rgb(80, 210, 120);  // = SUCCESS
    pub const TASK_FAILED: Color = Color::Rgb(230, 80, 80);      // = ERROR

    // ── Tool colors (grouped by category) ───────────────────────
    // Execution
    pub const TOOL_BASH: Color = Color::Rgb(230, 160, 60);     // amber
    pub const TOOL_TASK: Color = Color::Rgb(200, 140, 50);     // dark amber
    // File I/O
    pub const TOOL_READ: Color = Color::Rgb(80, 180, 220);     // blue
    pub const TOOL_WRITE: Color = Color::Rgb(80, 210, 120);    // green
    pub const TOOL_EDIT: Color = Color::Rgb(120, 200, 170);    // teal-green
    // Search
    pub const TOOL_GREP: Color = Color::Rgb(170, 130, 255);    // purple
    pub const TOOL_GLOB: Color = Color::Rgb(140, 160, 255);    // blue-purple
    // Network
    pub const TOOL_WEBFETCH: Color = Color::Rgb(230, 120, 180); // pink

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
            _ => Self::MUTED_TEXT,
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
        assert_eq!(Theme::tool_color("Unknown"), Theme::MUTED_TEXT);
    }
}
