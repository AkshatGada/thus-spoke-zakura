use super::*;
use zcash_keys::address::Address;
use zcash_primitives::block::Block;

// Templates returned by Zakura 1.6.0 on a ths Regtest chain: one with no
// mempool transactions (height 106), one with two (height 108).
const EMPTY_TEMPLATE: &str = include_str!("template-2.json");
const TWO_TX_TEMPLATE: &str = include_str!("template-4.json");

fn template(json: &str) -> BlockTemplate {
    serde_json::from_str(json).unwrap()
}

fn parse(block: &[u8]) -> Block {
    Block::read(block, &regtest_network()).unwrap()
}

fn display(bytes: [u8; 32]) -> String {
    let mut bytes = bytes;
    bytes.reverse();
    hex::encode(bytes)
}

fn coinbase_outputs(block: &Block) -> Vec<(u64, Vec<u8>)> {
    block
        .vtx()
        .first()
        .transparent_bundle()
        .unwrap()
        .vout
        .iter()
        .map(|out| (out.value().into_u64(), out.script_pubkey().0.0.clone()))
        .collect()
}

fn template_coinbase_outputs(json: &str) -> Vec<(u64, Vec<u8>)> {
    let template = template(json);
    let raw = hex::decode(&template.coinbasetxn.data).unwrap();
    let height = BlockHeight::from_u32(template.height);
    let tx = Transaction::read(&raw[..], BranchId::for_height(&regtest_network(), height)).unwrap();
    tx.transparent_bundle()
        .unwrap()
        .vout
        .iter()
        .map(|out| (out.value().into_u64(), out.script_pubkey().0.0.clone()))
        .collect()
}

#[test]
fn an_unchanged_template_reproduces_zakuras_own_roots() {
    for json in [EMPTY_TEMPLATE, TWO_TX_TEMPLATE] {
        let template = template(json);
        let raw: Value = serde_json::from_str(json).unwrap();
        let (block, hash) = assemble(&template, &BlockRequest::default(), &HashSet::new()).unwrap();
        let block = parse(&block);
        assert_eq!(block.vtx().len(), 1 + template.transactions.len());
        assert_eq!(
            display(block.header().merkle_root),
            raw["defaultroots"]["merkleroot"].as_str().unwrap()
        );
        assert_eq!(
            display(block.header().final_sapling_root),
            raw["defaultroots"]["blockcommitmentshash"]
                .as_str()
                .unwrap()
        );
        assert_eq!(block.header().hash().to_string(), hash);
        assert_eq!(
            hex::encode({
                let mut tx = Vec::new();
                block.vtx().first().write(&mut tx).unwrap();
                tx
            }),
            template.coinbasetxn.data
        );
    }
}

#[test]
fn the_header_meets_the_regtest_target_with_a_zero_solution() {
    let template = template(TWO_TX_TEMPLATE);
    for _ in 0..20 {
        let (block, hash) = assemble(&template, &BlockRequest::default(), &HashSet::new()).unwrap();
        assert!(hash.as_str() <= template.target.as_str());
        assert_eq!(
            parse(&block).header().solution,
            vec![0; REGTEST_SOLUTION_LEN]
        );
    }
}

#[test]
fn an_empty_block_returns_the_miner_share_of_every_fee() {
    let template = template(TWO_TX_TEMPLATE);
    let request = BlockRequest {
        empty: true,
        ..BlockRequest::default()
    };
    let (block, _) = assemble(&template, &request, &HashSet::new()).unwrap();
    let block = parse(&block);
    assert_eq!(block.vtx().len(), 1);
    let original = template_coinbase_outputs(TWO_TX_TEMPLATE);
    let rebuilt = coinbase_outputs(&block);
    // Two 15,000-zatoshi fees: the miner's NU7 share is 30,000 - 18,000.
    assert_eq!(rebuilt[0].0, original[0].0 - 12_000);
    assert_eq!(rebuilt[0].1, original[0].1);
    assert_eq!(rebuilt[1..], original[1..]);
}

#[test]
fn a_held_transaction_is_left_out_and_only_its_fee_share_is_removed() {
    let template = template(TWO_TX_TEMPLATE);
    let held: HashSet<_> = [template.transactions[0].hash.clone()].into();
    let (block, _) = assemble(&template, &BlockRequest::default(), &held).unwrap();
    let block = parse(&block);
    let txids: Vec<_> = block
        .vtx()
        .iter()
        .skip(1)
        .map(|tx| tx.txid().to_string())
        .collect();
    assert_eq!(txids, [template.transactions[1].hash.clone()]);
    let original = template_coinbase_outputs(TWO_TX_TEMPLATE);
    // 30,000 in fees claims 12,000; the remaining 15,000 claims 6,000.
    assert_eq!(coinbase_outputs(&block)[0].0, original[0].0 - 6_000);
}

#[test]
fn transactions_depending_on_an_excluded_one_are_excluded_too() {
    let mut raw: Value = serde_json::from_str(TWO_TX_TEMPLATE).unwrap();
    raw["transactions"][1]["depends"] = serde_json::json!([1]);
    let template: BlockTemplate = serde_json::from_value(raw).unwrap();
    let held: HashSet<_> = [template.transactions[0].hash.clone()].into();
    assert!(included_transactions(&template, &BlockRequest::default(), &held).is_empty());
    assert_eq!(
        included_transactions(&template, &BlockRequest::default(), &HashSet::new()).len(),
        2
    );
}

#[test]
fn the_coinbase_can_pay_another_transparent_script() {
    let template = template(TWO_TX_TEMPLATE);
    let address =
        Address::decode(&regtest_network(), "tmDGByq79K3SJdCt3aGCTjYquv45Nf3ExaL").unwrap();
    let Address::Transparent(address) = address else {
        panic!("expected a transparent address");
    };
    let script = Script::from(address.script());
    let request = BlockRequest {
        coinbase: Some(script.clone()),
        ..BlockRequest::default()
    };
    let (block, _) = assemble(&template, &request, &HashSet::new()).unwrap();
    let outputs = coinbase_outputs(&parse(&block));
    let original = template_coinbase_outputs(TWO_TX_TEMPLATE);
    assert_eq!(outputs[0], (original[0].0, script.0.0));
    assert_eq!(outputs[1..], original[1..]);
}

#[test]
fn a_template_with_an_unexpected_fee_claim_is_refused() {
    let mut raw: Value = serde_json::from_str(TWO_TX_TEMPLATE).unwrap();
    raw["coinbasetxn"]["fee"] = serde_json::json!(-30_000);
    let template: BlockTemplate = serde_json::from_value(raw).unwrap();
    let error = assemble(&template, &BlockRequest::default(), &HashSet::new()).unwrap_err();
    assert!(error.to_string().contains("miner fee share"), "{error:#}");
}

#[test]
fn the_miner_fee_share_rounds_the_nsm_contribution_down() {
    let height = BlockHeight::from_u32(108);
    assert_eq!(miner_fee_share(height, 0), 0);
    assert_eq!(miner_fee_share(height, 15_000), 6_000);
    assert_eq!(miner_fee_share(height, 15_001), 6_001);
    assert_eq!(miner_fee_share(height, 9), 4);
}

#[test]
fn merkle_and_auth_roots_follow_their_padding_rules() {
    let a = [1; 32];
    let b = [2; 32];
    let c = [3; 32];
    assert_eq!(merkle_root(&[a]), a);
    // An odd Merkle layer repeats its last node.
    assert_eq!(merkle_root(&[a, b, c]), merkle_root(&[a, b, c, c]));
    // The auth data tree pads with zero leaves instead.
    assert_eq!(
        auth_data_root(&[a, b, c]),
        auth_data_root(&[a, b, c, [0; 32]])
    );
    assert_ne!(auth_data_root(&[a, b, c]), auth_data_root(&[a, b, c, c]));
}

#[test]
fn compact_sizes_use_the_shortest_encoding() {
    for (n, expected) in [
        (0, vec![0]),
        (0xfc, vec![0xfc]),
        (0xfd, vec![0xfd, 0xfd, 0]),
        (0x1_0000, vec![0xfe, 0, 0, 1, 0]),
    ] {
        let mut out = Vec::new();
        write_compact_size(&mut out, n);
        assert_eq!(out, expected);
    }
}

#[tokio::test]
async fn holds_are_idempotent_and_release_returns_the_hold() {
    let producer = BlockProducer::default();
    let first = producer.hold("tx", 10).await;
    let again = producer.hold("tx", 12).await;
    assert_eq!(first.held_at, 10);
    assert_eq!(again.held_at, 10);
    assert_eq!(producer.held().await.len(), 1);
    assert!(producer.held_txids().await.contains("tx"));
    assert_eq!(producer.release("tx").await.unwrap().held_at, 10);
    assert!(producer.release("tx").await.is_none());
    assert!(producer.held().await.is_empty());
}
