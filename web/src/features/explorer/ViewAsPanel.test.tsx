import { afterEach, describe, expect, it, vi } from 'vitest';
import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { renderWithProviders, requestUrl } from '@/test/utils';
import { ViewAsPanel } from './ViewAsPanel';

const TXID = '0d'.repeat(32);
const ADDRESS = 'uregtest1recipient';

function json(body: unknown, status = 200): Promise<Response> {
  return Promise.resolve(
    new Response(JSON.stringify(body), {
      status,
      headers: { 'content-type': 'application/json' },
    }),
  );
}

function renderPanel() {
  renderWithProviders(
    <MemoryRouter>
      <ViewAsPanel txid={TXID} />
    </MemoryRouter>,
  );
}

afterEach(() => {
  vi.restoreAllMocks();
});

// jsdom lacks the pointer-capture and scroll APIs Radix Select calls.
async function chooseViewer(name: string) {
  Object.assign(Element.prototype, {
    hasPointerCapture: () => false,
    releasePointerCapture: () => undefined,
    scrollIntoView: () => undefined,
  });
  await userEvent.click(screen.getByLabelText('Viewing key'));
  await userEvent.click(await screen.findByRole('option', { name }));
}

function requestBody(init: RequestInit | undefined): unknown {
  return JSON.parse(typeof init?.body === 'string' ? init.body : '{}');
}

function mockDecrypt(respond: (viewer: unknown) => Promise<Response>) {
  return vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
    expect(requestUrl(input)).toContain(`/transactions/${TXID}/decrypt`);
    expect(init?.method).toBe('POST');
    return respond(requestBody(init));
  });
}

describe('ViewAsPanel', () => {
  it("decrypts as a development account and shows its outputs' values and memos", async () => {
    const fetchSpy = mockDecrypt(() =>
      json({
        txid: TXID,
        mined_height: 284,
        outputs: [
          {
            pool: 'ironwood',
            index: 0,
            transfer: 'sent',
            value_zatoshi: 10_000_000,
            recipient_address: ADDRESS,
            memo_text: 'order 42',
            memo_hex: null,
          },
          {
            pool: 'ironwood',
            index: 1,
            transfer: 'change',
            value_zatoshi: 5_000,
            recipient_address: null,
            memo_text: null,
            memo_hex: 'ff01',
          },
        ],
      }),
    );
    renderPanel();

    await userEvent.click(screen.getByRole('button', { name: 'Decrypt' }));

    expect(await screen.findByText('order 42')).toBeInTheDocument();
    expect(screen.getByText('Sent')).toBeInTheDocument();
    expect(screen.getByText('Change')).toBeInTheDocument();
    expect(screen.getByText(ADDRESS)).toBeInTheDocument();
    expect(screen.getByText('0xff01')).toBeInTheDocument();
    expect(requestBody(fetchSpy.mock.calls[0]?.[1])).toEqual({ account: 1 });
  });

  it('sends a pasted viewing key in the request body and reports when it sees nothing', async () => {
    const fetchSpy = mockDecrypt(() => json({ txid: TXID, mined_height: 284, outputs: [] }));
    renderPanel();

    await chooseViewer('Paste a viewing key');
    const decrypt = screen.getByRole('button', { name: 'Decrypt' });
    expect(decrypt).toBeDisabled();
    await userEvent.type(screen.getByLabelText('Unified full viewing key'), ' uviewregtest1abc ');
    await userEvent.click(decrypt);

    expect(
      await screen.findByText("This key can't decrypt any output of this transaction."),
    ).toBeInTheDocument();
    const [input, init] = fetchSpy.mock.calls[0] ?? [];
    expect(requestUrl(input!)).not.toContain('uviewregtest1abc');
    expect(requestBody(init)).toEqual({ ufvk: 'uviewregtest1abc' });
  });

  it("shows the server's message when a key is rejected", async () => {
    mockDecrypt(() =>
      json(
        { error: { message: 'ufvk is not a Regtest unified full viewing key', status: 400 } },
        400,
      ),
    );
    renderPanel();

    await chooseViewer('Paste a viewing key');
    await userEvent.type(screen.getByLabelText('Unified full viewing key'), 'nope');
    await userEvent.click(screen.getByRole('button', { name: 'Decrypt' }));

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'ufvk is not a Regtest unified full viewing key',
    );
  });
});
