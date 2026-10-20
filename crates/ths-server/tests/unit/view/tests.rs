use super::*;
use crate::state::tests::state_with_local_wallet;

/// A Regtest payment of 0.1 ZEC from Account 1 to Account 2 with the memo
/// "order 42", mined at height 284. Every instance derives the same account
/// keys from the fixed development seed, so these keys can decrypt it.
const PAYMENT: &str = include_str!("account-1-to-2-memo.hex");
const PAYMENT_HEIGHT: u32 = 284;
const AMOUNT: u64 = 10_000_000;

fn account_key(id: u8) -> UnifiedFullViewingKey {
    let (state, _dir) = state_with_local_wallet();
    let encoded = state
        .0
        .store
        .accounts()
        .unwrap()
        .into_iter()
        .find(|account| account.id == id)
        .and_then(|account| account.unified_full_viewing_key)
        .expect("account viewing key");
    UnifiedFullViewingKey::decode(&regtest_network(), &encoded).unwrap()
}

fn view_as(id: u8) -> Vec<ViewedOutput> {
    let raw = hex::decode(PAYMENT.trim()).unwrap();
    view_transaction(&raw, Some(PAYMENT_HEIGHT), PAYMENT_HEIGHT, &account_key(id)).unwrap()
}

#[test]
fn the_recipient_sees_the_value_and_memo_it_received() {
    let outputs = view_as(2);
    assert_eq!(outputs.len(), 1, "{outputs:?}");
    let output = &outputs[0];
    assert_eq!(output.pool, "ironwood");
    assert_eq!(output.transfer, "received");
    assert_eq!(output.value_zatoshi, AMOUNT);
    assert_eq!(output.memo_text.as_deref(), Some("order 42"));
    assert_eq!(output.memo_hex, None);
    assert!(
        output
            .recipient_address
            .as_deref()
            .is_some_and(|address| address.starts_with("uregtest1")),
        "{output:?}"
    );
}

#[test]
fn the_sender_sees_what_it_sent_and_its_change() {
    let outputs = view_as(1);
    let sent: Vec<_> = outputs.iter().filter(|o| o.transfer == "sent").collect();
    let change: Vec<_> = outputs.iter().filter(|o| o.transfer == "change").collect();
    assert_eq!(sent.len(), 1, "{outputs:?}");
    assert_eq!(sent[0].value_zatoshi, AMOUNT);
    assert_eq!(sent[0].memo_text.as_deref(), Some("order 42"));
    assert_eq!(change.len(), 1, "{outputs:?}");
    assert!(change[0].value_zatoshi > 0);
    assert_eq!(
        sent[0].recipient_address,
        view_as(2)[0].recipient_address,
        "the sender recovers the address the recipient received on"
    );
}

#[test]
fn an_unrelated_key_sees_nothing() {
    assert_eq!(view_as(3), Vec::new());
}

#[test]
fn a_mempool_transaction_is_parsed_for_the_next_height() {
    let raw = hex::decode(PAYMENT.trim()).unwrap();
    let outputs = view_transaction(&raw, None, PAYMENT_HEIGHT - 1, &account_key(2)).unwrap();
    assert_eq!(outputs.len(), 1);
}

#[test]
fn bytes_that_are_not_a_transaction_are_rejected() {
    assert!(view_transaction(&[0, 1, 2], Some(1), 1, &account_key(1)).is_err());
}

#[test]
fn non_text_memos_are_returned_as_trimmed_hex() {
    let mut bytes = [0u8; 512];
    bytes[0] = 0xf6;
    let memo = MemoBytes::from_bytes(&bytes).unwrap();
    assert_eq!(
        memo_fields(&memo),
        (None, None),
        "0xF6 then zeros is no memo"
    );

    bytes[1] = 0x01;
    bytes[0] = 0xff;
    let memo = MemoBytes::from_bytes(&bytes).unwrap();
    assert_eq!(memo_fields(&memo), (None, Some("ff01".to_owned())));
}
