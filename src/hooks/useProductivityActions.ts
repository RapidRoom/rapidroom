import { useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useUIStore } from '../store/useUIStore';
import { Invokes } from '../components/ui/AppProperties';
import type { DenoiseMethod } from '../components/modals/DenoiseModal';

function isCurrentDenoiseRun(run: number) {
  const { denoiseRun, denoiseModalState } = useUIStore.getState();
  return denoiseRun === run && denoiseModalState.isOpen && denoiseModalState.isProcessing;
}

// The dialog may have been cancelled while the backend was allocating the job.
async function createDenoiseJob(run: number): Promise<number | null> {
  const jobId = await invoke<number>(Invokes.CreateDenoiseJob);
  if (!isCurrentDenoiseRun(run)) {
    await invoke(Invokes.CancelDenoise, { jobId });
    return null;
  }
  useUIStore.getState().setUI((state) => ({ denoiseModalState: { ...state.denoiseModalState, jobId } }));
  return jobId;
}

export function useProductivityActions(refreshImageList: () => Promise<void>) {
  const setUI = useUIStore((state) => state.setUI);

  const handleStartPanorama = useCallback(
    (paths: string[]) => {
      setUI((state) => ({
        panoramaModalState: {
          ...state.panoramaModalState,
          isProcessing: true,
          error: null,
          finalImageBase64: null,
          progressMessage: 'Starting panorama process...',
        },
      }));
      invoke(Invokes.StitchPanorama, { paths }).catch((err) => {
        setUI((state) => ({
          panoramaModalState: { ...state.panoramaModalState, isProcessing: false, error: String(err) },
        }));
      });
    },
    [setUI],
  );

  const handleSavePanorama = useCallback(async (): Promise<string> => {
    const { panoramaModalState } = useUIStore.getState();
    if (panoramaModalState.stitchingSourcePaths.length === 0) {
      const err = 'Source paths for panorama not found.';
      setUI((state) => ({ panoramaModalState: { ...state.panoramaModalState, error: err } }));
      throw new Error(err);
    }
    try {
      const savedPath: string = await invoke(Invokes.SavePanorama, {
        firstPathStr: panoramaModalState.stitchingSourcePaths[0],
      });
      await refreshImageList();
      return savedPath;
    } catch (err) {
      console.error('Failed to save panorama:', err);
      setUI((state) => ({ panoramaModalState: { ...state.panoramaModalState, error: String(err) } }));
      throw err;
    }
  }, [refreshImageList, setUI]);

  const handleStartFocusStack = useCallback(
    (paths: string[]) => {
      setUI((state) => ({
        focusStackModalState: {
          ...state.focusStackModalState,
          isProcessing: true,
          error: null,
          finalImageBase64: null,
          depthMapBase64: null,
          progressMessage: 'Starting focus stacking process...',
        },
      }));
      invoke(Invokes.StitchFocusStack, { paths }).catch((err) => {
        setUI((state) => ({
          focusStackModalState: { ...state.focusStackModalState, isProcessing: false, error: String(err) },
        }));
      });
    },
    [setUI],
  );

  const handleSaveFocusStack = useCallback(async (): Promise<string> => {
    const { focusStackModalState } = useUIStore.getState();
    if (focusStackModalState.sourcePaths.length === 0) {
      const err = 'Source paths for focus stack not found.';
      setUI((state) => ({ focusStackModalState: { ...state.focusStackModalState, error: err } }));
      throw new Error(err);
    }
    try {
      const savedPath: string = await invoke(Invokes.SaveFocusStack, {
        firstPathStr: focusStackModalState.sourcePaths[0],
      });
      await refreshImageList();
      return savedPath;
    } catch (err) {
      console.error('Failed to save focus stack:', err);
      setUI((state) => ({ focusStackModalState: { ...state.focusStackModalState, error: String(err) } }));
      throw err;
    }
  }, [refreshImageList, setUI]);

  const handleStartHdr = useCallback(
    (paths: string[]) => {
      setUI((state) => ({
        hdrModalState: {
          ...state.hdrModalState,
          isProcessing: true,
          error: null,
          finalImageBase64: null,
          progressMessage: 'Starting HDR process...',
        },
      }));
      invoke(Invokes.MergeHdr, { paths }).catch((err) => {
        setUI((state) => ({ hdrModalState: { ...state.hdrModalState, isProcessing: false, error: String(err) } }));
      });
    },
    [setUI],
  );

  const handleSaveHdr = useCallback(async (): Promise<string> => {
    const { hdrModalState } = useUIStore.getState();
    if (hdrModalState.stitchingSourcePaths.length === 0) {
      const err = 'Source paths for HDR not found.';
      setUI((state) => ({ hdrModalState: { ...state.hdrModalState, error: err } }));
      throw new Error(err);
    }
    try {
      const savedPath: string = await invoke(Invokes.SaveHdr, { firstPathStr: hdrModalState.stitchingSourcePaths[0] });
      await refreshImageList();
      return savedPath;
    } catch (err) {
      console.error('Failed to save HDR image:', err);
      setUI((state) => ({ hdrModalState: { ...state.hdrModalState, error: String(err) } }));
      throw err;
    }
  }, [refreshImageList, setUI]);

  const handleApplyDenoise = useCallback(
    async (intensity: number, method: DenoiseMethod, sharpen = true) => {
      const { denoiseModalState, denoiseRun } = useUIStore.getState();
      if (!denoiseModalState.isOpen || denoiseModalState.isProcessing || denoiseModalState.targetPaths.length === 0)
        return;

      const run = denoiseRun + 1;
      setUI((state) => ({
        denoiseRun: run,
        denoiseModalState: {
          ...state.denoiseModalState,
          isProcessing: true,
          jobId: null,
          error: null,
          previewBase64: null,
          originalBase64: null,
          progressMessage: 'Starting engine...',
        },
      }));

      try {
        const jobId = await createDenoiseJob(run);
        if (jobId === null) return;
        await invoke(Invokes.ApplyDenoising, {
          jobId,
          path: denoiseModalState.targetPaths[0],
          intensity: intensity,
          method: method,
          sharpen,
        });
      } catch (err) {
        if (isCurrentDenoiseRun(run))
          setUI((state) => ({
            denoiseModalState: { ...state.denoiseModalState, isProcessing: false, error: String(err) },
          }));
      }
    },
    [setUI],
  );

  const handleBatchDenoise = useCallback(
    async (intensity: number, method: DenoiseMethod, paths: string[], sharpen = true) => {
      const { denoiseModalState, denoiseRun } = useUIStore.getState();
      if (!denoiseModalState.isOpen || denoiseModalState.isProcessing || paths.length === 0) return [];
      const run = denoiseRun + 1;
      setUI((state) => ({
        denoiseRun: run,
        denoiseModalState: { ...state.denoiseModalState, isProcessing: true, jobId: null, error: null },
      }));
      try {
        const jobId = await createDenoiseJob(run);
        if (jobId === null) return [];
        const savedPaths: string[] = await invoke(Invokes.BatchDenoiseImages, {
          jobId,
          paths,
          intensity,
          method,
          sharpen,
        });
        await refreshImageList();
        return savedPaths;
      } catch (err) {
        if (isCurrentDenoiseRun(run))
          setUI((state) => ({ denoiseModalState: { ...state.denoiseModalState, error: String(err) } }));
        throw err;
      } finally {
        if (isCurrentDenoiseRun(run))
          setUI((state) => ({ denoiseModalState: { ...state.denoiseModalState, isProcessing: false } }));
      }
    },
    [refreshImageList, setUI],
  );

  const handleSaveDenoisedImage = useCallback(async (): Promise<string> => {
    const { denoiseModalState } = useUIStore.getState();
    if (denoiseModalState.targetPaths.length === 0) throw new Error('No target path');
    if (denoiseModalState.jobId === null) throw new Error('No denoised image to save');
    const savedPath = await invoke<string>(Invokes.SaveDenoisedImage, {
      jobId: denoiseModalState.jobId,
      originalPathStr: denoiseModalState.targetPaths[0],
    });
    await refreshImageList();
    return savedPath;
  }, [refreshImageList]);

  const handleSaveCollage = useCallback(
    async (base64Data: string, firstPath: string): Promise<string> => {
      try {
        const savedPath: string = await invoke(Invokes.SaveCollage, { base64Data, firstPathStr: firstPath });
        await refreshImageList();
        return savedPath;
      } catch (err) {
        console.error('Failed to save collage:', err);
        throw err;
      }
    },
    [refreshImageList],
  );

  return {
    handleStartPanorama,
    handleSavePanorama,
    handleStartHdr,
    handleSaveHdr,
    handleApplyDenoise,
    handleBatchDenoise,
    handleSaveDenoisedImage,
    handleSaveCollage,
    handleStartFocusStack,
    handleSaveFocusStack,
  };
}
