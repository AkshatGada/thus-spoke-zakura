use super::*;
use crate::api::router;
use crate::state::AppState;
use crate::state::tests::state_with_local_wallet;
use axum::body::Body;
use axum::routing::post;
use axum::{
    Json, Router,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;
#[test]
fn copies_spent_output_address_and_value_onto_vin() {
    let mut tx = json!({
        "vin": [{"txid": "aa", "vout": 1}],
        "vout": []
    });
    let mut prev = std::collections::HashMap::new();
    prev.insert(
        "aa".into(),
        json!({
            "vout": [
                {"n": 0, "valueZat": 1},
                {
                    "n": 1,
                    "valueZat": 50_000_000,
                    "scriptPubKey": {"addresses": ["tmABC"]}
                }
            ]
        }),
    );
    attach_transparent_prevouts(&mut tx, &prev);
    assert_eq!(tx["vin"][0]["valueZat"], 50_000_000);
    assert_eq!(tx["vin"][0]["scriptPubKey"]["addresses"][0], "tmABC");
}

#[test]
fn leaves_coinbase_inputs_untouched() {
    let mut tx = json!({"vin": [{"coinbase": "00"}]});
    attach_transparent_prevouts(&mut tx, &Default::default());
    assert_eq!(tx["vin"][0]["coinbase"], "00");
    assert!(tx["vin"][0].get("valueZat").is_none());
}

#[test]
fn copies_the_ironwood_root_from_the_treestate() {
    let mut block = json!({"hash": "ab", "finalorchardroot": "orchard-root"});
    add_ironwood_root(
        &mut block,
        &json!({"ironwood": {"commitments": {"finalRoot": "ironwood-root", "finalState": "00"}}}),
    );
    assert_eq!(block["finalironwoodroot"], "ironwood-root");
    assert_eq!(block["finalorchardroot"], "orchard-root");

    // At the genesis block Zakura returns empty Ironwood commitments.
    let mut block = json!({"hash": "ab"});
    add_ironwood_root(&mut block, &json!({"ironwood": {"commitments": {}}}));
    assert!(block.get("finalironwoodroot").is_none());
}

#[tokio::test]
async fn explorer_rejects_malformed_addresses_before_reaching_zakura() {
    let (state, _dir) = state_with_local_wallet();
    let account = state.0.store.account(1).unwrap();
    assert!(require_transparent_address(&account.transparent_address).is_ok());

    let response = router(state)
        .oneshot(
            Request::get("/api/v1/addresses/tmOOOOOOOOOOOOOOOOOOOOOOOO")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn explorer_reports_unknown_blocks_and_transactions_as_404() {
    // A node that knows nothing, answering with Zakura's not-found codes.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let node = format!("http://{}", listener.local_addr().unwrap());
    let not_found = Router::new().route(
        "/",
        post(|Json(req): Json<Value>| async move {
            let code = if req["method"] == "getblock" { -8 } else { -5 };
            Json(json!({"error": {"code": code, "message": "not found"}}))
        }),
    );
    tokio::spawn(async move { axum::serve(listener, not_found).await });

    let (state, _dir) = state_with_local_wallet();
    let state = AppState::new(
        state.0.store.clone(),
        state.0.wallet.clone(),
        node,
        "test".into(),
    );
    let hash = "a".repeat(64);
    for path in [
        "/api/v1/blocks/999999".to_owned(),
        format!("/api/v1/transactions/{hash}"),
        "/api/v1/search?q=999999".to_owned(),
        format!("/api/v1/search?q={hash}"),
    ] {
        let response = router(state.clone())
            .oneshot(Request::get(&path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
    }
}

#[tokio::test]
async fn search_does_not_mask_a_block_node_failure_as_404() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let node = format!("http://{}", listener.local_addr().unwrap());
    let failing = Router::new().route(
        "/",
        post(|Json(req): Json<Value>| async move {
            let code = if req["method"] == "getblock" { -28 } else { -5 };
            Json(json!({"error": {"code": code, "message": "injected"}}))
        }),
    );
    tokio::spawn(async move { axum::serve(listener, failing).await });

    let (state, _dir) = state_with_local_wallet();
    let state = AppState::new(
        state.0.store.clone(),
        state.0.wallet.clone(),
        node,
        "t".into(),
    );
    let uri = format!("/api/v1/search?q={}", "a".repeat(64));
    let response = router(state)
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

const PAYMENT_TXID: &str = "0dab5355f30af51a61fe92bf53fe51fbc9449485b9ff0d0b5fd6146a7f47e056";
const PAYMENT_HEX: &str = include_str!("../../view/account-1-to-2-memo.hex");

/// A node that knows only the fixture payment, mined at height 284.
async fn node_with_payment() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let node = format!("http://{}", listener.local_addr().unwrap());
    let rpc = Router::new().route(
        "/",
        post(|Json(req): Json<Value>| async move {
            let result = match req["method"].as_str() {
                Some("getblockchaininfo") => json!({
                    "chain": "test",
                    "blocks": 290,
                    "bestblockhash": "b".repeat(64),
                }),
                Some("getrawtransaction") if req["params"][0] == PAYMENT_TXID => json!({
                    "txid": PAYMENT_TXID,
                    "hex": PAYMENT_HEX.trim(),
                    "height": 284,
                    "confirmations": 7,
                }),
                _ => return Json(json!({"error": {"code": -5, "message": "not found"}})),
            };
            Json(json!({"result": result, "error": null}))
        }),
    );
    tokio::spawn(async move { axum::serve(listener, rpc).await });
    node
}

async fn decrypt(state: AppState, txid: &str, body: Value) -> (StatusCode, Value) {
    let response = router(state)
        .oneshot(
            Request::post(format!("/api/v1/transactions/{txid}/decrypt"))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn state_with_payment_node() -> (AppState, tempfile::TempDir) {
    let (state, dir) = state_with_local_wallet();
    let node = node_with_payment().await;
    let state = AppState::new(
        state.0.store.clone(),
        state.0.wallet.clone(),
        node,
        "test".into(),
    );
    (state, dir)
}

#[tokio::test]
async fn decrypt_shows_a_payment_as_an_account_or_a_pasted_key_sees_it() {
    let (state, _dir) = state_with_payment_node().await;

    let (status, body) = decrypt(state.clone(), PAYMENT_TXID, json!({"account": 2})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["mined_height"], 284);
    assert_eq!(body["outputs"][0]["transfer"], "received");
    assert_eq!(body["outputs"][0]["value_zatoshi"], 10_000_000);
    assert_eq!(body["outputs"][0]["memo_text"], "order 42");

    let ufvk = state
        .0
        .store
        .accounts()
        .unwrap()
        .into_iter()
        .find(|account| account.id == 2)
        .and_then(|account| account.unified_full_viewing_key)
        .unwrap();
    let (status, pasted) = decrypt(state.clone(), PAYMENT_TXID, json!({"ufvk": ufvk})).await;
    assert_eq!(status, StatusCode::OK, "{pasted}");
    assert_eq!(pasted["outputs"], body["outputs"]);

    let (status, unrelated) = decrypt(state, PAYMENT_TXID, json!({"account": 3})).await;
    assert_eq!(status, StatusCode::OK, "{unrelated}");
    assert_eq!(unrelated["outputs"], json!([]));
}

#[tokio::test]
async fn decrypt_rejects_unclear_or_invalid_keys_before_reaching_zakura() {
    let (state, _dir) = state_with_local_wallet();
    for body in [
        json!({}),
        json!({"account": 2, "ufvk": "uviewregtest1"}),
        json!({"account": 6}),
        json!({"account": 0}),
        json!({"ufvk": "not a key"}),
        json!({"account": 2, "memo_text": "unknown field"}),
    ] {
        let (status, error) = decrypt(state.clone(), PAYMENT_TXID, body.clone()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body} -> {error}");
        assert!(error["error"]["message"].is_string(), "{error}");
    }
    let (_, error) = decrypt(state, PAYMENT_TXID, json!({"account": 2, "memo": "x"})).await;
    assert_eq!(error["error"]["message"], "memo is not a recognized field");
}

#[tokio::test]
async fn decrypt_reports_an_unknown_transaction_as_404() {
    let (state, _dir) = state_with_payment_node().await;
    for txid in ["a".repeat(64), "not-a-txid".to_owned()] {
        let (status, error) = decrypt(state.clone(), &txid, json!({"account": 1})).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{txid}: {error}");
    }
}
