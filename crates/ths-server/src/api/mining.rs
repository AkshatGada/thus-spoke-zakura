use super::error::require_started;
use super::{
    error::{ApiError, ApiJson, ApiPath, ApiResult},
    payments::require_key,
};
use crate::blocks::{BlockRequest, HeldTransaction};
use crate::mining::{MiningJob, MiningOptions, MiningState};
use crate::network::regtest_network;
use crate::state::AppState;
use anyhow::Context;
use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
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
#[serde(deny_unknown_fields)]
pub(crate) struct MineRequest {
    blocks: u32,
    #[serde(default)]
    to: Option<String>,
    #[serde(default)]
    empty: bool,
}
pub(crate) async fn mine(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<MineRequest>,
) -> ApiResult<Json<Value>> {
    require_started(&state)?;
    if !(1..=10_000).contains(&req.blocks) {
        return Err(ApiError::bad_request("blocks must be between 1 and 10,000"));
    }
    let (options, request) = resolve_options(&state, req.to, req.empty).await?;
    let mut admission = state
        .start_mining(
            req.blocks,
            uuid::Uuid::new_v4().to_string(),
            options,
            request,
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HoldRequest {
    txid: String,
}

/// A held transaction as the API reports it: whether Zakura still has it, and
/// the height after which it expires and Zakura drops it regardless.
#[derive(Serialize)]
pub(crate) struct HeldTransactionView {
    #[serde(flatten)]
    hold: HeldTransaction,
    in_mempool: bool,
    expiry_height: Option<u64>,
}

async fn held_view(
    state: &AppState,
    hold: HeldTransaction,
    mempool: &[String],
) -> HeldTransactionView {
    let in_mempool = mempool.contains(&hold.txid);
    // Expiry is informative, so an unknown transaction (already expired and
    // dropped) reports none instead of failing the whole listing.
    let expiry_height = match state.0.rpc.lookup_transaction(&hold.txid).await {
        Ok(Some(tx)) => tx.get("expiryheight").and_then(Value::as_u64),
        Ok(None) => None,
        Err(error) => {
            tracing::debug!(error = %format_args!("{error:#}"), txid = %hold.txid, "reading held transaction expiry");
            None
        }
    };
    HeldTransactionView {
        hold,
        in_mempool,
        expiry_height,
    }
}

pub(crate) async fn held_transactions(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    require_started(&state)?;
    let mempool = state.0.rpc.mempool().await?;
    let mut held = Vec::new();
    for hold in state.0.blocks.held().await {
        held.push(held_view(&state, hold, &mempool).await);
    }
    Ok(Json(json!({ "held": held })))
}

/// Keeps a transaction out of every block ths mines until released. It may be
/// held before it reaches the mempool: in `mempool` mining mode a block follows
/// within seconds of a broadcast, so holding first is the only reliable order.
pub(crate) async fn hold_transaction(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<HoldRequest>,
) -> ApiResult<Json<HeldTransactionView>> {
    require_started(&state)?;
    if req.txid.len() != 64
        || !req
            .txid
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ApiError::bad_request(
            "txid must be 64 lowercase hexadecimal characters",
        ));
    }
    let mempool = state.0.rpc.mempool().await?;
    if !mempool.contains(&req.txid)
        && let Some(tx) = state.0.rpc.lookup_transaction(&req.txid).await?
        && tx.get("height").and_then(Value::as_u64).is_some()
    {
        return Err(ApiError {
            status: StatusCode::CONFLICT,
            message: "transaction is already mined; a hold only applies to unmined transactions"
                .into(),
        });
    }
    let height = state.0.rpc.checkpoint().await?.height;
    let hold = state.0.blocks.hold(&req.txid, height).await;
    Ok(Json(held_view(&state, hold, &mempool).await))
}

pub(crate) async fn release_transaction(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<HoldRequest>,
) -> ApiResult<Json<HeldTransactionView>> {
    require_started(&state)?;
    let hold = state
        .0
        .blocks
        .release(&req.txid)
        .await
        .ok_or_else(|| ApiError {
            status: StatusCode::NOT_FOUND,
            message: "transaction is not held".into(),
        })?;
    let mempool = state.0.rpc.mempool().await?;
    Ok(Json(held_view(&state, hold, &mempool).await))
}

#[cfg(test)]
#[path = "../../tests/unit/api/mining/tests.rs"]
pub(crate) mod tests;
