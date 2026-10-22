use super::*;
use serde_json::Value;
use zcash_primitives::block::Block;

// Templates returned by Zakura 1.6.0 on a ths Regtest chain: one with no
// mempool transactions (height 106), one with two (height 108).
pub(crate) const EMPTY_TEMPLATE: &str =
    include_str!("../../support/blocks/template-empty-mempool.json");
pub(crate) const TWO_TX_TEMPLATE: &str =
    include_str!("../../support/blocks/template-two-transactions.json");

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

fn template_coinbase(json: &str) -> Transaction {
    let template = template(json);
    let raw = hex::decode(&template.coinbasetxn.data).unwrap();
    let height = BlockHeight::from_u32(template.height);
    Transaction::read(&raw[..], BranchId::for_height(&regtest_network(), height)).unwrap()
}

fn outputs(tx: &Transaction) -> Vec<(u64, Script)> {
    tx.transparent_bundle()
        .unwrap()
        .vout
        .iter()
        .map(|out| (out.value().into_u64(), out.script_pubkey().clone()))
        .collect()
}

/// The template's own miner script: the treasury that Zakura pays.
fn treasury(json: &str) -> Script {
    outputs(&template_coinbase(json))[0].1.clone()
}

fn account_script(address: &str) -> Script {
    let Some(Address::Transparent(address)) = Address::decode(&regtest_network(), address) else {
        panic!("expected a transparent address");
    };
    Script::from(address.script())
}

#[test]
fn an_unchanged_template_reproduces_zakuras_own_roots() {
    for json in [EMPTY_TEMPLATE, TWO_TX_TEMPLATE] {
        let template = template(json);
        let raw: Value = serde_json::from_str(json).unwrap();
        let (block, hash) = assemble(&template, &BlockRequest::default(), &treasury(json)).unwrap();
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
        let mut coinbase = Vec::new();
        block.vtx().first().write(&mut coinbase).unwrap();
        assert_eq!(hex::encode(coinbase), template.coinbasetxn.data);
    }
}

#[test]
fn the_header_meets_the_regtest_target_with_a_zero_solution() {
    let template = template(TWO_TX_TEMPLATE);
    for _ in 0..20 {
        let (block, hash) = assemble(
            &template,
            &BlockRequest::default(),
            &treasury(TWO_TX_TEMPLATE),
        )
        .unwrap();
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
    let (block, _) = assemble(&template, &request, &treasury(TWO_TX_TEMPLATE)).unwrap();
    let block = parse(&block);
    assert_eq!(block.vtx().len(), 1);
    let original = outputs(&template_coinbase(TWO_TX_TEMPLATE));
    let rebuilt = outputs(block.vtx().first());
    // Two 15,000-zatoshi fees: the miner's NU7 share is 30,000 - 18,000.
    assert_eq!(rebuilt[0].0, original[0].0 - 12_000);
    assert_eq!(rebuilt[0].1, original[0].1);
    assert_eq!(rebuilt[1..], original[1..]);
}

#[test]
fn the_coinbase_can_pay_another_transparent_script() {
    let template = template(TWO_TX_TEMPLATE);
    let script = account_script("tmDGByq79K3SJdCt3aGCTjYquv45Nf3ExaL");
    let request = BlockRequest {
        coinbase: Some(script.clone()),
        ..BlockRequest::default()
    };
    let (block, _) = assemble(&template, &request, &treasury(TWO_TX_TEMPLATE)).unwrap();
    let rebuilt = outputs(parse(&block).vtx().first());
    let original = outputs(&template_coinbase(TWO_TX_TEMPLATE));
    assert_eq!(rebuilt[0], (original[0].0, script));
    assert_eq!(rebuilt[1..], original[1..]);
}

#[test]
fn the_coinbase_is_only_rewritten_when_it_pays_the_treasury_first() {
    let template = template(TWO_TX_TEMPLATE);
    let other = account_script("tmDGByq79K3SJdCt3aGCTjYquv45Nf3ExaL");
    let empty = BlockRequest {
        empty: true,
        ..BlockRequest::default()
    };
    let error = assemble(&template, &empty, &other).unwrap_err();
    assert!(
        error.to_string().contains("does not pay the treasury"),
        "{error:#}"
    );
    // Without a rewrite the coinbase is left exactly as Zakura built it.
    assert!(assemble(&template, &BlockRequest::default(), &other).is_ok());
}

#[test]
fn a_template_with_an_unexpected_fee_claim_is_refused() {
    let mut raw: Value = serde_json::from_str(TWO_TX_TEMPLATE).unwrap();
    raw["coinbasetxn"]["fee"] = serde_json::json!(-30_000);
    let template: BlockTemplate = serde_json::from_value(raw).unwrap();
    let error = assemble(
        &template,
        &BlockRequest::default(),
        &treasury(TWO_TX_TEMPLATE),
    )
    .unwrap_err();
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
