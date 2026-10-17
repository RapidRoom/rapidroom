import { invoke } from '@tauri-apps/api/core';
import { toast } from 'react-toastify';
import i18n from 'i18next';
import { Invokes } from '../components/ui/AppProperties';

export interface TransferSummary {
  completed: number;
  renamed: number;
  identicalSkipped: number;
  failed: number;
  errors: string[];
}

export function showTransferSummary(summary: TransferSummary, operation: 'copy' | 'import'): void {
  const counts = {
    completed: summary.completed,
    renamed: summary.renamed,
    skipped: summary.identicalSkipped,
    failed: summary.failed,
  };
  const message = operation === 'copy' ? i18n.t('fileTransfers.copy', counts) : i18n.t('fileTransfers.import', counts);
  if (summary.failed) toast.error([message, ...summary.errors.slice(0, 3)].join('\n'), { autoClose: false });
  else toast.success(message);
}

export async function copyFilesWithSummary(sourcePaths: string[], destinationFolder: string): Promise<void> {
  const summary = await invoke<TransferSummary>(Invokes.CopyFiles, { sourcePaths, destinationFolder });
  showTransferSummary(summary, 'copy');
}
