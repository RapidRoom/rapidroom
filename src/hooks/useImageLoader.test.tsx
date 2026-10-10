// @vitest-environment jsdom
import { act, createElement, createRef } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, describe, expect, it } from 'vitest';
import EmbeddedPreviewWarning from '../components/panel/EmbeddedPreviewWarning';
import { Invokes, SelectedImage } from '../components/ui/AppProperties';
import '../i18n';
import { useEditorStore } from '../store/useEditorStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { useSettingsStore } from '../store/useSettingsStore';
import { mockCommand } from '../test/tauriMock';
import { INITIAL_ADJUSTMENTS } from '../utils/adjustments';
import { useImageLoader } from './useImageLoader';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const initialEditorState = useEditorStore.getState();
const initialLibraryState = useLibraryStore.getState();
const initialSettingsState = useSettingsStore.getState();

afterEach(() => {
  useEditorStore.setState(initialEditorState, true);
  useLibraryStore.setState(initialLibraryState, true);
  useSettingsStore.setState(initialSettingsState, true);
});

function image(path: string): SelectedImage {
  return {
    exif: null,
    height: 0,
    isRaw: true,
    isReady: false,
    path,
    thumbnailUrl: '',
    width: 0,
  };
}

function Harness({ cachedEditStateRef }: { cachedEditStateRef: React.RefObject<Record<string, unknown> | null> }) {
  useImageLoader(cachedEditStateRef);
  const selectedImage = useEditorStore((state) => state.selectedImage);
  return (
    <EmbeddedPreviewWarning
      isReady={selectedImage?.isReady ?? false}
      usedEmbeddedPreview={selectedImage?.usedEmbeddedPreview}
    />
  );
}

async function waitUntil(predicate: () => boolean) {
  for (let i = 0; i < 20; i += 1) {
    if (predicate()) return;
    await act(async () => new Promise((resolve) => setTimeout(resolve, 0)));
  }
  expect(predicate()).toBe(true);
}

describe('useImageLoader embedded-preview warning', () => {
  it('shows the localized alert for a fallback load and clears it after switching to a normal photo', async () => {
    mockCommand('clear_session_caches', () => undefined);
    mockCommand(Invokes.LoadMetadata, () => ({ adjustments: { is_null: true } }));
    mockCommand(Invokes.LoadImage, ({ path } = {}) => ({
      width: 640,
      height: 480,
      exif: {},
      metadata: {},
      is_raw: true,
      used_embedded_preview: path === '/photos/fallback.NEF',
    }));

    const container = document.createElement('div');
    const root = createRoot(container);
    const cachedEditStateRef = createRef<Record<string, unknown> | null>();
    await act(async () => {
      root.render(createElement(Harness, { cachedEditStateRef }));
      useEditorStore.getState().setEditor({
        selectedImage: image('/photos/fallback.NEF'),
        adjustments: INITIAL_ADJUSTMENTS,
      });
    });

    try {
      await waitUntil(() => useEditorStore.getState().selectedImage?.isReady === true);
      expect(useEditorStore.getState().selectedImage?.usedEmbeddedPreview).toBe(true);
      expect(container.querySelector('[role="alert"]')?.textContent).toBe(
        'RAW development failed; showing the embedded preview instead.',
      );

      await act(async () => {
        useEditorStore.getState().setEditor({ selectedImage: image('/photos/normal.NEF') });
      });
      await waitUntil(
        () =>
          useEditorStore.getState().selectedImage?.path === '/photos/normal.NEF' &&
          useEditorStore.getState().selectedImage?.isReady === true,
      );

      expect(useEditorStore.getState().selectedImage?.usedEmbeddedPreview).toBe(false);
      expect(container.querySelector('[role="alert"]')).toBeNull();
    } finally {
      await act(async () => root.unmount());
    }
  });
});
