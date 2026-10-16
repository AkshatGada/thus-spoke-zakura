# Local evidence for issue #354

Source commit: `5dc76c8ead9f3ae78dae86c61a2be9311c8c8b21`.
Base: `9e89a4dd16361353bdfb325e9309a0359bb505ee`.

This branch holds review evidence separately from the implementation branch. Screenshots are from the rebuilt app against a fresh local Zakura Regtest node and lightwalletd. All results below were captured locally; they are not upstream CI results.

## Automated checks

Node 24.19.0: lint, formatting, all **357 tests in 34 files**, production build and book-link checks passed. The build retains the existing chunk-size warning. `checks/` contains output. `checks/before-tests.log` records three behavioral failures against the original production code before implementation. Local home/checkout paths were replaced with placeholders; ANSI color codes were removed.

The focused tests cover pending links, loading/empty/error separation, polling without a chain event, stopping after unmount, stale-result suppression, Retry, chain invalidation, API schema validation, and explicit mining confirmation.

An independent GPT-6 Astra medium agent audit found no actionable defects and independently reran all **25 focused tests across four files**. It inspected the saved Regtest/browser evidence and did not repeat the live run. See `AUDIT.md` for a concise record.

## Real Regtest flow

1. At height 105 the dashboard started with an empty pending list.
2. Retained a payment key and sent 0.1 ZEC from account 1 to account 2 through the app. A test-only loopback RPC proxy blocked automatic mining after startup, so this real signed and broadcast transaction remained pending.
3. The transaction appeared in the already-open Explorer without navigation or a height change. Its link opened detail with 0 confirmations and Pending.
4. Mine 1 block opened the existing dialog with one block selected. Explicit confirmation completed one job and advanced the canonical chain to height 106. The original transaction disappeared from the pending list and showed one confirmation.
5. Node lookup and canonical block lookup matched the same transaction/block. Account 2 received 10,000,000 zatoshi. Account 1 ended at 489,990,000 zatoshi, including the 10,000-zatoshi fee. Same-key replay left balances and height unchanged.
6. Injected a mempool RPC read failure: the real API returned 500, and the panel showed an error with Retry instead of an empty result. Removing the fault and using Retry recovered without changing chain, balances or mining job.
7. Checked the panel at a 390 px viewport. Fixture cleanup completed with no failures; 17 unrelated containers were preserved.

The transaction was broadcast by the existing app Send worker, not a separate external wallet. The controlled mining/read failures belong only to the verification fixture. No production mining-mode change is part of this PR.

Transaction: `5e669b7383f92aade8120cf576ac0bf8cdd77bb2fb95e211386d717d15739d2f`.
Canonical block 106: `3bc05eb57904c0e3b5995c718ff0b3142dcbfdc98d1c36d67736202f6767bffd`.

`regtest/` contains selected receipts and browser observations. `pending-summary.json` omits raw transaction bytes/proofs for readability. Wallet seeds, keys, private configuration and initialization output are excluded. `SHA256SUMS` identifies this evidence snapshot.

## Screenshots

- `pending-desktop.jpg`: transaction visible at height 105.
- `pending-transaction-detail.jpg`: same transaction with 0 confirmations.
- `mine-one-block-dialog.jpg` and `mining-completed.jpg`: explicit one-block action.
- `confirmed-empty-desktop.jpg` and `confirmed-transaction-detail.jpg`: empty panel at height 106 and confirmed detail.
- `mempool-error.jpg` and `mempool-recovered.jpg`: failed read and recovered empty list.
- `empty-mobile-final.jpg`: empty panel at 390 px viewport before the payment.
