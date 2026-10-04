interface UndoToastProps {
  message: string;
  undoLabel: string;
  onUndo(): void;
  closeToast?(): void;
}

export default function UndoToast({ message, undoLabel, onUndo, closeToast }: UndoToastProps) {
  return (
    <div className="flex items-center justify-between gap-3">
      <span>{message}</span>
      <button
        className="px-2 py-1 rounded-md bg-surface text-text-primary text-xs font-semibold hover:bg-card-active transition-colors"
        onClick={() => {
          closeToast?.();
          onUndo();
        }}
      >
        {undoLabel}
      </button>
    </div>
  );
}
