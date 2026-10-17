# CLI payment retry: local Regtest evidence

Fix commit: `95a248bf609238e80ae066ab5e42dc520407041d`.
Base: `9e89a4dd16361353bdfb325e9309a0359bb505ee`.
Approved tree: `4195dfea5776c11a54ebd7579e7957fcac1c80b5`.

The actual full CLI created fresh named Regtest instances using cached Zakura, lightwalletd and THS app images. The caller closed each CLI stdout pipe, observed BrokenPipe after confirmed payments, then retried identical arguments. No fake CLI, payment API, node or indexer was used in these live runs. The same runtime images were used before and after; this patch changes only the host CLI.

| Payment transaction identities after failed output plus retry | Before | After |
| --- | --- | --- |
| Send | 2 | 1 |
| External-address Faucet | 2 | 1 |
| Two-account Faucet batch | 4 | 2 |

Counts exclude the later deliberate-repeat control and unrelated initialization/treasury transactions. Fixed retries preserve the original keys and txids, wallet balances and canonical tip. A later deliberate invocation creates new confirmed payments as intended. Full receipts, recorded journal state, balances and heights are in `before/results.json` and `after/results.json`. Binary and runtime image identities are in each `artifact.json`. Absolute local paths in copied logs/artifacts are replaced with placeholders, and trailing blank lines in test logs are normalized; payment outcomes are unchanged.

The harness checks each receipt's actual Zakura transaction, positive confirmations, inclusion-block membership and the canonical block hash. Raw RPC responses were checked during execution but were not separately archived. External recipient wallet balance was not observed; transaction identity and canonical inclusion are the oracle for that case. All live output-failure scenarios use JSON; owner-level tests cover text and partial writes.

`before/send-closed-pipe.stderr` and its after counterpart contain the actual output error. The adjacent `send-retry.stdout` files contain the actual identical-command retry receipts. The PNG panels are renderings of these saved logs plus the original API receipt, not newly captured live terminal screenshots. Their generator is included.

## Regression and required checks

Six new regression tests cover JSON/text, immediate/partial writes, terminal failures, whole and unfinished batches, unrelated saved keys, deliberate repeats and resolved Max amounts. Final tests against the original client produced 40 passes and four expected missing-identity failures (`regression-before.log`).

- Final workspace tests: 566 passed, 24 ordinarily ignored, with two test threads (`workspace-tests.log`).
- Release-distribution checks: 285 passed, 4 ignored, with two test threads (`release-distribution-tests.log`).
- Formatting, workspace Clippy with warnings denied, book links and whitespace passed.
- The initial default-parallel workspace run failed the unchanged 30 ms lifecycle fixture `run_timeout_after_allocation_skips_child_and_cleans_its_instance`. It passed in isolation and the complete two-thread rerun. `workspace-initial-parallel.log`, `lifecycle-isolated.log` and `checks.json` retain that history. No lifecycle code was changed.

An independent audit approved the exact final tree with no unresolved findings and separately reran all 44 CLI client tests successfully (`audit-tests.log`). The review identified a Max test writer/error-kind mismatch; it was corrected and both original-code negative control and independent patched run were repeated. The auditor inspected the real Regtest and full-suite evidence; it did not independently rerun those runs.

Both live phases removed their own resources. All 17 pre-existing running containers were preserved; final verification is `cleanup.json`. No mainnet/testnet or production wallet was used.

## Reproduction

Build the base and fixed full `ths` executables with their locked dependencies, retaining each binary. Ensure the three image tags used in `live-cli.py` are cached, Docker is available, and the selected loopback ports are free. The included harness differs from the executed private copy only in making its output directory the directory containing the script, rather than a machine-specific absolute directory. It still refuses to reuse a phase directory and checks the 45 GiB floor. Original executed harness SHA-256: `1be04a3eaa5e6ef422b13f2cfe5f9adf55df4590040d57985cfba116ff7ec5b6`.

```sh
python3 live-cli.py --binary /path/to/ths-before --phase before --port-offset 500
python3 live-cli.py --binary /path/to/ths-after --phase after --port-offset 600
```

Use a fresh directory for reproduction; the attached evidence already contains `before/` and `after/`. Startup logs contain disposable development credentials and have deliberately not been included. The public receipts contain only Regtest payment data and balance totals.

This fixes reported stdout write errors; it does not guarantee downstream consumption of buffered output, arbitrary-crash recovery or retry safety after every nonzero exit. A successfully reported terminal failure still releases its key. There are no server/API/dependency/image changes in this patch.
