import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Invokes } from '../components/ui/AppProperties';
import type { Adjustments } from '../utils/adjustments';

export type LensCorrectionSource = 'lensfun' | 'camera_data' | 'legacy';

export interface LensCorrectionResolution {
  recipe_version: number;
  mode: 'auto' | 'manual';
  components: Record<
    'distortion' | 'tca' | 'vignetting',
    {
      available_source: Exclude<LensCorrectionSource, 'legacy'> | null;
      applied_source: LensCorrectionSource | null;
      enabled: boolean;
      amount: number;
    }
  >;
}

export function useLensCorrectionResolution(path: string | undefined, adjustments: Adjustments, active: boolean) {
  const request = useMemo(() => JSON.stringify({ path, adjustments }), [path, adjustments]);
  const [result, setResult] = useState<{
    request: string;
    resolution: LensCorrectionResolution | null;
  } | null>(null);

  useEffect(() => {
    if (!active || !path || adjustments.lensCorrectionMode !== 'auto') return;
    let cancelled = false;
    const timer = setTimeout(() => {
      invoke<LensCorrectionResolution>(Invokes.GetLensCorrectionResolution, JSON.parse(request))
        .then((resolution) => {
          if (!cancelled) setResult({ request, resolution });
        })
        .catch(() => {
          if (!cancelled) setResult({ request, resolution: null });
        });
    }, 250);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [active, path, adjustments.lensCorrectionMode, request]);

  const current = result?.request === request ? result : null;
  return { resolution: current?.resolution ?? null, failed: !!current && !current.resolution };
}
