//! Blocks that ths assembles itself from Zakura's block template.
//!
//! Zakura's `generate` mines every mempool transaction and pays the coinbase to
//! the configured miner address. To pay the coinbase elsewhere, mine an empty
//! block, or keep held transactions out, ths takes `getblocktemplate`, removes
//! transactions, rewrites the coinbase, recomputes the header commitments and
//! submits the block. Regtest does not verify Equihash, so the solution is the
//! all-zero placeholder that Zakura's own Regtest miner uses.

use std::collections::{BTreeMap, HashSet};

use anyhow::{Context, Result, bail};
use blake2b_simd::Params;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use transparent::{address::Script, bundle::TxOut};
use zcash_primitives::{
    block::{BlockHash, BlockHeaderData},
    transaction::Transaction,
};
use zcash_protocol::{
    consensus::{BlockHeight, BranchId, NetworkUpgrade, Parameters},
    value::Zatoshis,
};

use crate::{network::regtest_network, rpc::NodeRpc, state::AppState};

/// Regtest's placeholder Equihash solution: a 36-byte, all-zero field.
const REGTEST_SOLUTION_LEN: usize = 36;
/// Regtest's easy target is met by roughly one header hash in sixteen.
const MAX_NONCE_ATTEMPTS: usize = 4096;

/// What one block ths mines should contain beyond the node's defaults.
#[derive(Clone, Debug, Default)]
pub(crate) struct BlockRequest {
    /// Pay the coinbase's miner output to this script instead of the treasury.
    pub(crate) coinbase: Option<Script>,
    /// Leave every mempool transaction out of the block.
    pub(crate) empty: bool,
}

impl BlockRequest {
    fn is_default(&self) -> bool {
        self.coinbase.is_none() && !self.empty
    }
}

/// A transaction ths keeps out of every block it mines until it is released.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct HeldTransaction {
    pub(crate) txid: String,
    /// Chain height when the hold began.
    pub(crate) held_at: u64,
}

/// Holds last for the server lifetime. Zakura has no RPC to evict a mempool
/// transaction, so a hold only stops ths from mining it; the node still drops
/// it when it expires.
#[derive(Default)]
pub(crate) struct BlockProducer {
    held: Mutex<BTreeMap<String, HeldTransaction>>,
}

impl BlockProducer {
    pub(crate) async fn hold(&self, txid: &str, held_at: u64) -> HeldTransaction {
        self.held
            .lock()
            .await
            .entry(txid.to_owned())
            .or_insert_with(|| HeldTransaction {
                txid: txid.to_owned(),
                held_at,
            })
            .clone()
    }

    pub(crate) async fn release(&self, txid: &str) -> Option<HeldTransaction> {
        self.held.lock().await.remove(txid)
    }

    pub(crate) async fn held(&self) -> Vec<HeldTransaction> {
        self.held.lock().await.values().cloned().collect()
    }

    pub(crate) async fn held_txids(&self) -> HashSet<String> {
        self.held.lock().await.keys().cloned().collect()
    }
}

#[derive(Deserialize)]
struct BlockTemplate {
    version: i32,
    previousblockhash: String,
    curtime: u32,
    bits: String,
    target: String,
    height: u32,
    defaultroots: DefaultRoots,
    coinbasetxn: TemplateTransaction,
    transactions: Vec<TemplateTransaction>,
}

#[derive(Deserialize)]
struct DefaultRoots {
    chainhistoryroot: String,
}

#[derive(Deserialize)]
struct TemplateTransaction {
    data: String,
    hash: String,
    authdigest: String,
    fee: i64,
    /// One-based indexes of earlier template transactions this one spends.
    #[serde(default)]
    depends: Vec<usize>,
}

impl AppState {
    /// Mines `blocks` blocks. Without a request or held transactions this is
    /// Zakura's own `generate`, so default mining is unchanged; otherwise ths
    /// assembles each block itself so held transactions stay out.
    pub(crate) async fn generate_blocks(
        &self,
        blocks: u32,
        request: &BlockRequest,
    ) -> Result<Vec<String>> {
        let held = self.0.blocks.held_txids().await;
        if request.is_default() && held.is_empty() {
            return self.0.rpc.generate(blocks).await;
        }
        let mut hashes = Vec::with_capacity(blocks as usize);
        for _ in 0..blocks {
            hashes.push(mine_assembled_block(&self.0.rpc, request, &held).await?);
        }
        Ok(hashes)
    }
}

async fn mine_assembled_block(
    rpc: &NodeRpc,
    request: &BlockRequest,
    held: &HashSet<String>,
) -> Result<String> {
    let template: BlockTemplate = serde_json::from_value(rpc.block_template().await?)
        .context("decoding Zakura's block template")?;
    let (block, hash) = assemble(&template, request, held)?;
    if let Some(reason) = rpc.submit_block(&hex::encode(&block)).await? {
        bail!("Zakura rejected the assembled block: {reason}");
    }
    let tip = rpc.checkpoint().await?;
    anyhow::ensure!(
        tip.hash == hash,
        "the assembled block {hash} did not become the chain tip"
    );
    Ok(hash)
}

/// Builds a complete serialized block and returns it with its display hash.
fn assemble(
    template: &BlockTemplate,
    request: &BlockRequest,
    held: &HashSet<String>,
) -> Result<(Vec<u8>, String)> {
    let included = included_transactions(template, request, held);
    let height = BlockHeight::from_u32(template.height);
    let all_fees = template_fees(template.transactions.iter())?;
    let included_fees = template_fees(included.iter().copied())?;
    let template_share = miner_fee_share(height, all_fees);
    // Guard against a fee rule this code does not model: Zakura's template
    // coinbase must claim exactly the share computed here.
    anyhow::ensure!(
        template.coinbasetxn.fee == -i64::try_from(template_share)?,
        "Zakura's template claims a miner fee share of {} zatoshi, not the expected {template_share}",
        -template.coinbasetxn.fee
    );
    let excluded_share = template_share - miner_fee_share(height, included_fees);
    let coinbase = rewrite_coinbase(
        &template.coinbasetxn.data,
        height,
        excluded_share,
        request.coinbase.as_ref(),
    )?;

    let mut txids = vec![coinbase.txid];
    let mut auth_digests = vec![coinbase.auth_digest];
    for tx in &included {
        txids.push(internal_bytes(&tx.hash)?);
        auth_digests.push(internal_bytes(&tx.authdigest)?);
    }
    let merkle_root = merkle_root(&txids);
    let commitments = block_commitments(
        &internal_bytes(&template.defaultroots.chainhistoryroot)?,
        &auth_data_root(&auth_digests),
    );
    let bits = u32::from_str_radix(&template.bits, 16).context("invalid template bits")?;
    let target = hex::decode(&template.target).context("invalid template target")?;
    anyhow::ensure!(target.len() == 32, "template target must be 32 bytes");

    for _ in 0..MAX_NONCE_ATTEMPTS {
        let header = BlockHeaderData {
            version: template.version,
            prev_block: BlockHash(internal_bytes(&template.previousblockhash)?),
            merkle_root,
            final_sapling_root: commitments,
            time: template.curtime,
            bits,
            nonce: rand10::random(),
            solution: vec![0; REGTEST_SOLUTION_LEN],
        }
        .freeze()
        .context("serializing the block header")?;
        let mut display = header.hash().0;
        display.reverse();
        if display.as_slice() > target.as_slice() {
            continue;
        }
        let mut block = Vec::new();
        header.write(&mut block)?;
        write_compact_size(&mut block, 1 + included.len() as u64);
        block.extend(coinbase.bytes.iter());
        for tx in &included {
            block.extend(hex::decode(&tx.data).context("invalid template transaction")?);
        }
        return Ok((block, hex::encode(display)));
    }
    bail!("no nonce met the Regtest target in {MAX_NONCE_ATTEMPTS} attempts")
}

/// Template transactions to include: none for an empty block, otherwise all
/// except held ones and any that depend, directly or not, on an excluded one.
fn included_transactions<'a>(
    template: &'a BlockTemplate,
    request: &BlockRequest,
    held: &HashSet<String>,
) -> Vec<&'a TemplateTransaction> {
    if request.empty {
        return Vec::new();
    }
    let mut excluded: Vec<bool> = template
        .transactions
        .iter()
        .map(|tx| held.contains(&tx.hash))
        .collect();
    // `depends` only points at earlier transactions, so one forward pass
    // excludes every descendant.
    for index in 0..template.transactions.len() {
        if template.transactions[index]
            .depends
            .iter()
            .any(|parent| parent.checked_sub(1).is_some_and(|parent| excluded[parent]))
        {
            excluded[index] = true;
        }
    }
    template
        .transactions
        .iter()
        .zip(excluded)
        .filter(|(_, excluded)| !excluded)
        .map(|(tx, _)| tx)
        .collect()
}

fn template_fees<'a>(transactions: impl Iterator<Item = &'a TemplateTransaction>) -> Result<u64> {
    transactions
        .map(|tx| u64::try_from(tx.fee).context("template fee is negative"))
        .sum()
}

/// The miner's part of a block's transaction fees. From NU7 every block
/// contributes `floor(6 * fees / 10)` to the NSM; before NU7 the miner takes
/// all fees (Zakura `subsidy::miner_fee_share`).
fn miner_fee_share(height: BlockHeight, fees: u64) -> u64 {
    if regtest_network().is_nu_active(NetworkUpgrade::Nu7, height) {
        fees - fees * 6 / 10
    } else {
        fees
    }
}

struct Coinbase {
    bytes: Vec<u8>,
    txid: [u8; 32],
    auth_digest: [u8; 32],
}

/// Lowers the miner output by the fee share of excluded transactions so the
/// coinbase still balances exactly, and optionally pays it to another script.
fn rewrite_coinbase(
    data: &str,
    height: BlockHeight,
    excluded_share: u64,
    destination: Option<&Script>,
) -> Result<Coinbase> {
    let raw = hex::decode(data).context("invalid template coinbase")?;
    let branch = BranchId::for_height(&regtest_network(), height);
    let tx = Transaction::read(&raw[..], branch).context("parsing the template coinbase")?;
    let mut rewritten = Ok(());
    let data = tx.into_data().map_bundles(
        |bundle| {
            bundle.map(|mut bundle| {
                let Some(miner) = bundle.vout.first() else {
                    rewritten = Err(anyhow::anyhow!("the template coinbase has no miner output"));
                    return bundle;
                };
                match Zatoshis::from_u64(excluded_share)
                    .ok()
                    .and_then(|share| miner.value() - share)
                    .context("the excluded fees exceed the coinbase miner output")
                {
                    Ok(value) => {
                        let script = destination
                            .cloned()
                            .unwrap_or_else(|| miner.script_pubkey().clone());
                        bundle.vout[0] = TxOut::new(value, script);
                    }
                    Err(error) => rewritten = Err(error),
                }
                bundle
            })
        },
        |sapling| sapling,
        |orchard| orchard,
    );
    rewritten?;
    let tx = data.freeze().context("rebuilding the coinbase")?;
    let mut bytes = Vec::new();
    tx.write(&mut bytes)?;
    Ok(Coinbase {
        bytes,
        txid: *tx.txid().as_ref(),
        auth_digest: tx
            .auth_commitment()
            .as_bytes()
            .try_into()
            .context("coinbase auth digest is not 32 bytes")?,
    })
}

/// RPC hashes are displayed byte-reversed; consensus hashing uses the stored order.
fn internal_bytes(display: &str) -> Result<[u8; 32]> {
    let mut bytes: [u8; 32] = hex::decode(display)
        .context("invalid hash hex")?
        .try_into()
        .map_err(|_| anyhow::anyhow!("hash must be 32 bytes"))?;
    bytes.reverse();
    Ok(bytes)
}

fn double_sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(Sha256::digest(data)).into()
}

/// Bitcoin-style transaction Merkle root: an odd layer repeats its last node.
fn merkle_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    let mut layer = leaves.to_vec();
    while layer.len() > 1 {
        if layer.len() % 2 == 1 {
            layer.push(*layer.last().expect("nonempty layer"));
        }
        layer = layer
            .chunks(2)
            .map(|pair| double_sha256(&[pair[0], pair[1]].concat()))
            .collect();
    }
    layer[0]
}

/// ZIP 244 authorizing data root: leaves padded with zeros to a power of two.
fn auth_data_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    let mut layer = leaves.to_vec();
    layer.resize(leaves.len().next_power_of_two(), [0; 32]);
    while layer.len() > 1 {
        layer = layer
            .chunks(2)
            .map(|pair| blake2b_32(b"ZcashAuthDatHash", &[pair[0], pair[1]].concat()))
            .collect();
    }
    layer[0]
}

/// ZIP 244 block commitments hash over the chain history and auth data roots.
fn block_commitments(chain_history_root: &[u8; 32], auth_data_root: &[u8; 32]) -> [u8; 32] {
    blake2b_32(
        b"ZcashBlockCommit",
        &[&chain_history_root[..], &auth_data_root[..], &[0; 32]].concat(),
    )
}

fn blake2b_32(personal: &[u8; 16], data: &[u8]) -> [u8; 32] {
    Params::new()
        .hash_length(32)
        .personal(personal)
        .hash(data)
        .as_bytes()
        .try_into()
        .expect("a 32-byte BLAKE2b digest")
}

fn write_compact_size(out: &mut Vec<u8>, n: u64) {
    match n {
        0..0xfd => out.push(n as u8),
        0xfd..=0xffff => {
            out.push(0xfd);
            out.extend((n as u16).to_le_bytes());
        }
        0x1_0000..=0xffff_ffff => {
            out.push(0xfe);
            out.extend((n as u32).to_le_bytes());
        }
        _ => {
            out.push(0xff);
            out.extend(n.to_le_bytes());
        }
    }
}

impl NodeRpc {
    pub(crate) async fn block_template(&self) -> Result<Value> {
        self.call("getblocktemplate", serde_json::json!([])).await
    }

    /// `None` when Zakura accepted the block, otherwise its rejection reason.
    pub(crate) async fn submit_block(&self, block_hex: &str) -> Result<Option<String>> {
        let result: Value = self
            .call("submitblock", serde_json::json!([block_hex]))
            .await?;
        Ok(match result {
            Value::Null => None,
            Value::String(reason) => Some(reason),
            other => Some(other.to_string()),
        })
    }
}

#[cfg(test)]
#[path = "../tests/unit/blocks/tests.rs"]
mod tests;
