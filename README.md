# External transparent-spend sync: local Regtest evidence

Fix commit: `3e0194bdc810f37b5a50600b3c5b15027234e889`.
Base: `9e89a4dd16361353bdfb325e9309a0359bb505ee`.
Audited tree: `c84803400e995461bca2da56797aa249cd7c5a64`.

## Identical-test before/after

`before-regression.log` is the new checked-in live test against unchanged base production code. Only the two identical test files were copied into that baseline checkout. It fails at `ready wallet retained externally spent transparent output: 10000000` after the node confirms the original outpoint was spent.

`after-regression.log` runs the exact same test against freshly compiled patched source in a task-owned target. It passes: Account 2 spends its 0.1 transparent ZEC through a separate client to send 0.0999 ZEC, with 0.0001 ZEC fee and no wallet-owned shielded change. Account 4's unrelated 0.2 ZEC stays untouched. The test also passes an extra block, restart, same-height reorg restoring funds, and a different canonical replacement spender. See PROVENANCE.json for commit, test-file hashes and binary identity.

The Cargo-built native server processes used real cached Zakura 1.6.0 and lightwalletd containers. This was not a fresh dashboard app-image build. Source tests create UUID-owned fixtures with loopback ports and cleanup. Build base and patched versions in **different Cargo target directories** to avoid artifact reuse.

```sh
cargo test --locked -p ths-server --test activity_recovery external_transparent_spend_reconciles_after_restart -- --ignored --exact --nocapture
```

The fixture expects cached `zakuracore/zakura:1.6.0` and `ths-recovery-lightwalletd:local`. Prepare the latter from the repository's pinned docker/lightwalletd.Dockerfile; local validation tagged the already-cached image rather than downloading a new one. The test uses separate development-wallet files and does not establish a particular third-party wallet's import flow.

## Retained-wallet upgrade proof

The `upgrade/` records are a separate real experiment. An original wallet retained 0.1 transparent ZEC after another client sent 0.05 ZEC, received 0.04985 ZEC Ironwood change, and paid 0.00015 ZEC in fees. `before.json` shows the wrong balance at a ready canonical checkpoint and absence of the new history table. `after.json` records the spending input, canonical block membership, spent-outpoint UTXO result and corrected balances after replacing the binary, at the **same height/hash 107 without mining**. Further block and restart records remain correct at 108.

The two PNG panels render these saved records; they are **not live browser or terminal screenshots**. This upgrade payment differs from the exact-max transparent-only payment in the checked-in regression. The renderer is included. Actual keys, startup/server logs and wallet databases are excluded. Regtest transactions and transparent addresses are public development data.

## Tests, audit and corrections

- Fifteen focused SDK/SQLite and reconciliation regressions passed independently (`audit-tests.log`). Astra medium approved the final tree with no unresolved actionable findings; it inspected/independently asserted the live records rather than rerunning Docker. See AUDIT.md for scope and limits.
- Workspace tests: 575 passed, 25 ignored. Release-distribution tests: 279 passed, 4 ignored. The selected live test was run explicitly, separately from ordinary workspace tests.
- Workspace formatting/Clippy with warnings denied, book links and whitespace passed. Frontend lint/format/build and all 345 web tests also passed before publication, using Node 24 and the unchanged lockfile. Logs are under checks/.
- A first implementation's reorg retry error was corrected; final live reorg/replacement and retry-classification tests passed. An attempted green run incorrectly reused the baseline executable through a shared target. It is excluded; build-cache-correction.json and AUDIT.md document that correction. The accepted green explicitly recompiles into an isolated target and pins the resulting binary.
- Partial import chunks can persist while coverage stays behind; retries are idempotent. No dense-history performance benchmark or full-publication midstream fault/process-kill experiment is claimed.
- All seventeen pre-existing containers were preserved, no owned test resources remain, and storage stayed above the 40 GB floor. cleanup.json records the final inventory.

Only absolute machine paths in copied text are replaced with placeholders. Transaction values, identities and outcomes are unchanged. SHA256.json hashes the actual published artifact files; per-command private-run log hashes in checks.json refer to the original pre-path-redaction logs.
