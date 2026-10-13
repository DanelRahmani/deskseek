pub mod agent;
pub mod assemble;
pub mod review;
pub mod summarize;

use crate::api::client::Client;
use crate::api::models;
use crate::api::types::Usage;
use crate::store::{db, keychain, settings::Settings};
use crate::AppState;

/// The client a turn runs against: the default endpoint, and the key it needs.
///
/// Here rather than in `api` because it is the only thing that needs both the
/// settings and the keychain, and `api` deliberately knows about neither — which
/// is what lets every wire shape be tested without a profile on disk.
pub fn client(state: &AppState, settings: &Settings) -> Result<Client, String> {
    let endpoint = settings
        .endpoint()
        .ok_or("no endpoint is configured")?
        .clone();
    let key = keychain::get_key(&endpoint.key_ref)?;
    Ok(Client::new(state.http.clone(), endpoint, key))
}

/// Writes down one overhead call: a summary or an in-place revision — work the
/// user did not type and did pay for.
///
/// Here, in the parent of the two modules that make such calls, because there is
/// exactly one of it and there must be: the same table, the same prices and the
/// same `priced` rule as a turn, since the one thing this app refuses is a
/// second, quieter story about money. Its own `run_id` makes it its own row
/// rather than a round of somebody's turn.
///
/// The price comes from the settings because a rate belongs to an endpoint and a
/// model together: the same id can cost different money in two places, and this
/// row has to carry the rate of the endpoint that was actually called. A model
/// with no published rate records its tokens and leaves the money unknown.
///
/// Deliberately not `async` and deliberately ignoring the error: a call that has
/// already been paid for must not be able to fail the thing the user asked for,
/// and there is nothing useful to do with a write failure here but carry on.
pub fn record_overhead(
    state: &AppState,
    settings: &Settings,
    chat_id: &str,
    kind: &str,
    model: &str,
    usage: &Usage,
) {
    let price = settings
        .model_info(model)
        .cost(usage, models::is_peak(chrono::Utc::now()));
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
