import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ThemeToggle } from './ThemeToggle';
import { THEME_STORAGE_KEY } from '@/lib/theme';

beforeEach(() => {
  localStorage.clear();
  document.documentElement.removeAttribute('data-theme');
});

afterEach(() => {
  vi.restoreAllMocks();
});

function renderToggle() {
  render(
    <>
      <button>Before theme</button>
      <ThemeToggle />
      <button>After theme</button>
    </>,
  );
}

describe('ThemeToggle', () => {
  it('has one Tab stop at the selected option, in both directions', async () => {
    const user = userEvent.setup();
    renderToggle();

    await user.tab();
    expect(screen.getByRole('button', { name: 'Before theme' })).toHaveFocus();
    await user.tab();
    expect(screen.getByRole('radio', { name: 'System' })).toHaveFocus();
    await user.tab();
    expect(screen.getByRole('button', { name: 'After theme' })).toHaveFocus();
    await user.tab({ shift: true });
    expect(screen.getByRole('radio', { name: 'System' })).toHaveFocus();
  });

  it.each([
    { from: 'Light', key: '{ArrowRight}', to: 'System' },
    { from: 'Light', key: '{ArrowDown}', to: 'System' },
    { from: 'System', key: '{ArrowLeft}', to: 'Light' },
    { from: 'System', key: '{ArrowUp}', to: 'Light' },
    { from: 'Light', key: '{ArrowLeft}', to: 'Dark' },
    { from: 'Dark', key: '{ArrowRight}', to: 'Light' },
  ])('moves focus and selection from $from to $to with $key', async ({ from, key, to }) => {
    const user = userEvent.setup();
    renderToggle();
    await user.click(screen.getByRole('radio', { name: from }));
    await user.keyboard(key);

    const selected = screen.getByRole('radio', { name: to });
    expect(selected).toHaveFocus();
    expect(selected).toHaveAttribute('aria-checked', 'true');
    expect(screen.getAllByRole('radio', { checked: true })).toEqual([selected]);
    expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe(to.toLowerCase());
    expect(document.documentElement.getAttribute('data-theme')).toBe(
      to === 'System' ? null : to.toLowerCase(),
    );
    await user.tab();
    expect(screen.getByRole('button', { name: 'After theme' })).toHaveFocus();
  });

  it.each([' ', '{Enter}'])('keeps native button activation working with %s', async (key) => {
    const user = userEvent.setup();
    renderToggle();
    const dark = screen.getByRole('radio', { name: 'Dark' });
    dark.focus();
    await user.keyboard(key);

    expect(dark).toHaveAttribute('aria-checked', 'true');
    expect(document.documentElement).toHaveAttribute('data-theme', 'dark');
  });

  it('uses a saved selection as the Tab stop', async () => {
    localStorage.setItem(THEME_STORAGE_KEY, 'dark');
    const user = userEvent.setup();
    renderToggle();
    await user.tab();
    await user.tab();
    expect(screen.getByRole('radio', { name: 'Dark' })).toHaveFocus();
  });
});
