import * as React from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { X } from "lucide-react";
import { cn } from "../../lib/utils";

export const Dialog = DialogPrimitive.Root;
export const DialogTrigger = DialogPrimitive.Trigger;
export const DialogClose = DialogPrimitive.Close;

export function DialogContent({
  className,
  children,
  title,
  onClose,
  width = "max-w-lg",
}: {
  className?: string;
  children: React.ReactNode;
  title?: string;
  onClose?: () => void;
  width?: string;
}) {
  return (
    <DialogPrimitive.Portal>
      <DialogPrimitive.Overlay className="fixed inset-0 z-50 bg-black/30" />
      <DialogPrimitive.Content
        className={cn(
          "fixed left-1/2 top-1/2 z-50 w-[min(92vw,720px)] -translate-x-1/2 -translate-y-1/2 rounded-md border border-border bg-card shadow-sm",
          width,
          className,
        )}
        onEscapeKeyDown={onClose}
        onPointerDownOutside={onClose}
      >
        {title && (
          <div className="flex items-center justify-between border-b border-border px-3 py-2">
            <DialogPrimitive.Title className="text-[13px] font-semibold">{title}</DialogPrimitive.Title>
            <button type="button" onClick={onClose} className="rounded-md p-1 hover:bg-muted">
              <X className="h-3.5 w-3.5" />
            </button>
          </div>
        )}
        {children}
      </DialogPrimitive.Content>
    </DialogPrimitive.Portal>
  );
}

export function AppDialog({
  open = true,
  title,
  onClose,
  children,
  width,
}: {
  open?: boolean;
  title: string;
  onClose: () => void;
  children: React.ReactNode;
  width?: string;
}) {
  return (
    <Dialog open={open} onOpenChange={(v) => { if (!v) onClose(); }}>
      <DialogContent title={title} onClose={onClose} width={width}>
        {children}
      </DialogContent>
    </Dialog>
  );
}
