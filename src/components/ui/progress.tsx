import { cn } from "../../lib/utils";

export function Progress({ value, className }: { value: number; className?: string }) {
  const v = Math.max(0, Math.min(100, value));
  return (
    <div className={cn("h-1.5 w-full overflow-hidden rounded-sm bg-muted", className)}>
      <div className="h-full bg-primary" style={{ width: `${v}%` }} />
    </div>
  );
}
