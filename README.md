# Mine storage regression evidence

Code tested: [`1d00c0b1ea17a4d1184753d937057a0c333069ed`](https://github.com/AkshatGada/thus-spoke-zakura/commit/1d00c0b1ea17a4d1184753d937057a0c333069ed), based on upstream `ef0cfb2`.

The application and lightwalletd images were built from that checkout. The node used `zakuracore/zakura:1.6.0`. The app image ID and observed job/transaction identities are in [live-report.json](live-report.json).

## Automated checks

Node 24.19.0. All 299 tests in 27 files passed. Lint, formatting, production build, book links and diff whitespace checks passed. Raw command output is in [checks](checks). The production build retains its existing bundle-size warning. Rust files were unchanged; Rust test and Clippy suites were not rerun.

## Live browser checks

[verify-live.cjs](verify-live.cjs) runs real Chrome against the local server, Zakura and lightwalletd. It injects browser storage faults; it does not claim that Chrome policy naturally produced these failures. For acceptance cases, it forwards the mining POST to the real server, introduces the storage fault after receiving the real response, then delivers that response unchanged to the app. No fake job or progress response is supplied.

- Getter, read, write and silent non-retention failures: the dialog opens, shows a storage error on submission and closes normally. Zero mining POSTs, no chain-height change and no uncaught page errors.
- Normal mining: one POST, one block mined, job completed and wallet synchronized.
- Cleanup failure after acceptance: one POST, two blocks mined, accepted job still shown with a separate cleanup warning. The original key remains retained.
- Storage getter denied after closing the dialog during submission: one POST, three blocks mined, completed job visible after reopening.
- Additional smoke check: 0.1 test ZEC sent from Account 1 to Account 2 and confirmed. Same-key replay returned the original activity and transaction, with no extra balance or height change. This does not establish every payment failure/recovery path.

The live run advanced from height 103 to 110. All eight checks passed. This is a local verification script, not a new CI job.

To repeat, use a **fresh, settled Regtest instance**, with Account 1 funded and Account 2 empty, and no other writers. Install Playwright in a separate QA environment and make a compatible Chrome/Chromium executable available, then run:

```sh
LIVE_BASE_URL=http://127.0.0.1:32805 CHROME_PATH=/path/to/chrome node verify-live.cjs
```

Output goes into `live-results` (override with `EVIDENCE_DIR`). The script sends a test payment and mines six additional blocks. Before retrying a failed run, inspect its retained payment key and backend state; do not turn an uncertain payment into a new one. Use a fresh instance for a complete repeat.

## Screenshots

- `before-fixture.jpg`: original failure in a local UI fixture with mocked mining status. This is not a live network screenshot.
- `storage-getter.png`: fixed dialog against the live stack with an injected getter denial.
- `mining-cleanup.png`: two actual blocks completed, with the injected storage cleanup warning shown separately.
- `mining-denied-after-unmount.png`: completed three-block job after storage became denied while the dialog was closed.

Only selected screenshots, receipts, the reproduction script and check logs are published here. This branch is hosted on the contributor fork and is separate from the five-file code diff.
