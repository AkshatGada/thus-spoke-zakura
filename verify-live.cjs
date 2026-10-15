const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { randomUUID } = require('node:crypto');
const base = process.env.LIVE_BASE_URL || 'http://127.0.0.1:32805';
const target = new URL(base);
assert(target.protocol === 'http:' && ['127.0.0.1', 'localhost'].includes(target.hostname), 'Use a loopback Regtest endpoint.');
const out = require('node:path').resolve(process.env.EVIDENCE_DIR || './live-results');
fs.mkdirSync(out, { recursive: true });
const report = { source: '1d00c0b1ea17a4d1184753d937057a0c333069ed', started: new Date().toISOString(), cases: [] };
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function waitFor(check, label, timeout = 60000) {
  const start = Date.now();
  while (Date.now() - start < timeout) {
    const value = await check();
    if (value) return value;
    await delay(200);
  }
  throw new Error(`Timed out: ${label}`);
}
function record(name, data) {
  report.cases.push({ name, ...data });
  fs.writeFileSync(`${out}/live-report.json`, JSON.stringify(report, null, 2));
  console.log(JSON.stringify({ name, ...data }));
}
async function api(path) {
  const response = await fetch(`${base}/api/v1/${path}`, { signal: AbortSignal.timeout(5000) });
  assert(response.ok, `${path}: HTTP ${response.status}`);
  return response.json();
}
async function ready() {
  return waitFor(async () => {
    const status = await api('status');
    return status.settled && status.wallet_sync.state === 'ready' && status;
  }, 'settled wallet');
}
async function payment() {
  const before = await ready();
  const request = { from_account: 1, to_account: 2, source_pool: 'ironwood', destination_pool: 'ironwood', amount_zatoshi: 10000000, idempotency_key: `live-check-${randomUUID()}` };
  // Persist the retry identity before POST; an uncertain response must not create a new payment.
  fs.writeFileSync(`${out}/payment-request.json`, JSON.stringify(request, null, 2));
  const response = await fetch(`${base}/api/v1/send`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(request), signal: AbortSignal.timeout(10000) });
  assert.equal(response.status, 202);
  const admission = await response.json();
  const location = response.headers.get('location');
  assert.equal(location, `/api/v1/activity?idempotency_key=${encodeURIComponent(request.idempotency_key)}`);
  assert(['preparing', 'prepared', 'broadcast'].includes(admission.status));
  const confirmed = await waitFor(async () => {
    const rows = await api(location.replace('/api/v1/', ''));
    if (rows[0]?.status === 'failed') throw new Error(`Payment failed: ${rows[0].last_error}`);
    return rows[0]?.status === 'confirmed' && rows[0];
  }, 'original payment confirmed');
  const settled = await ready();
  const accounts = await api('accounts');
  assert.equal(accounts.find(a => a.id === 2).ironwood_zatoshi, 10000000);
  assert(confirmed.txid && confirmed.block_hash);
  const tx = await api(`transactions/${confirmed.txid}`);
  assert.equal(tx.blockhash, confirmed.block_hash);
  const replay = await fetch(`${base}/api/v1/send`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(request), signal: AbortSignal.timeout(10000) });
  assert.equal(replay.status, 200);
  const replayed = await replay.json();
  assert.equal(replayed.id, confirmed.id);
  assert.equal(replayed.txid, confirmed.txid);
  await delay(2500);
  const after = await ready();
  const accountsAfter = await api('accounts');
  assert.equal(after.node.blocks, settled.node.blocks);
  assert.deepEqual(accountsAfter.map(a => [a.id, a.ironwood_zatoshi, a.transparent_zatoshi]), accounts.map(a => [a.id, a.ironwood_zatoshi, a.transparent_zatoshi]));
  record('shielded payment admission, confirmation and same-key replay', { admission_http: response.status, replay_http: replay.status, admission_status: admission.status, activity_id: confirmed.id, txid: confirmed.txid, block_hash: confirmed.block_hash, start_height: before.node.blocks, end_height: after.node.blocks, recipient_zatoshi: 10000000, replay_unchanged: true });
}
async function openMine(page) {
  await page.goto(base);
  await page.getByRole('button', { name: 'Mine', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Mine blocks', exact: true });
  await dialog.waitFor();
  await waitFor(() => dialog.getByRole('button', { name: 'Mine blocks', exact: true }).isEnabled(), 'Mine enabled');
  return dialog;
}
async function blockedStorage(browser, mode) {
  const context = await browser.newContext();
  try {
    await context.addInitScript(mode => {
      if (mode === 'getter') {
        Object.defineProperty(window, 'sessionStorage', { configurable: true, get() { throw new DOMException('Injected live-test storage denial', 'SecurityError'); } });
      } else {
        const storage = window.sessionStorage;
        const method = mode === 'read' ? 'getItem' : 'setItem';
        const original = Storage.prototype[method];
        Storage.prototype[method] = function(key, ...args) {
          if (this === storage && key.startsWith('ths:mine:')) {
            if (mode === 'nonretaining') return;
            throw new DOMException('Injected live-test storage denial', 'SecurityError');
          }
          return original.call(this, key, ...args);
        };
      }
    }, mode);
    const page = await context.newPage();
    const errors = [];
    let posts = 0;
    page.on('pageerror', error => errors.push(error.message));
    page.on('request', request => { if (request.method() === 'POST' && new URL(request.url()).pathname === '/api/v1/mining/jobs') posts++; });
    const height = (await ready()).node.blocks;
    const dialog = await openMine(page);
    await dialog.getByRole('button', { name: 'Mine blocks', exact: true }).click();
    await dialog.getByRole('alert').filter({ hasText: 'No mining request was sent.' }).waitFor();
    assert.equal(posts, 0);
    assert.equal((await ready()).node.blocks, height);
    assert.equal(await dialog.getByRole('button', { name: 'Mine blocks', exact: true }).isEnabled(), true);
    await page.screenshot({ path: `${out}/storage-${mode}.png`, fullPage: true });
    await dialog.getByRole('button', { name: 'Close dialog' }).click();
    await dialog.waitFor({ state: 'hidden' });
    assert.deepEqual(errors, []);
    record(`real-browser storage ${mode} failure`, { mining_posts: posts, height_unchanged: true, dialog_closable: true, page_errors: errors });
  } finally { await context.close(); }
}
async function acceptedMining(browser, mode, blocks) {
  const context = await browser.newContext();
  try {
    const page = await context.newPage();
    const errors = [];
    let posts = 0, accepted, submitted;
    page.on('pageerror', error => errors.push(error.message));
    await page.route('**/api/v1/mining/jobs', async route => {
      if (route.request().method() !== 'POST') return route.continue();
      posts++;
      submitted = route.request().postDataJSON();
      const response = await route.fetch(); // The actual local backend accepts and mines this job.
      assert.equal(response.status(), 202);
      accepted = await response.json();
      if (mode === 'cleanup') {
        await page.evaluate(() => {
          const storage = window.sessionStorage;
          const original = Storage.prototype.removeItem;
          Storage.prototype.removeItem = function(key) {
            if (this === storage && key.startsWith('ths:mine:')) throw new DOMException('Injected cleanup failure after real acceptance', 'SecurityError');
            return original.call(this, key);
          };
        });
      } else if (mode === 'denied-after-unmount') {
        await page.getByRole('dialog', { name: 'Mine blocks' }).getByRole('button', { name: 'Close dialog' }).click();
        await page.evaluate(() => {
          Object.defineProperty(window, 'sessionStorage', { configurable: true, get() { throw new DOMException('Storage denied after real acceptance', 'SecurityError'); } });
        });
      }
      await route.fulfill({ response }); // Preserve the real backend's response, never fabricate a job.
    });
    const before = await ready();
    let dialog = await openMine(page);
    await dialog.getByLabel('Number of blocks').fill(String(blocks));
    await dialog.getByRole('button', { name: 'Mine blocks', exact: true }).click();
    await waitFor(() => accepted, 'actual mining acceptance');
    if (mode === 'denied-after-unmount') {
      await page.getByRole('button', { name: 'Mine', exact: true }).click();
      dialog = page.getByRole('dialog', { name: 'Mine blocks', exact: true });
    }
    if (mode === 'cleanup') await dialog.getByRole('alert').filter({ hasText: 'Mining job accepted, but browser storage cleanup failed:' }).waitFor();
    const completed = await waitFor(async () => {
      const job = (await api('mining/jobs')).job;
      if (job?.id === accepted.id && job.state === 'failed') throw new Error(job.error);
      return job?.id === accepted.id && job.state === 'completed' && job;
    }, 'actual job completion');
    await dialog.getByText('Mining completed. Wallet synchronized.', { exact: true }).waitFor();
    assert.equal(await dialog.getByText('Unable to start mining:', { exact: false }).count(), 0);
    assert.equal(completed.completed_blocks, blocks);
    assert.equal(posts, 1);
    const after = await ready();
    assert.equal(after.node.blocks - before.node.blocks, blocks);
    if (mode === 'cleanup') {
      const retained = await page.evaluate(blocks => sessionStorage.getItem(`ths:mine:${blocks}`), blocks);
      assert.equal(retained, submitted.idempotency_key);
    }
    assert.deepEqual(errors, []);
    await page.screenshot({ path: `${out}/mining-${mode}.png`, fullPage: true });
    record(`real mining ${mode}`, { job_id: completed.id, requested_blocks: blocks, completed_blocks: completed.completed_blocks, chain_height_delta: after.node.blocks - before.node.blocks, mining_posts: posts, page_errors: errors });
  } finally { await context.close(); }
}
(async () => {
  await payment();
  const browser = await chromium.launch({ headless: true, ...(process.env.CHROME_PATH ? { executablePath: process.env.CHROME_PATH } : {}) });
  try {
    for (const mode of ['getter', 'read', 'write', 'nonretaining']) await blockedStorage(browser, mode);
    await acceptedMining(browser, 'normal', 1);
    await acceptedMining(browser, 'cleanup', 2);
    await acceptedMining(browser, 'denied-after-unmount', 3);
  } finally { await browser.close(); }
  report.finished = new Date().toISOString();
  report.free_gib = Number(fs.statfsSync(out).bavail * fs.statfsSync(out).bsize / 1024**3).toFixed(1);
  fs.writeFileSync(`${out}/live-report.json`, JSON.stringify(report, null, 2));
  console.log('All live checks passed; free GiB:', report.free_gib);
})().catch(error => {
  report.error = error.stack;
  fs.writeFileSync(`${out}/live-report.json`, JSON.stringify(report, null, 2));
  console.error(error);
  process.exitCode = 1;
});
