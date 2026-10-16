import { useEffect } from 'react';
import clsx from 'clsx';
import { Slide, ToastContainer, toast } from 'react-toastify';

export default function AppNotifications({ isLightTheme }: { isLightTheme: boolean }) {
  useEffect(() => {
    const persistentErrors = new Set<string | number>();
    return toast.onChange(({ id, containerId, type, status }) => {
      if (status === 'removed' || type !== 'error') {
        persistentErrors.delete(id);
      } else if (!persistentErrors.has(id)) {
        persistentErrors.add(id);
        toast.update(id, { containerId, autoClose: false, closeOnClick: false });
      }
    });
  }, []);

  return (
    <ToastContainer
      position="bottom-right"
      autoClose={5000}
      hideProgressBar={false}
      newestOnTop={false}
      closeOnClick
      rtl={false}
      pauseOnFocusLoss
      draggable={false}
      pauseOnHover
      theme={isLightTheme ? 'light' : 'dark'}
      transition={Slide}
      style={{ maxHeight: 'calc(100dvh - 32px)', overflowY: 'auto', overflowAnchor: 'none' }}
      toastClassName={({ defaultClassName, type } = {}) =>
        clsx(
          defaultClassName,
          'shrink-0 min-h-16 p-4 rounded-lg overflow-hidden mb-4',
          'bg-surface! text-text-primary! border! border-border-color! shadow-2xl! max-w-[420px]!',
          type === 'error' && 'order-first',
        )
      }
    />
  );
}
