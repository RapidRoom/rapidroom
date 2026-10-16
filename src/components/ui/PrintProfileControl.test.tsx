// @vitest-environment jsdom
import { act, createElement, useState } from 'react';
import { createRoot, Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import PrintProfileControl from './PrintProfileControl';
import type { PrintProfileOptions } from '../../utils/printProfile';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let root: Root;
let host: HTMLDivElement;
const changed = vi.fn();
let resetSelection: () => void;
afterEach(async () => {
  if (root) await act(async () => root.unmount());
  host?.remove();
  vi.resetAllMocks();
});

async function mount(initial: PrintProfileOptions | null = null, proof = false) {
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  function Harness() {
    const [value, onChange] = useState(initial);
    const [generation, setGeneration] = useState(0);
    resetSelection = () => {
      onChange(null);
      setGeneration((n) => n + 1);
    };
    return createElement(PrintProfileControl, {
      value,
      onChange: (next) => {
        changed(next);
        onChange(next);
      },
      proof,
      selectionGeneration: generation,
    });
  }
  await act(async () => root.render(createElement(Harness)));
  return host.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')[0];
}

async function choose(path: string) {
  vi.mocked(open).mockResolvedValueOnce(path);
  await act(async () => host.querySelector('button')!.click());
}

describe('local ICC profile controls', () => {
  it('starts off and choosing a validated profile still requires explicit enable', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const toggle = await mount(null, true);
    expect(toggle.checked).toBe(false);
    expect(toggle.disabled).toBe(true);
    await choose('/profiles/paper.icc');
    expect(changed).not.toHaveBeenCalled();
    expect(toggle.checked).toBe(false);
    expect(toggle.disabled).toBe(false);
    await act(async () => toggle.click());
    expect(toggle.checked).toBe(true);
    expect(invoke).toHaveBeenLastCalledWith('validate_print_profile', {
      proof: true,
      options: { profilePath: '/profiles/paper.icc', intent: 'relative', blackPointCompensation: true },
    });
    await act(async () => toggle.click());
    expect(toggle.checked).toBe(false);
  });

  it('does not activate or replace a valid choice when another profile fails validation', async () => {
    const initial: PrintProfileOptions = {
      profilePath: '/profiles/valid.icc',
      intent: 'relative',
      blackPointCompensation: true,
    };
    vi.mocked(invoke).mockRejectedValueOnce('Export needs an RGB output profile');
    const toggle = await mount(initial);
    await choose('/profiles/cmyk.icc');
    expect(toggle.checked).toBe(true);
    expect(host.querySelector('button')!.textContent).toBe('valid.icc');
    expect(host.querySelector('[role="alert"]')!.textContent).toContain('RGB');
  });

  it('keeps controls disabled during validation and shows a missing-file error', async () => {
    let reject!: (error: string) => void;
    vi.mocked(invoke).mockReturnValueOnce(
      new Promise((_resolve, fail) => {
        reject = fail;
      }),
    );
    const toggle = await mount();
    await choose('/profiles/missing.icc');
    expect(host.querySelector('fieldset')!.disabled).toBe(true);
    await act(async () => reject('Cannot open the chosen ICC profile'));
    expect(toggle.checked).toBe(false);
    expect(host.querySelector('[role="alert"]')!.textContent).toContain('Cannot open');
  });
  it('keeps a recipe reset off when active-profile validation completes late', async () => {
    let resolve!: () => void;
    vi.mocked(invoke).mockReturnValueOnce(
      new Promise<void>((done) => {
        resolve = done;
      }),
    );
    const toggle = await mount({ profilePath: '/first.icc', intent: 'relative', blackPointCompensation: true });
    await choose('/second.icc');
    await act(async () => resetSelection());
    expect(toggle.checked).toBe(false);
    await act(async () => resolve());
    expect(toggle.checked).toBe(false);
    expect(changed).not.toHaveBeenCalled();
  });

  it('cancels pending enable even when recipe reset leaves the parent value null', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined);
    const toggle = await mount();
    await choose('/first.icc');
    let resolve!: () => void;
    vi.mocked(invoke).mockReturnValueOnce(
      new Promise<void>((done) => {
        resolve = done;
      }),
    );
    await act(async () => toggle.click());
    await act(async () => resetSelection());
    await act(async () => resolve());
    expect(toggle.checked).toBe(false);
    expect(changed).not.toHaveBeenCalled();
  });
});
