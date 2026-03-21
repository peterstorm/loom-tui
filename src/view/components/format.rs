use std::time::Duration;

/// Format elapsed seconds as human-readable string.
/// - < 60s: "Xs"
/// - < 3600s: "XmYs"
/// - >= 3600s: "XhYm"
pub fn format_elapsed(secs: i64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    }
}

/// Format duration as human-readable string, or "—" if None.
/// - None: "—"
/// - < 60s: "Xs"
/// - < 3600s: "Xm Ys"
/// - >= 3600s: "Xh Ym"
pub fn format_duration(duration: Option<Duration>) -> String {
    match duration {
        Some(d) => {
            let secs = d.as_secs();
            let mins = secs / 60;
            let hours = mins / 60;
            if hours > 0 {
                format!("{}h {}m", hours, mins % 60)
            } else if mins > 0 {
                format!("{}m {}s", mins, secs % 60)
            } else {
                format!("{}s", secs)
            }
        }
        None => "—".to_string(),
    }
}

/// Format a token count for compact display: 42k, 1.2M, etc.
pub fn format_token_count(n: u64) -> String {
    if n >= 1_000_000 {
        let m = n as f64 / 1_000_000.0;
        if m >= 10.0 {
            format!("{}M", m as u64)
        } else {
            format!("{:.1}M", m)
        }
    } else if n >= 1_000 {
        let k = n as f64 / 1_000.0;
        if k >= 10.0 {
            format!("{}k", k as u64)
        } else {
            format!("{:.1}k", k)
        }
    } else {
        format!("{}", n)
    }
}

/// Format cost in cents as USD string: 0 → "$0.00", 123 → "$1.23", 1234 → "$12.34"
pub fn format_cost_usd(cents: u64) -> String {
    let dollars = cents / 100;
    let remainder = cents % 100;
    format!("${}.{:02}", dollars, remainder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_elapsed_zero() {
        assert_eq!(format_elapsed(0), "0s");
    }

    #[test]
    fn format_elapsed_seconds() {
        assert_eq!(format_elapsed(45), "45s");
        assert_eq!(format_elapsed(59), "59s");
    }

    #[test]
    fn format_elapsed_minutes() {
        assert_eq!(format_elapsed(60), "1m0s");
        assert_eq!(format_elapsed(125), "2m5s");
        assert_eq!(format_elapsed(3599), "59m59s");
    }

    #[test]
    fn format_elapsed_hours() {
        assert_eq!(format_elapsed(3600), "1h0m");
        assert_eq!(format_elapsed(3661), "1h1m");
        assert_eq!(format_elapsed(7265), "2h1m");
    }

    #[test]
    fn format_elapsed_negative() {
        // Edge case: negative elapsed (shouldn't happen but handle gracefully)
        assert_eq!(format_elapsed(-10), "-10s");
    }

    #[test]
    fn format_duration_none() {
        assert_eq!(format_duration(None), "—");
    }

    #[test]
    fn format_duration_zero() {
        assert_eq!(format_duration(Some(Duration::from_secs(0))), "0s");
    }

    #[test]
    fn format_duration_seconds() {
        assert_eq!(format_duration(Some(Duration::from_secs(30))), "30s");
        assert_eq!(format_duration(Some(Duration::from_secs(59))), "59s");
    }

    #[test]
    fn format_duration_minutes() {
        assert_eq!(format_duration(Some(Duration::from_secs(60))), "1m 0s");
        assert_eq!(format_duration(Some(Duration::from_secs(90))), "1m 30s");
        assert_eq!(format_duration(Some(Duration::from_secs(3599))), "59m 59s");
    }

    #[test]
    fn format_duration_hours() {
        assert_eq!(format_duration(Some(Duration::from_secs(3600))), "1h 0m");
        assert_eq!(format_duration(Some(Duration::from_secs(3665))), "1h 1m");
        assert_eq!(format_duration(Some(Duration::from_secs(7265))), "2h 1m");
    }

    #[test]
    fn format_token_count_small() {
        assert_eq!(format_token_count(500), "500");
    }

    #[test]
    fn format_token_count_thousands() {
        assert_eq!(format_token_count(1_200), "1.2k");
        assert_eq!(format_token_count(42_000), "42k");
    }

    #[test]
    fn format_token_count_millions() {
        assert_eq!(format_token_count(1_200_000), "1.2M");
        assert_eq!(format_token_count(15_000_000), "15M");
    }

    #[test]
    fn format_cost_usd_zero() {
        assert_eq!(format_cost_usd(0), "$0.00");
    }

    #[test]
    fn format_cost_usd_cents() {
        assert_eq!(format_cost_usd(123), "$1.23");
        assert_eq!(format_cost_usd(1234), "$12.34");
        assert_eq!(format_cost_usd(5), "$0.05");
    }
}
