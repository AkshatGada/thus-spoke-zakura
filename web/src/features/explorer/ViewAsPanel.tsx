import { useState, type FormEvent } from 'react';
import { Button } from '@/components/ui/Button';
import { DataTable, NumCell, Row, type Column } from '@/components/ui/DataTable';
import { Field } from '@/components/ui/Field';
import { PanelNote, Panel } from '@/components/ui/Panel';
import { Select, type SelectOption } from '@/components/ui/Select';
import { controlStyles } from '@/components/ui/control-styles';
import { useDecryptTransaction } from '@/hooks/mutations';
import { errorMessage, type TransactionViewer, type ViewedOutput } from '@/lib/api';
import { formatZecAmount } from '@/lib/money';
import { shortHash } from '@/lib/format';

type ViewerChoice = `account-${number}` | 'ufvk';

const VIEWER_OPTIONS: ReadonlyArray<SelectOption<ViewerChoice>> = [
  ...[1, 2, 3, 4, 5].map((id) => ({ value: `account-${id}` as const, label: `Account ${id}` })),
  { value: 'ufvk', label: 'Paste a viewing key' },
];

const COLUMNS: Column[] = [
  { key: 'index', header: '#', width: '56px' },
  { key: 'transfer', header: 'Seen as', width: '110px' },
  { key: 'address', header: 'Address', collapse: true },
  { key: 'memo', header: 'Memo' },
  { key: 'value', header: 'Value', align: 'right', width: '150px' },
];

const TRANSFER_LABEL: Record<ViewedOutput['transfer'], string> = {
  received: 'Received',
  change: 'Change',
  sent: 'Sent',
};

function MemoCell({ output }: { output: ViewedOutput }) {
  if (output.memo_text !== null) {
    return <span className="text-ink wrap-anywhere">{output.memo_text}</span>;
  }
  if (output.memo_hex !== null) {
    return (
      <code className="text-ink-muted font-mono wrap-anywhere" title="Not a text memo">
        0x{output.memo_hex}
      </code>
    );
  }
  return <span className="text-ink-muted">—</span>;
}

/**
 * Decrypts a transaction's shielded outputs with one development account or a
 * pasted unified full viewing key. The Explorer above only shows public data;
 * this shows what that key can see, and an unrelated key sees nothing.
 */
export function ViewAsPanel({ txid }: { txid: string }) {
  const [choice, setChoice] = useState<ViewerChoice>('account-1');
  const [ufvk, setUfvk] = useState('');
  const decrypt = useDecryptTransaction(txid);
  const pasting = choice === 'ufvk';

  const viewer = (): TransactionViewer =>
    pasting ? { ufvk: ufvk.trim() } : { account: Number(choice.slice('account-'.length)) };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    decrypt.mutate(viewer());
  };

  const outputs = decrypt.data?.outputs;

  return (
    <Panel eyebrow="VIEW AS" title="Decrypt with a viewing key">
      <form
        onSubmit={submit}
        noValidate
        className="grid gap-x-4 px-5 pt-4 sm:grid-cols-[220px_1fr]"
      >
        <Field label="Viewing key">
          {(aria) => (
            <Select
              {...aria}
              value={choice}
              options={VIEWER_OPTIONS}
              onValueChange={(value) => {
                setChoice(value);
                decrypt.reset();
              }}
            />
          )}
        </Field>
        {pasting ? (
          <Field
            label="Unified full viewing key"
            hint="Used for this request only and never stored."
          >
            {(aria) => (
              <textarea
                {...aria}
                value={ufvk}
                onChange={(event) => {
                  setUfvk(event.target.value);
                  decrypt.reset();
                }}
                rows={2}
                autoComplete="off"
                spellCheck={false}
                placeholder="uviewregtest1…"
                className={`${controlStyles} font-mono text-[12px]`}
              />
            )}
          </Field>
        ) : (
          <div className="mb-4 flex items-end">
            <p className="text-ink-muted text-[12px] leading-relaxed">
              Decrypts with this account's viewing key: outputs it received, its change, and what it
              sent.
            </p>
          </div>
        )}
        <div className="mb-4 sm:col-span-2">
          <Button
            type="submit"
            variant="primary"
            loading={decrypt.isPending}
            disabled={decrypt.isPending || (pasting && ufvk.trim() === '')}
          >
            {decrypt.isPending ? 'Decrypting…' : 'Decrypt'}
          </Button>
        </div>
      </form>

      {decrypt.isError && (
        <p role="alert" className="text-negative border-line border-t px-5 py-3 text-[12px]">
          {errorMessage(decrypt.error)}
        </p>
      )}

      {outputs && outputs.length > 0 && (
        <DataTable columns={COLUMNS}>
          {outputs.map((output, index) => (
            <Row key={`${output.pool}:${output.index}`} index={index}>
              <td className="text-ink-muted tabular-nums" title={`${output.pool} output`}>
                {output.index}
              </td>
              <td className="font-semibold">{TRANSFER_LABEL[output.transfer]}</td>
              <td className="max-sm:hidden">
                {output.recipient_address ? (
                  <code className="text-ink font-mono" title={output.recipient_address}>
                    {shortHash(output.recipient_address, 14, 8)}
                  </code>
                ) : (
                  <span className="text-ink-muted">—</span>
                )}
              </td>
              <td>
                <MemoCell output={output} />
              </td>
              <NumCell className="font-semibold">{formatZecAmount(output.value_zatoshi)}</NumCell>
            </Row>
          ))}
        </DataTable>
      )}

      {outputs && outputs.length === 0 && (
        <PanelNote>This key can't decrypt any output of this transaction.</PanelNote>
      )}

      {outputs && outputs.length > 0 && (
        <PanelNote>
          Only what this key can decrypt is shown. "Sent" outputs are recovered with the key's
          outgoing viewing key; transparent inputs and outputs are already public above.
        </PanelNote>
      )}
    </Panel>
  );
}
