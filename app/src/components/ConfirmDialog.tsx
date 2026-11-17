import { useEffect, useRef } from "react";

interface ConfirmDialogProps {
  title: string;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
  children: React.ReactNode;
}

/** Diálogo modal de confirmação. O foco inicial fica em "Cancelar". */
export default function ConfirmDialog({
  title,
  confirmLabel,
  onConfirm,
  onCancel,
  children,
}: ConfirmDialogProps) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  const cancelCb = useRef(onCancel);
  cancelCb.current = onCancel;
  useEffect(() => {
    cancelRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") cancelCb.current();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="dialog-backdrop">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="confirm-title"
        className="dialog"
      >
        <h2 id="confirm-title">{title}</h2>
        <div className="dialog-body">{children}</div>
        <div className="dialog-actions">
          <button
            type="button"
            className="btn"
            ref={cancelRef}
            onClick={onCancel}
          >
            Cancelar
          </button>
          <button type="button" className="btn btn-danger" onClick={onConfirm}>
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
