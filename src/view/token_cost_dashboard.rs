use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Bar, BarChart, BarGroup, Block, Borders, Paragraph, Row, Table},
    Frame,
};

use crate::app::state::AppState;
use crate::model::{Agent, AgentId, SessionId, Theme};
use super::components::footer::render_footer;
use super::components::format::{format_cost_usd, format_token_count};

// ── Pricing (per 1M tokens, in cents) ──────────────────────────────────────

const OPUS_INPUT_PER_M: u64 = 1_500;   // $15
const OPUS_OUTPUT_PER_M: u64 = 7_500;   // $75
const SONNET_INPUT_PER_M: u64 = 300;    // $3
const SONNET_OUTPUT_PER_M: u64 = 1_500;  // $15
const HAIKU_INPUT_PER_M: u64 = 80;      // $0.80
const HAIKU_OUTPUT_PER_M: u64 = 400;     // $4

// ── Aggregation types ──────────────────────────────────────────────────────

struct SessionTokenSummary {
    id: SessionId,
    date: DateTime<Utc>,
    model: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_tokens: u64,
    estimated_cost_cents: u64,
}

struct ModelBreakdown {
    model: String,
    input_tokens: u64,
    output_tokens: u64,
    estimated_cost_cents: u64,
}

struct DashboardData {
    sessions: Vec<SessionTokenSummary>,
    by_model: Vec<ModelBreakdown>,
    total_input: u64,
    total_output: u64,
    total_cache: u64,
    total_cost_cents: u64,
}

// ── Pure functions ─────────────────────────────────────────────────────────

fn estimate_cost_cents(model: &str, input: u64, output: u64) -> u64 {
    let lower = model.to_lowercase();
    let (input_rate, output_rate) = if lower.contains("opus") {
        (OPUS_INPUT_PER_M, OPUS_OUTPUT_PER_M)
    } else if lower.contains("haiku") {
        (HAIKU_INPUT_PER_M, HAIKU_OUTPUT_PER_M)
    } else {
        // sonnet fallback
        (SONNET_INPUT_PER_M, SONNET_OUTPUT_PER_M)
    };
    // cost = tokens * rate_per_million / 1_000_000
    (input * input_rate + output * output_rate) / 1_000_000
}

/// Determine the dominant model from a set of agents (model with most API tokens).
/// Uses api_tokens() (input+output) to avoid cache_read inflation.
/// Skips agents with no model set to avoid misattribution.
fn dominant_model(agents: &BTreeMap<AgentId, Agent>) -> String {
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    for agent in agents.values() {
        if let Some(ref model) = agent.model {
            *counts.entry(model.clone()).or_default() += agent.token_usage.api_tokens();
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(m, _)| m)
        .unwrap_or_else(|| "unknown".to_string())
}

/// Aggregate token data from active sessions and loaded archives.
fn aggregate(state: &AppState) -> DashboardData {
    let mut sessions = Vec::new();
    let mut model_map: BTreeMap<String, (u64, u64, u64)> = BTreeMap::new(); // model -> (input, output, cost)

    // Active sessions: gather agents by session_id.
    // Agents created via AgentMetadataUpdated often lack session_id,
    // so also match unattributed agents to the sole active session.
    let active_sessions: Vec<_> = state.domain.confirmed_active_sessions().collect();
    let single_active_sid = if active_sessions.len() == 1 {
        Some(active_sessions[0].0.clone())
    } else {
        None
    };

    for &(sid, meta) in &active_sessions {
        // Session-level tokens from main transcript (orchestrator)
        let main_input = meta.token_usage.input_tokens;
        let main_output = meta.token_usage.output_tokens;
        let main_cache = meta.token_usage.cache_creation_input_tokens
            + meta.token_usage.cache_read_input_tokens;
        let main_model = meta.model.clone().unwrap_or_else(|| "unknown".to_string());

        // Subagent tokens
        let session_agents: BTreeMap<AgentId, Agent> = state
            .domain
            .agents
            .iter()
            .filter(|(_, a)| {
                a.session_id.as_ref() == Some(sid)
                    || (a.session_id.is_none() && single_active_sid.as_ref() == Some(sid))
            })
            .map(|(id, a)| (id.clone(), a.clone()))
            .collect();

        let (agent_input, agent_output, agent_cache) = sum_tokens(&session_agents);

        let total_input = main_input + agent_input;
        let total_output = main_output + agent_output;
        let total_cache = main_cache + agent_cache;

        if total_input == 0 && total_output == 0 {
            continue;
        }

        // Cost: main transcript + per-agent
        let main_cost = estimate_cost_cents(&main_model, main_input, main_output);
        let agent_cost = estimate_session_cost(&session_agents);
        let total_cost = main_cost + agent_cost;

        // Model display: use main orchestrator model
        let display_model = if main_input + main_output > 0 {
            main_model.clone()
        } else {
            dominant_model(&session_agents)
        };

        // Per-model breakdown: main orchestrator (normalize to short name)
        if main_input + main_output > 0 {
            let entry = model_map.entry(short_model(&main_model)).or_default();
            entry.0 += main_input;
            entry.1 += main_output;
            entry.2 += main_cost;
        }
        // Per-model breakdown: subagents (normalize to short name)
        for agent in session_agents.values() {
            if let Some(ref m) = agent.model {
                let entry = model_map.entry(short_model(m)).or_default();
                entry.0 += agent.token_usage.input_tokens;
                entry.1 += agent.token_usage.output_tokens;
                entry.2 += estimate_cost_cents(m, agent.token_usage.input_tokens, agent.token_usage.output_tokens);
            }
        }

        sessions.push(SessionTokenSummary {
            id: sid.clone(),
            date: meta.timestamp,
            model: display_model,
            input_tokens: total_input,
            output_tokens: total_output,
            cache_tokens: total_cache,
            estimated_cost_cents: total_cost,
        });
    }

    // Archived sessions (loaded only)
    for archived in &state.domain.sessions {
        if let Some(ref data) = archived.data {
            if data.agents.is_empty() {
                continue;
            }

            let (input, output, cache) = sum_tokens(&data.agents);
            let model = dominant_model(&data.agents);
            let cost = estimate_session_cost(&data.agents);

            for agent in data.agents.values() {
                let m = agent.model.as_deref().unwrap_or("unknown");
                let entry = model_map.entry(short_model(m)).or_default();
                entry.0 += agent.token_usage.input_tokens;
                entry.1 += agent.token_usage.output_tokens;
                entry.2 += estimate_cost_cents(
                    agent.model.as_deref().unwrap_or("unknown"),
                    agent.token_usage.input_tokens,
                    agent.token_usage.output_tokens,
                );
            }

            sessions.push(SessionTokenSummary {
                id: archived.meta.id.clone(),
                date: archived.meta.timestamp,
                model,
                input_tokens: input,
                output_tokens: output,
                cache_tokens: cache,
                estimated_cost_cents: cost,
            });
        }
    }

    // Sort sessions by date descending
    sessions.sort_by(|a, b| b.date.cmp(&a.date));

    let by_model: Vec<ModelBreakdown> = model_map
        .into_iter()
        .filter(|(model, _)| model != "unknown" && !model.contains("synthetic"))
        .map(|(model, (input, output, cost))| ModelBreakdown {
            model,
            input_tokens: input,
            output_tokens: output,
            estimated_cost_cents: cost,
        })
        .collect();

    let total_input: u64 = sessions.iter().map(|s| s.input_tokens).sum();
    let total_output: u64 = sessions.iter().map(|s| s.output_tokens).sum();
    let total_cache: u64 = sessions.iter().map(|s| s.cache_tokens).sum();
    let total_cost_cents: u64 = sessions.iter().map(|s| s.estimated_cost_cents).sum();

    DashboardData {
        sessions,
        by_model,
        total_input,
        total_output,
        total_cache,
        total_cost_cents,
    }
}

fn sum_tokens(agents: &BTreeMap<AgentId, Agent>) -> (u64, u64, u64) {
    agents.values().fold((0, 0, 0), |(i, o, c), a| {
        (
            i + a.token_usage.input_tokens,
            o + a.token_usage.output_tokens,
            c + a.token_usage.cache_creation_input_tokens + a.token_usage.cache_read_input_tokens,
        )
    })
}

/// Estimate cost per-agent (each agent's tokens × its own model's rate).
fn estimate_session_cost(agents: &BTreeMap<AgentId, Agent>) -> u64 {
    agents.values().map(|a| {
        let model = a.model.as_deref().unwrap_or("unknown");
        estimate_cost_cents(model, a.token_usage.input_tokens, a.token_usage.output_tokens)
    }).sum()
}

// ── Render ──────────────────────────────────────────────────────────────────

pub fn render_token_cost_dashboard(frame: &mut Frame, state: &AppState, area: Rect) {
    let data = aggregate(state);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Stats row
            Constraint::Min(10),  // Content
            Constraint::Length(1), // Footer
        ])
        .split(area);

    render_stats_row(frame, layout[0], &data);

    if data.sessions.is_empty() {
        render_empty_state(frame, layout[1]);
    } else {
        let content = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(layout[1]);

        render_session_table(frame, content[0], &data, state);
        render_model_chart(frame, content[1], &data);
    }

    render_footer(frame, layout[2], state);
}

fn render_stats_row(frame: &mut Frame, area: Rect, data: &DashboardData) {
    let api_tokens = data.total_input + data.total_output;
    let stats = format!(
        " {} sessions │ ~{} tokens │ {} cache │ {} est.",
        data.sessions.len(),
        format_token_count(api_tokens),
        format_token_count(data.total_cache),
        format_cost_usd(data.total_cost_cents),
    );

    let paragraph = Paragraph::new(Line::from(vec![
        Span::styled("Token Cost Dashboard", Style::default().fg(Theme::ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled(
            format!("  {stats}"),
            Style::default().fg(Theme::MUTED_TEXT),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Theme::PANEL_BORDER)),
    );

    frame.render_widget(paragraph, area);
}

fn render_session_table(frame: &mut Frame, area: Rect, data: &DashboardData, state: &AppState) {
    let header = Row::new(vec!["Session", "Date", "Model", "Tokens", "Cache", "Cost"])
        .style(
            Style::default()
                .fg(Theme::INFO)
                .add_modifier(Modifier::BOLD),
        );

    let scroll = state.ui.scroll_offsets.token_dashboard_left;
    let selected = state.ui.scroll_offsets.token_dashboard_left; // selection = scroll offset for this view

    let rows: Vec<Row> = data
        .sessions
        .iter()
        .enumerate()
        .map(|(idx, s)| {
            let is_selected = idx == selected;
            let total = s.input_tokens + s.output_tokens; // API tokens only, excludes cache
            let style = if is_selected {
                Style::default()
                    .bg(Theme::SELECTION_BG)
                    .fg(Theme::TEXT)
            } else {
                Style::default().fg(Theme::TEXT)
            };

            Row::new(vec![
                truncate_id(&s.id, 12),
                s.date.format("%m-%d %H:%M").to_string(),
                short_model(&s.model),
                format_token_count(total),
                format_token_count(s.cache_tokens),
                format_cost_usd(s.estimated_cost_cents),
            ])
            .style(style)
        })
        .collect();

    let widths = [
        Constraint::Length(12), // Session ID
        Constraint::Length(12), // Date
        Constraint::Length(8),  // Model
        Constraint::Length(8),  // Tokens
        Constraint::Length(8),  // Cache
        Constraint::Length(8),  // Cost
    ];

    let is_focused = matches!(state.ui.focus, crate::app::PanelFocus::Left);

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .title(" Per-Session ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(if is_focused {
                    Theme::ACTIVE_BORDER
                } else {
                    Theme::PANEL_BORDER
                })),
        )
        .row_highlight_style(
            Style::default()
                .bg(Theme::SELECTION_BG)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    // Skip rows for scrolling
    let _ = scroll; // scroll offset used in row selection above
    frame.render_widget(table, area);
}

fn render_model_chart(frame: &mut Frame, area: Rect, data: &DashboardData) {
    if data.by_model.is_empty() {
        let empty = Paragraph::new("No token data")
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .title(" Per-Model ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Theme::PANEL_BORDER)),
            );
        frame.render_widget(empty, area);
        return;
    }

    let bars: Vec<Bar> = data
        .by_model
        .iter()
        .map(|m| {
            let total = m.input_tokens + m.output_tokens; // API tokens
            let color = model_color(&m.model);
            Bar::default()
                .label(Line::from(short_model(&m.model)))
                .value(total)
                .text_value(format!("{} ({})", format_token_count(total), format_cost_usd(m.estimated_cost_cents)))
                .style(Style::default().fg(color))
        })
        .collect();

    let chart = BarChart::default()
        .block(
            Block::default()
                .title(" Per-Model Tokens ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Theme::PANEL_BORDER)),
        )
        .data(BarGroup::default().bars(&bars))
        .bar_width(
            area.width
                .saturating_sub(4)
                .checked_div(data.by_model.len() as u16 + 1)
                .unwrap_or(5)
                .clamp(5, 15),
        )
        .bar_gap(1)
        .direction(Direction::Vertical);

    frame.render_widget(chart, area);
}

fn render_empty_state(frame: &mut Frame, area: Rect) {
    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "No token data available",
            Style::default()
                .fg(Theme::MUTED_TEXT)
                .add_modifier(Modifier::ITALIC),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Token data appears as agents process requests",
            Style::default().fg(Theme::MUTED_TEXT),
        )),
    ];

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .title(" Token Cost Dashboard ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Theme::PANEL_BORDER)),
        )
        .alignment(Alignment::Center);

    frame.render_widget(paragraph, area);
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn truncate_id(id: &SessionId, max: usize) -> String {
    let s = id.to_string();
    if s.len() <= max {
        s
    } else {
        format!("{}…", &s[..max - 1])
    }
}

fn short_model(model: &str) -> String {
    let lower = model.to_lowercase();
    if lower.contains("opus") {
        "opus".to_string()
    } else if lower.contains("haiku") {
        "haiku".to_string()
    } else if lower.contains("sonnet") {
        "sonnet".to_string()
    } else {
        model.to_string()
    }
}

fn model_color(model: &str) -> ratatui::style::Color {
    let lower = model.to_lowercase();
    if lower.contains("opus") {
        Theme::ACCENT_PURPLE
    } else if lower.contains("haiku") {
        Theme::SUCCESS
    } else {
        Theme::ACCENT // sonnet = teal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::AppState;
    use crate::model::{Agent, ArchivedSession, SessionArchive, SessionMeta, SessionStatus, TokenUsage};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::path::PathBuf;

    #[test]
    fn estimate_cost_opus() {
        // 1M input + 1M output at opus rates = $15 + $75 = $90 = 9000 cents
        let cost = estimate_cost_cents("claude-opus-4-6", 1_000_000, 1_000_000);
        assert_eq!(cost, 9000);
    }

    #[test]
    fn estimate_cost_sonnet() {
        let cost = estimate_cost_cents("claude-sonnet-4-6", 1_000_000, 1_000_000);
        assert_eq!(cost, 1800); // $3 + $15 = $18 = 1800 cents
    }

    #[test]
    fn estimate_cost_haiku() {
        let cost = estimate_cost_cents("claude-haiku-4-5", 1_000_000, 1_000_000);
        assert_eq!(cost, 480); // $0.80 + $4 = $4.80 = 480 cents
    }

    #[test]
    fn estimate_cost_unknown_falls_back_to_sonnet() {
        let cost = estimate_cost_cents("unknown-model", 1_000_000, 1_000_000);
        assert_eq!(cost, 1800);
    }

    #[test]
    fn dominant_model_selects_highest_api_tokens() {
        let mut agents = BTreeMap::new();
        let mut a1 = Agent::new("a1", Utc::now());
        a1.model = Some("opus".to_string());
        a1.token_usage = TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            ..Default::default()
        };
        agents.insert(AgentId::new("a1"), a1);

        let mut a2 = Agent::new("a2", Utc::now());
        a2.model = Some("sonnet".to_string());
        a2.token_usage = TokenUsage {
            input_tokens: 1000,
            output_tokens: 500,
            ..Default::default()
        };
        agents.insert(AgentId::new("a2"), a2);

        assert_eq!(dominant_model(&agents), "sonnet");
    }

    #[test]
    fn dominant_model_ignores_cache_tokens() {
        let mut agents = BTreeMap::new();
        // haiku agent with huge cache but small API tokens
        let mut a1 = Agent::new("a1", Utc::now());
        a1.model = Some("haiku".to_string());
        a1.token_usage = TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_input_tokens: 10_000_000,
            ..Default::default()
        };
        agents.insert(AgentId::new("a1"), a1);

        // opus agent with more API tokens
        let mut a2 = Agent::new("a2", Utc::now());
        a2.model = Some("opus".to_string());
        a2.token_usage = TokenUsage {
            input_tokens: 5000,
            output_tokens: 2000,
            ..Default::default()
        };
        agents.insert(AgentId::new("a2"), a2);

        assert_eq!(dominant_model(&agents), "opus");
    }

    #[test]
    fn aggregate_empty_state() {
        let state = AppState::new();
        let data = aggregate(&state);
        assert!(data.sessions.is_empty());
        assert!(data.by_model.is_empty());
        assert_eq!(data.total_cost_cents, 0);
    }

    #[test]
    fn aggregate_with_archived_session() {
        let mut state = AppState::new();
        let mut agents = BTreeMap::new();
        let mut a = Agent::new("a1", Utc::now());
        a.model = Some("sonnet".to_string());
        a.token_usage = TokenUsage {
            input_tokens: 500_000,
            output_tokens: 100_000,
            cache_creation_input_tokens: 50_000,
            cache_read_input_tokens: 10_000,
        };
        agents.insert(AgentId::new("a1"), a);

        let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string())
            .with_status(SessionStatus::Completed);
        let archive = SessionArchive::new(meta.clone()).with_agents(agents);
        state.domain.sessions.push(
            ArchivedSession::new(meta, PathBuf::new()).with_data(archive),
        );

        let data = aggregate(&state);
        assert_eq!(data.sessions.len(), 1);
        assert_eq!(data.sessions[0].input_tokens, 500_000);
        assert_eq!(data.sessions[0].output_tokens, 100_000);
        assert!(data.total_cost_cents > 0);
    }

    #[test]
    fn aggregate_includes_unattributed_agents_in_single_active_session() {
        let mut state = AppState::new();
        let sid = SessionId::new("s1");

        // Create a confirmed active session
        let mut meta = SessionMeta::new(sid.clone(), Utc::now(), "/proj".to_string());
        meta.confirmed = true;
        state.domain.active_sessions.insert(sid.clone(), meta);

        // Agent WITHOUT session_id (like AgentMetadataUpdated creates)
        let mut a = Agent::new("a1", Utc::now());
        a.model = Some("sonnet".to_string());
        a.token_usage = TokenUsage {
            input_tokens: 100_000,
            output_tokens: 50_000,
            ..Default::default()
        };
        // Note: a.session_id is None
        state.domain.agents.insert(AgentId::new("a1"), a);

        let data = aggregate(&state);
        assert_eq!(data.sessions.len(), 1);
        assert_eq!(data.sessions[0].input_tokens, 100_000);
        assert!(data.total_cost_cents > 0);
    }

    #[test]
    fn short_model_names() {
        assert_eq!(short_model("claude-opus-4-6"), "opus");
        assert_eq!(short_model("claude-sonnet-4-6"), "sonnet");
        assert_eq!(short_model("claude-haiku-4-5"), "haiku");
        assert_eq!(short_model("custom-model"), "custom-model");
    }

    #[test]
    fn truncate_id_short() {
        let id = SessionId::new("abc");
        assert_eq!(truncate_id(&id, 12), "abc");
    }

    #[test]
    fn truncate_id_long() {
        let id = SessionId::new("abcdefghijklmnop");
        let result = truncate_id(&id, 12);
        assert_eq!(result.chars().count(), 12);
        assert!(result.ends_with('…'));
    }

    #[test]
    fn render_does_not_panic_empty() {
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = AppState::new();

        terminal
            .draw(|frame| render_token_cost_dashboard(frame, &state, frame.area()))
            .unwrap();
    }

    #[test]
    fn render_does_not_panic_with_data() {
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut state = AppState::new();

        let mut agents = BTreeMap::new();
        let mut a = Agent::new("a1", Utc::now());
        a.model = Some("sonnet".to_string());
        a.token_usage = TokenUsage {
            input_tokens: 100_000,
            output_tokens: 50_000,
            ..Default::default()
        };
        agents.insert(AgentId::new("a1"), a);

        let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string())
            .with_status(SessionStatus::Completed);
        let archive = SessionArchive::new(meta.clone()).with_agents(agents);
        state.domain.sessions.push(
            ArchivedSession::new(meta, PathBuf::new()).with_data(archive),
        );

        terminal
            .draw(|frame| render_token_cost_dashboard(frame, &state, frame.area()))
            .unwrap();
    }
}
