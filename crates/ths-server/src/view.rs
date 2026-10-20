//! Decrypts one transaction with one unified full viewing key, for the
//! Explorer's "View as" option. Nothing is stored: the result is only what
//! that key can see in the transaction's shielded outputs.

use std::collections::HashMap;

use anyhow::{Context, Result};
use serde::Serialize;
use zcash_client_backend::{DecryptedOutput, TransferType, decrypt_transaction};
use zcash_keys::{
    address::{Address, UnifiedAddress},
    keys::UnifiedFullViewingKey,
};
use zcash_primitives::transaction::Transaction;
use zcash_protocol::{
    ShieldedPool,
    consensus::{BlockHeight, BranchId},
    memo::{Memo, MemoBytes},
};

use crate::network::regtest_network;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ViewedOutput {
    /// `sapling`, `orchard`, or `ironwood`.
    pub(crate) pool: &'static str,
    /// The output's position within its pool's bundle.
    pub(crate) index: usize,
    /// `received` by the key's addresses, `change` back to the same account,
    /// or `sent` to someone else (recovered with the key's outgoing viewing key).
    pub(crate) transfer: &'static str,
    pub(crate) value_zatoshi: u64,
    pub(crate) recipient_address: Option<String>,
    pub(crate) memo_text: Option<String>,
    /// Hex of a memo that is not text, with trailing zero padding removed.
    pub(crate) memo_hex: Option<String>,
}

/// Decrypts `raw` with `ufvk`. A mined transaction is parsed for its mined
/// height; a mempool transaction for the height after `tip`.
pub(crate) fn view_transaction(
    raw: &[u8],
    mined_height: Option<u32>,
    tip: u32,
    ufvk: &UnifiedFullViewingKey,
) -> Result<Vec<ViewedOutput>> {
    let params = regtest_network();
    let mined_height = mined_height.map(BlockHeight::from_u32);
    let tip = BlockHeight::from_u32(tip);
    let height = mined_height.unwrap_or(tip + 1);
    let tx = Transaction::read(raw, BranchId::for_height(&params, height))
        .context("parsing the transaction")?;
    let keys = HashMap::from([((), ufvk.clone())]);
    let decrypted = decrypt_transaction(&params, mined_height, Some(tip), &tx, &keys);

    let mut outputs: Vec<ViewedOutput> = decrypted
        .sapling_outputs()
        .iter()
        .map(|output| {
            let recipient = Address::Sapling(output.note().recipient()).encode(&params);
            viewed(output, output.note_value().into_u64(), Some(recipient))
        })
        .collect();
    for output in decrypted
        .orchard_outputs()
        .iter()
        .chain(decrypted.ironwood_outputs())
    {
        let (note, _) = output.note();
        let recipient = UnifiedAddress::from_receivers(Some(note.recipient()), None, None)
            .map(|address| Address::Unified(address).encode(&params));
        outputs.push(viewed(output, note.value().inner(), recipient));
    }
    outputs.sort_by_key(|output| (pool_order(output.pool), output.index));
    Ok(outputs)
}

fn viewed<Note>(
    output: &DecryptedOutput<Note, ()>,
    value_zatoshi: u64,
    recipient_address: Option<String>,
) -> ViewedOutput {
    let (memo_text, memo_hex) = memo_fields(output.memo());
    ViewedOutput {
        pool: pool_name(output.value_pool()),
        index: output.index(),
        transfer: match output.transfer_type() {
            TransferType::Incoming | TransferType::WalletInternal => "received",
            TransferType::AccountInternal => "change",
            TransferType::Outgoing => "sent",
        },
        value_zatoshi,
        recipient_address,
        memo_text,
        memo_hex,
    }
}

fn pool_name(pool: ShieldedPool) -> &'static str {
    match pool {
        ShieldedPool::Sapling => "sapling",
        ShieldedPool::Orchard => "orchard",
        ShieldedPool::Ironwood => "ironwood",
    }
}

fn pool_order(pool: &str) -> u8 {
    match pool {
        "sapling" => 0,
        "orchard" => 1,
        _ => 2,
    }
}

fn memo_fields(memo: &MemoBytes) -> (Option<String>, Option<String>) {
    match Memo::try_from(memo.clone()) {
        Ok(Memo::Empty) => (None, None),
        Ok(Memo::Text(text)) => (Some(String::from(&*text)), None),
        Ok(Memo::Arbitrary(_) | Memo::Future(_)) | Err(_) => {
            let bytes = memo.as_slice();
            let end = bytes
                .iter()
                .rposition(|byte| *byte != 0)
                .map_or(0, |i| i + 1);
            (None, Some(hex::encode(&bytes[..end])))
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/view/tests.rs"]
mod tests;
