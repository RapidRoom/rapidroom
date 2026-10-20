import { invoke } from '@tauri-apps/api/core';
import { toast } from 'react-toastify';
import i18n from 'i18next';

export function showInstagramExportComplete(folder: string) {
  toast.success(
    <div className="space-y-2">
      <p>{i18n.t('export.instagram.complete')}</p>
      <button
        type="button"
        className="underline font-medium"
        onClick={(event) => {
          event.stopPropagation();
          void invoke('open_export_folder', { path: folder }).catch((error: unknown) => toast.error(String(error)));
        }}
      >
        {i18n.t('export.instagram.openFolder')}
      </button>
    </div>,
    { autoClose: false, closeOnClick: false },
  );
}
