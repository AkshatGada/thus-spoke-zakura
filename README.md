# Local test evidence for #356

Implementation commit: `51fcaec11b2eb9c7d8f1faf4b1c90c9fb143e9d9`.
Audited source tree: `8808aafc4756c5d6d1f5db841cba6dc0f38488c0`.
Base: `9e89a4dd16361353bdfb325e9309a0359bb505ee`.

This branch holds evidence separately from the implementation diff. Results are from local checks and a fresh rebuilt Regtest instance, not upstream CI.

## Automated checks

Node 24.19.0: lint, formatting, **376 tests in 34 files**, production build, book links and whitespace checks passed. Existing bundle-size warning remains. No Rust source or dependencies changed.

- `checks/before-tests.log`:11 new behavioral regressions failed against original production code.
- `checks/tests.log`: full passing suite.
- Other `checks/` logs: lint, formatting, build, book links and whitespace.
- `AUDIT.md`: independent Astra medium audit, no actionable introduced defects;67 focused tests independently rerun and passed.

Logs retain captured output except for removed ANSI colors and local home/checkout paths replaced with placeholders.

## Rebuilt artifact

Standard `ths build --dev` completed using the preserved unchanged source CLI. Temporary build-context ignore excluded generated output only, then was removed.

App image: `sha256:dc0df16562a6af4931c035e17615f68f016db75c85d1423e89b7e32f86e06856`.
Lightwalletd image: `sha256:01f2ba37c0905e7f31439060a1f317769dfbf139caecad95499306794d451583`.
Zakura: pinned 1.6.0, Regtest only.

The running app's frontend hashes matched the locally built assets:

- `index-C7VQvHJ4.css`: `cccec4f0048ce03eae46caaadc96fe5efd097ee6420a7ae4d0caf0406e0ac588`
- `index-DhASQbJc.js`: `49cf3ced901051f02f826ac3ac33e194f2580ffd27a6b00013af33c2eaad2ff0`

## Actual Regtest/browser verification

Fresh uniquely owned Zakura, lightwalletd and rebuilt app services, all host ports on 127.0.0.1. No mining fault or production patch was needed.

1. Wallet account 2 link opened its stable detail page; direct reload worked. All 3 completed clipboard writes exactly matched the existing accounts API: unified address, transparent address and viewing key.
2. The actual account 2 unified address searched in Explorer opened the same detail page. A synthetic unknown UA-shaped string retained the no-public-history explanation.
3. Real 0.1 ZEC Ironwood account 1 → transparent account 2 confirmed at block 106; recipient received 10,000,000 zatoshi. Same-key replay preserved identity, balances and chain tip.
4. Real 0.05 ZEC transparent account 2→transparent account 3 confirmed at block 107. Its enriched input matched account 2's address; output paid 5,000,000 zatoshi to account 3. Browser showed Account2/Account3 labels. The account 3 link opened detail, while the raw input address still opened public history. Same-key replay again left state unchanged.
5. Known transparent address detail linked account 2; Explorer DOM did not contain the viewing key. Public mining address kept public history without a development-account label. `/wallet/accounts/6` showed Account not found.
6. At 390 px viewport, body/client width both 375 px and one h1; long fields wrapped. Viewport override reset afterward.
7. Browser checks left height 107 and balances unchanged. Fixture exited 0 and removed its 6 owned resources; all 17 pre-existing containers were preserved. Guard 45 GiB, above requested 40 GB floor.

Transaction1: `3aca5560bc7b75ea3d903ade467b2c18193af62e0929256b907238208d77d4e1`.
Block106: `eec2871dc98d7203dc9d2fdd2e8d98d7e1837f32aa0955b4cc7c50cf263359c9`.
Transaction2: `d43279bd0a9c296f9a680c5ec53e1e10d9ba59e02515292e32c85accf0ef1432`.
Block107: `5825b17ad434266b1e40535c19d08a7ca94c700379864b2359b60d65b396ed5f`.

## Evidence files and limits

`regtest/` contains selected captured transaction responses, balances, UI observations, canonical block/miner checks and cleanup. Transaction summaries omit raw hex and shielded proof/ciphertext; wallet seeds, spending keys, private initialization/configuration and request keys are excluded.

`screenshots/` contains Wallet links, desktop/mobile detail, transaction labels, known-address label, unlabelled mining address and unknown-UA search. All are from the actual rebuilt local app. The public account-detail screenshots are fresh captures from a second owned instance using the same verified app image, cropped above the viewing-key row. No viewing-key values are published. The remaining screenshots and payment receipts are from the original verification instance. The second screenshot-only instance made no payments and was cleaned up after capture.

Missing-key compatibility, failed lookup/retry and late-search cases were tested with controlled unit responses, not a live older server or browser network-fault run. The auditor inspected the saved live proof and did not repeat the live run. `SHA256SUMS` identifies this publication snapshot.
