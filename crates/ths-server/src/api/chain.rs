use super::error::{ApiJson, ApiResult, require_started};
use crate::{
    chain::{ChainChangeError, ChainChangeResult, MAX_REORG_DEPTH, ReorgPlan},
    state::AppState,
};
use anyhow::Context;
use axum::{Json, extract::State};
use serde::Deserialize;
use std::collections::BTreeMap;

// Unknown fields are rejected so a misspelled `keep_txs`, `drop` or `place` cannot
// silently run a different reorg than the one requested.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReorgRequest {
    depth: u32,
    #[serde(default)]
    keep_txs: bool,
    #[serde(default)]
    drop: Vec<String>,
    #[serde(default)]
    place: BTreeMap<String, u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RollbackRequest {
    depth: u32,
}

pub(crate) async fn reorg(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<ReorgRequest>,
) -> ApiResult<Json<ChainChangeResult>> {
    let plan = ReorgPlan {
        keep_txs: req.keep_txs,
        drop: req.drop,
        place: req.place,
        ..ReorgPlan::default()
    };
    change_chain(state, req.depth, true, plan).await
}

pub(crate) async fn rollback(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<RollbackRequest>,
) -> ApiResult<Json<ChainChangeResult>> {
    change_chain(state, req.depth, false, ReorgPlan::default()).await
}

async fn change_chain(
    state: AppState,
    depth: u32,
    replace: bool,
    plan: ReorgPlan,
) -> ApiResult<Json<ChainChangeResult>> {
    require_started(&state)?;
    if !(1..=MAX_REORG_DEPTH).contains(&depth) {
        return Err(anyhow::Error::from(ChainChangeError::InvalidDepth).into());
    }
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let worker = tokio::spawn(async move { state.change_chain(depth, replace, plan).await });
    // The supervisor owns the worker even after the HTTP caller disconnects.
    tokio::spawn(async move {
        let result = worker
            .await
            .context("chain operation worker stopped")
            .and_then(|result| result);
        if let Err(error) = &result {
            tracing::warn!(error = %format_args!("{error:#}"), "chain operation failed");
        }
        if sender.send(result).is_err() {
            tracing::debug!("chain operation requester disconnected");
        }
    });
    Ok(Json(
        receiver
            .await
            .context("chain operation supervisor stopped")??,
    ))
}
