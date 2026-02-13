use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Frame;

use crate::app::state::{AppState, ViewState};

pub mod agent_detail;
pub mod components;
pub mod dashboard;
pub mod session_detail;
pub mod sessions;

pub use agent_detail::render_agent_detail;
pub use dashboard::render_dashboard;
pub use session_detail::render_session_detail;
pub use sessions::render_sessions;

/// Main view dispatcher.
/// Renders global header on all views, then routes content area to specific view.
/// Overlays filter bar and help if active.
pub fn render(state: &AppState, frame: &mut Frame) {
    // Global header + content split
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Global header
            Constraint::Min(0),   // Content area
        ])
        .split(frame.area());

    // Always render global header
    components::header::render_header(frame, layout[0], state);

    // Route content area to specific view
    match &state.ui.view {
        ViewState::Dashboard => {
            dashboard::render_dashboard(frame, state, layout[1]);
        }
        ViewState::AgentDetail => {
            agent_detail::render_agent_detail(frame, state, layout[1]);
        }
        ViewState::Sessions => {
            sessions::render_sessions(frame, state, layout[1]);
        }
        ViewState::SessionDetail => {
            session_detail::render_session_detail(frame, state, layout[1]);
        }
    }

    // Overlay filter bar if active
    if state.ui.filter.is_some() {
        components::filter_bar::render_filter_bar(frame, state);
    }

    // Overlay help if active (on top of filter bar)
    if state.ui.show_help {
        components::help_overlay::render_help_overlay(frame);
    }
}
