use std::process::Command;

/// Predefined tmux pane layout presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutPreset {
    /// 50/50 horizontal split
    Split,
    /// Horizontal split + vertical split on right
    Triple,
    /// 2x2 grid
    Grid,
}

impl LayoutPreset {
    pub const ALL: [LayoutPreset; 3] = [Self::Split, Self::Triple, Self::Grid];

    pub fn label(self) -> &'static str {
        match self {
            Self::Split => "Split (2 panes)",
            Self::Triple => "Triple (3 panes)",
            Self::Grid => "Grid (4 panes)",
        }
    }

    pub fn ascii_preview(self) -> &'static str {
        match self {
            Self::Split => concat!(
                "┌──────┬──────┐\n",
                "│      │      │\n",
                "│ loom │  sh  │\n",
                "│      │      │\n",
                "└──────┴──────┘",
            ),
            Self::Triple => concat!(
                "┌──────┬──────┐\n",
                "│      │  sh  │\n",
                "│ loom ├──────┤\n",
                "│      │  sh  │\n",
                "└──────┴──────┘",
            ),
            Self::Grid => concat!(
                "┌──────┬──────┐\n",
                "│ loom │  sh  │\n",
                "├──────┼──────┤\n",
                "│  sh  │  sh  │\n",
                "└──────┴──────┘",
            ),
        }
    }
}

/// Check if currently running inside a tmux session.
pub fn is_inside_tmux() -> bool {
    std::env::var("TMUX").is_ok()
}

/// Spawn panes according to the given layout preset.
/// Returns `Err` with stderr output on failure.
pub fn spawn_layout(preset: LayoutPreset) -> Result<(), String> {
    match preset {
        LayoutPreset::Split => {
            tmux(&["split-window", "-h", "-p", "50"])?;
            tmux(&["select-pane", "-t", "{left}"])?;
        }
        LayoutPreset::Triple => {
            tmux(&["split-window", "-h", "-p", "50"])?;
            tmux(&["split-window", "-v", "-t", "{right}", "-p", "50"])?;
            tmux(&["select-pane", "-t", "{left}"])?;
        }
        LayoutPreset::Grid => {
            tmux(&["split-window", "-h", "-p", "50"])?;
            tmux(&["split-window", "-v", "-t", "{right}", "-p", "50"])?;
            tmux(&["split-window", "-v", "-t", "{top-left}", "-p", "50"])?;
            tmux(&["select-pane", "-t", "{top-left}"])?;
        }
    }
    Ok(())
}

fn tmux(args: &[&str]) -> Result<(), String> {
    let output = Command::new("tmux")
        .args(args)
        .output()
        .map_err(|e| format!("tmux exec failed: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("tmux {}: {}", args.join(" "), stderr.trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_labels_not_empty() {
        for p in LayoutPreset::ALL {
            assert!(!p.label().is_empty());
        }
    }

    #[test]
    fn preset_ascii_previews_not_empty() {
        for p in LayoutPreset::ALL {
            assert!(!p.ascii_preview().is_empty());
        }
    }

    #[test]
    fn all_contains_three_presets() {
        assert_eq!(LayoutPreset::ALL.len(), 3);
    }
}
