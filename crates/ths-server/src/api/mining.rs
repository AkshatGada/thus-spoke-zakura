use super::error::require_started;
use super::{
    error::{ApiError, ApiJson, ApiPath, ApiResult},
    payments::require_key,
};
use crate::blocks::BlockRequest;
use crate::mining::{MiningJob, MiningOptions, MiningState};
use crate::network::regtest_network;
use crate::state::AppState;
use anyhow::Context;
use axum::{Json, extract::State, http::StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use transparent::address::Script;
use zcash_keys::address::Address;

// Unknown fields are rejected so a misspelled `to` cannot silently pay the
// coinbase to the treasury instead.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StartMiningRequest {
    blocks: u32,
    idempotency_key: String,
    /// A development account (1 to 5) or transparent Regtest address that
    /// receives the coinbase instead of the treasury.
    #[serde(default)]
    to: Option<String>,
    #[serde(default)]
    empty: bool,
}

async fn resolve_options(
    state: &AppState,
    to: Option<String>,
    empty: bool,
) -> ApiResult<(MiningOptions, BlockRequest)> {
    let coinbase = match &to {
        None => None,
        Some(to) => Some(coinbase_script(state, to).await?),
    };
    Ok((
        MiningOptions { to, empty },
        BlockRequest { coinbase, empty },
    ))
}

/// The coinbase can only pay a transparent script: a shielded coinbase output
/// would need an Ironwood bundle that ths does not build.
async fn coinbase_script(state: &AppState, to: &str) -> ApiResult<Script> {
    let address = match to.parse::<u8>() {
        Ok(id) => state
            .accounts()
            .await
            .into_iter()
            .find(|account| account.id == id)
            .map(|account| account.transparent_address)
            .ok_or_else(|| {
                ApiError::bad_request("to must be account 1 to 5 or a transparent Regtest address")
            })?,
        Err(_) => to.to_owned(),
    };
    match Address::decode(&regtest_network(), &address) {
        Some(Address::Transparent(address)) => Ok(Script::from(address.script())),
        Some(_) => Err(ApiError::bad_request(
            "the coinbase can only pay a transparent address; send it to an account number to use that account's transparent address",
        )),
        None => Err(ApiError::bad_request(
            "to must be account 1 to 5 or a transparent Regtest address",
        )),
    }
}

pub(crate) async fn start_mining_job(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<StartMiningRequest>,
) -> ApiResult<(StatusCode, Json<MiningJob>)> {
    require_started(&state)?;
    require_key(&req.idempotency_key)?;
    let (options, request) = resolve_options(&state, req.to, req.empty).await?;
    let admission = state
        .start_mining(req.blocks, req.idempotency_key, options, request)
        .await?;
    Ok((StatusCode::ACCEPTED, Json(admission.job)))
}

pub(crate) async fn latest_mining_job(State(state): State<AppState>) -> Json<Value> {
    Json(json!({"job":state.0.mining.latest().await}))
}

pub(crate) async fn mining_job(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<String>,
) -> ApiResult<Json<MiningJob>> {
    state
        .0
        .mining
        .get(&id)
        .await
        .map(Json)
        .ok_or_else(|| ApiError {
            status: StatusCode::NOT_FOUND,
            message: "Mining job does not exist".into(),
        })
}

#[derive(Deserialize)]
pub(crate) struct MineRequest {
    blocks: u32,
}
pub(crate) async fn mine(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<MineRequest>,
) -> ApiResult<Json<Value>> {
    require_started(&state)?;
    if !(1..=10_000).contains(&req.blocks) {
        return Err(ApiError::bad_request("blocks must be between 1 and 10,000"));
    }
    let mut admission = state
        .start_mining(
            req.blocks,
            uuid::Uuid::new_v4().to_string(),
            MiningOptions::default(),
            BlockRequest::default(),
        )
        .await?;
    loop {
        let update = admission.updates.borrow_and_update().clone();
        match update.job.state {
            MiningState::Completed => {
                let hashes = update
                    .hashes
                    .context("Completed mining job omitted its hashes")?;
                return Ok(Json(json!({"blocks":hashes.len(),"hashes":*hashes})));
            }
            MiningState::Failed => {
                return Err(anyhow::anyhow!(
                    update
                        .job
                        .error
                        .context("Failed mining job omitted its error")?
                )
                .into());
            }
            MiningState::Mining | MiningState::Syncing => {}
        }
        admission
            .updates
            .changed()
            .await
            .context("Mining job update channel closed before completion")?;
    }
}

#[cfg(test)]
#[path = "../../tests/unit/api/mining/tests.rs"]
pub(crate) mod tests;
