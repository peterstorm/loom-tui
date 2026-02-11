use ratatui::Frame;

use crate::app::state::{AppState, ViewState};

pub mod agent_detail;
pub mod components;
pub mod dashboard;
pub mod sessions;

pub use agent_detail::render_agent_detail;
pub use dashboard::render_dashboard;
pub use sessions::render_sessions;

/// Main view dispatcher.
/// Routes to specific view based on current ViewState.
/// Overlays filter bar and help if active.
pub fn render(state: &AppState, frame: &mut Frame) {
    // Main view dispatch based on ViewState
    match &state.view {
        ViewState::Dashboard => {
            dashboard::render_dashboard(frame, state);
        }
        ViewState::AgentDetail { agent_id } => {
            agent_detail::render_agent_detail(frame, state, agent_id);
        }
        ViewState::Sessions => {
            sessions::render_sessions(frame, state);
        }
    }

    // Overlay filter bar if active
    if state.filter.is_some() {
        components::filter_bar::render_filter_bar(frame, state);
    }

    // Overlay help if active (on top of filter bar)
    if state.show_help {
        components::help_overlay::render_help_overlay(frame);
    }
}
