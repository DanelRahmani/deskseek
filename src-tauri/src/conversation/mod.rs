pub mod agent;
pub mod assemble;
pub mod review;
pub mod summarize;

use crate::api::models;
use crate::api::types::Usage;
use crate::store::db;
use crate::AppState;

/// Writes down one overhead call: a summary or an in-place revision — work the
/// user did not type and did pay for.
///
/// Here, in the parent of the two modules that make such calls, because there is
/// exactly one of it and there must be: the same table, the same prices and the
/// same `priced` rule as a turn, since the one thing this app refuses is a
/// second, quieter story about money. Its own `run_id` makes it its own row
/// rather than a round of somebody's turn.
///
/// Deliberately not `async` and deliberately ignoring the error: a call that has
/// already been paid for must not be able to fail the thing the user asked for,
/// and there is nothing useful to do with a write failure here but carry on.
pub fn record_overhead(state: &AppState, chat_id: &str, kind: &str, model: &str, usage: &Usage) {
    let price = models::cost_usd(model, usage, models::is_peak(chrono::Utc::now()));
    let conn = state.db.lock().unwrap();
    let _ = db::record_turn_usage(
        &conn,
        chat_id,
        &format!("{kind}:{}", uuid::Uuid::new_v4()),
        &db::TurnUsage {
            model,
            kind,
            priced: price.is_some(),
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            cache_hit_tokens: usage.prompt_cache_hit_tokens,
            cache_miss_tokens: usage.prompt_cache_miss_tokens,
            cost_usd: price.unwrap_or(0.0),
        },
        None,
    );
}
