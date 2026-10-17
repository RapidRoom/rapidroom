import { useEffect, useState } from 'react';
import { useEditorStore } from '../store/useEditorStore';
import {
  hashPresetAdjustments,
  loadAppliedPreset,
  useAppliedPresetStore,
  type AppliedPreset,
} from '../utils/appliedPreset';
import type { Adjustments } from '../utils/adjustments';

export function useAppliedPreset(): AppliedPreset | null {
  const path = useEditorStore((s) => (s.selectedImage?.isReady ? s.selectedImage.path : null));
  const adjustments = useEditorStore((s) => s.adjustments);
  const record = useAppliedPresetStore((s) => (path ? s.records[path] : null));
  const [matched, setMatched] = useState<{ path: string; adjustments: Adjustments; record: AppliedPreset } | null>(
    null,
  );
  useEffect(() => {
    if (path) void loadAppliedPreset(path);
  }, [path]);
  useEffect(() => {
    let current = true;
    if (path && record) {
      void hashPresetAdjustments(adjustments)
        .then((hash) => {
          if (current && hash === record.adjustmentsHash) setMatched({ path, adjustments, record });
        })
        .catch(() => {});
    }
    return () => {
      current = false;
    };
  }, [path, adjustments, record]);
  // Hide immediately on edits/path changes, including while the new hash is pending.
  return matched?.path === path && matched.adjustments === adjustments && matched.record === record
    ? (record ?? null)
    : null;
}
