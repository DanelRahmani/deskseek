//! Model registry and cost estimation.
//!
//! Prices are USD per 1M tokens. Off-peak is half of peak; peak hours are
//! 01:00-04:00 and 06:00-10:00 UTC, Monday-Friday (excluding Chinese holidays,
//! which we cannot know locally — treat them as peak, i.e. over-estimate).

use chrono::{DateTime, Datelike, Timelike, Utc};
use serde::Serialize;

use crate::api::types::Usage;

#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub context: u32,
    pub max_output: u32,
}

pub const MODELS: &[ModelInfo] = &[
    ModelInfo {
        id: "deepseek-flash",
        label: "DeepSeek Flash (V4.1)",
        context: 1_000_000,
        max_output: 393_216,
    },
    ModelInfo {
        id: "deepseek-v4-pro",
        label: "DeepSeek V4 Pro",
        context: 1_000_000,
        max_output: 393_216,
    },
];

/// One model's context window, in tokens.
///
/// An id we do not ship gets the largest window we know about, which is the same
/// fallback the frontend's meter makes. Guessing *small* is the dangerous
/// direction: it would fold a chat that had room left and spend a summariser call
/// nobody needed.
pub fn context_for(model: &str) -> u32 {
    MODELS
        .iter()
        .find(|m| m.id == model)
        .map(|m| m.context)
        .unwrap_or(1_000_000)
}

pub fn is_peak(now: DateTime<Utc>) -> bool {
    if matches!(now.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun) {
        return false;
    }
    let h = now.hour();
    (1..4).contains(&h) || (6..10).contains(&h)
}

/// (cache-hit input, cache-miss input, output) USD per 1M tokens, off-peak.
fn off_peak_rates(model: &str) -> Option<(f64, f64, f64)> {
    match model {
        "deepseek-flash" => Some((0.003, 0.15, 0.60)),
        "deepseek-v4-pro" => Some((0.022, 0.66, 1.98)),
        _ => None,
    }
}

pub fn price_per_million(model: &str, peak: bool) -> Option<(f64, f64, f64)> {
    let (hit, miss, out) = off_peak_rates(model)?;
    let m = if peak { 2.0 } else { 1.0 };
    Some((hit * m, miss * m, out * m))
}

/// Cost in USD for one response. Falls back to billing the whole prompt as a
/// cache miss when the API omits the cache breakdown.
pub fn cost_usd(model: &str, usage: &Usage, peak: bool) -> Option<f64> {
    let (hit_rate, miss_rate, out_rate) = price_per_million(model, peak)?;
    let miss_tokens = if usage.prompt_cache_hit_tokens + usage.prompt_cache_miss_tokens == 0 {
        usage.prompt_tokens
    } else {
        usage.prompt_cache_miss_tokens
    };
    Some(
        (usage.prompt_cache_hit_tokens as f64 / 1_000_000.0) * hit_rate
            + (miss_tokens as f64 / 1_000_000.0) * miss_rate
            + (usage.completion_tokens as f64 / 1_000_000.0) * out_rate,
    )
}

/// Whether a run has crossed the user's spend limit.
///
/// A limit of 0 means "no limit" rather than "stop at once", which is the
/// default and the only value nobody has to think about. Reaching the limit
/// exactly counts as crossing it: a limit you can sit on is not a limit.
pub fn over_budget(spent: f64, limit: f64) -> bool {
    limit > 0.0 && spent >= limit
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn peak_hours_are_weekday_windows_only() {
        // Tuesday 02:00 UTC -> peak
        let t = Utc.with_ymd_and_hms(2026, 10, 13, 2, 0, 0).unwrap();
        assert!(is_peak(t));
        // Tuesday 12:00 UTC -> off-peak
        let t = Utc.with_ymd_and_hms(2026, 10, 13, 12, 0, 0).unwrap();
        assert!(!is_peak(t));
        // Saturday 02:00 UTC -> off-peak
        let t = Utc.with_ymd_and_hms(2026, 10, 17, 2, 0, 0).unwrap();
        assert!(!is_peak(t));
    }

    #[test]
    fn one_million_miss_tokens_on_flash_off_peak_costs_fifteen_cents() {
        let usage = Usage {
            prompt_tokens: 1_000_000,
            prompt_cache_miss_tokens: 1_000_000,
            ..Default::default()
        };
        let c = cost_usd("deepseek-flash", &usage, false).unwrap();
        assert!((c - 0.15).abs() < 1e-9, "got {c}");
    }

    #[test]
    fn missing_cache_breakdown_is_billed_as_a_miss() {
        let usage = Usage {
            prompt_tokens: 1_000_000,
            ..Default::default()
        };
        let c = cost_usd("deepseek-flash", &usage, false).unwrap();
        assert!((c - 0.15).abs() < 1e-9, "got {c}");
    }

    #[test]
    fn a_zero_limit_is_no_limit() {
        assert!(!over_budget(5.0, 0.0));
        assert!(!over_budget(0.0, 1.0));
        assert!(over_budget(1.0, 1.0), "the limit itself counts as reached");
        assert!(over_budget(2.5, 1.0));
    }

    #[test]
    fn peak_doubles_the_price() {
        let usage = Usage {
            prompt_cache_miss_tokens: 1_000_000,
            ..Default::default()
        };
        let off = cost_usd("deepseek-flash", &usage, false).unwrap();
        let peak = cost_usd("deepseek-flash", &usage, true).unwrap();
        assert!((peak - off * 2.0).abs() < 1e-9);
    }
}
