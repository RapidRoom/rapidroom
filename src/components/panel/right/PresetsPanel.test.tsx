// @vitest-environment jsdom
import React, { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, it, expect, vi } from 'vitest';
import type { Preset } from '../../ui/AppProperties';
import { INITIAL_ADJUSTMENTS } from '../../../utils/adjustments';
import { useEditorStore } from '../../../store/useEditorStore';
import { useUIStore } from '../../../store/useUIStore';
import { clearPresetHoverPreview } from '../../../utils/presetHoverPreview';
import { debouncedSetHistory } from '../../../hooks/useEditorActions';
import PresetsPanel from './PresetsPanel';
const fixtures = vi.hoisted(() => ({
  presets: [{ preset: { id: 'warm', name: 'Warm', adjustments: { exposure: 2 } } as Preset }],
  isLoading: false,
}));
vi.mock('../../../hooks/usePresets', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../hooks/usePresets')>()),
  usePresets: () => fixtures,
}));
vi.mock('../../../context/ContextMenuContext', () => ({ useContextMenu: () => ({ showContextMenu: vi.fn() }) }));
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (k: string) => k }) }));
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let root: Root | undefined;
let container: HTMLDivElement | undefined;
afterEach(async () => {
  await act(async () => root?.unmount());
  root = undefined;
  container?.remove();
  fixtures.presets[0].preset.cameraModelRestriction = undefined;
  clearPresetHoverPreview();
  debouncedSetHistory.cancel();
});
it('search removal clears an owned hover preview', async () => {
  useEditorStore.getState().resetHistory({ ...INITIAL_ADJUSTMENTS });
  useEditorStore.setState({
    selectedImage: { path: '/review.raw', isReady: true, exif: {} } as NonNullable<
      ReturnType<typeof useEditorStore.getState>['selectedImage']
    >,
    previewOverride: null,
    isSliderDragging: false,
    showOriginal: false,
  });
  useUIStore.setState({ activeView: 'editor', presetCreationRequested: false });
  container = document.createElement('div');
  document.body.append(container);
  root = createRoot(container);
  await act(async () => root!.render(<PresetsPanel onNavigateToCommunity={() => {}} />));
  const row = [...container.querySelectorAll('[role=button]')].find((e) => e.textContent === 'Warm')!;
  expect(row).toBeTruthy();
  await act(async () => row.dispatchEvent(new MouseEvent('mouseover', { bubbles: true })));
  expect(useEditorStore.getState().previewOverride?.exposure).toBe(2);
  const input = container.querySelector('input[aria-label="editor.presets.search"]') as HTMLInputElement;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(input, 'no matches');
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 400));
  });
  expect(container!.textContent).not.toContain('Warm');
  expect(useEditorStore.getState().previewOverride).toBeNull();
});

it('compatibility filtering clears only the preview owned by this panel', async () => {
  useEditorStore.getState().resetHistory({ ...INITIAL_ADJUSTMENTS });
  useEditorStore.setState({
    selectedImage: { path: '/review.raw', isReady: true, exif: {} } as NonNullable<
      ReturnType<typeof useEditorStore.getState>['selectedImage']
    >,
    previewOverride: null,
    isSliderDragging: false,
    showOriginal: false,
  });
  useUIStore.setState({ activeView: 'editor', presetCreationRequested: false });
  container = document.createElement('div');
  document.body.append(container);
  root = createRoot(container);
  await act(async () => root!.render(<PresetsPanel onNavigateToCommunity={() => {}} />));
  const row = [...container.querySelectorAll('[role=button]')].find((e) => e.textContent === 'Warm')!;
  await act(async () => row.dispatchEvent(new MouseEvent('mouseover', { bubbles: true })));
  expect(useEditorStore.getState().previewOverride?.exposure).toBe(2);
  fixtures.presets[0].preset.cameraModelRestriction = 'Sony';
  await act(async () => (container!.querySelector('label input[type=checkbox]') as HTMLInputElement).click());
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 400));
  });
  expect((container!.querySelector('label input[type=checkbox]') as HTMLInputElement).checked).toBe(true);
  expect(container.textContent).not.toContain('Warm');
  expect(useEditorStore.getState().previewOverride).toBeNull();
  const foreign = { ...INITIAL_ADJUSTMENTS, exposure: 3 };
  useEditorStore.setState({ previewOverride: foreign });
  await act(async () => (container!.querySelector('label input[type=checkbox]') as HTMLInputElement).click());
  expect(useEditorStore.getState().previewOverride).toBe(foreign);
});
