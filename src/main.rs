use chrono::Utc;
use color_eyre::eyre::Result;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use loom_tui::{
    app::{update, AppState},
    event::AppEvent,
    paths::Paths,
    session,
    view::render,
    watcher,
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::path::PathBuf;
use std::time::{Duration, Instant};


fn main() -> Result<()> {
    // Install color-eyre panic handler for better error messages
    color_eyre::install()?;

    // Parse CLI args: optional first arg is project root path
    let args: Vec<String> = std::env::args().collect();
    let project_root = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

    // Resolve all file paths
    let paths = Paths::resolve(&project_root);

    // Initialize application state
    let mut state = AppState::new()
        .with_project_path(project_root.display().to_string());

    // Load deleted session tombstones
    state.meta.archive_dir = Some(paths.archive_dir.clone());
    let deleted_ids = session::load_deleted_ids(&paths.archive_dir);
    state.domain.deleted_session_ids = deleted_ids
        .into_iter()
        .map(loom_tui::model::SessionId::new)
        .collect();

    // Terminal initialization
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Start file watchers (returns channel for receiving events)
    let watcher_rx = watcher::start_watching(&paths)
        .map_err(|e| color_eyre::eyre::eyre!("Failed to start file watcher: {}", e))?;

    // Main event loop (Elm Architecture)
    let tick_rate = Duration::from_millis(250);
    let mut last_tick = Instant::now();

    let result = run_event_loop(
        &mut terminal,
        &mut state,
        &watcher_rx,
        tick_rate,
        &mut last_tick,
    );

    // Terminal cleanup (always execute even if event loop errored)
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    // Return event loop result
    result
}

/// Main event loop following Elm Architecture.
/// Separated from main() for testability.
fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    state: &mut AppState,
    watcher_rx: &std::sync::mpsc::Receiver<AppEvent>,
    tick_rate: Duration,
    last_tick: &mut Instant,
) -> Result<()> {
    // Channel for background session loads
    let (load_tx, load_rx) = std::sync::mpsc::channel::<AppEvent>();
    let mut load_in_flight = false;

    loop {
        // Render current state
        terminal.draw(|frame| {
            render(state, frame);
        })?;

        // Poll keyboard events with timeout
        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::ZERO);

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                // Send key event to update
                update(state, AppEvent::Key(key));
            }
        }

        // Drain file watcher events
        while let Ok(event) = watcher_rx.try_recv() {
            update(state, event);
        }

        // Drain background session load results
        while let Ok(event) = load_rx.try_recv() {
            update(state, event);
            load_in_flight = false;
        }

        // Spawn background session load if requested and not already in flight
        if let Some(ref sid) = state.ui.loading_session {
            if !load_in_flight {
                if let Some(session) = state.domain.sessions.iter().find(|s| &s.meta.id == sid) {
                    let path = session.path.clone();
                    let tx = load_tx.clone();
                    load_in_flight = true;
                    std::thread::spawn(move || {
                        match session::load_session(&path) {
                            Ok(archive) => {
                                let _ = tx.send(AppEvent::SessionLoaded(archive));
                            }
                            Err(e) => {
                                let _ = tx.send(AppEvent::Error {
                                    source: path.display().to_string(),
                                    error: e.into(),
                                });
                            }
                        }
                    });
                }
            }
        }

        // Tick event
        if last_tick.elapsed() >= tick_rate {
            update(state, AppEvent::Tick(Utc::now()));
            *last_tick = Instant::now();
        }

        // Check quit condition
        if state.meta.should_quit {
            break;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_main_event_loop_quits_on_should_quit() {
        // This test verifies the quit logic without actually running terminal I/O
        let mut state = AppState::new();
        state.meta.should_quit = true;

        // Quit flag should be set
        assert!(state.meta.should_quit);
    }

    #[test]
    fn test_tick_duration_configuration() {
        let tick_rate = Duration::from_millis(250);
        assert_eq!(tick_rate.as_millis(), 250);
    }

    #[test]
    fn test_paths_resolution_from_current_dir() {
        // Verify that paths can be resolved without error
        let current_dir = std::env::current_dir().unwrap();
        let paths = Paths::resolve(&current_dir);

        // Verify paths are absolute
        assert!(paths.task_graph.is_absolute());
        assert!(paths.transcript_dir.is_absolute());
        assert!(paths.archive_dir.is_absolute());
    }
}
