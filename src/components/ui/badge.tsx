import { cn } from "../../lib/utils";

export function Badge({
  className,
  variant = "default",
  children,
}: {
  className?: string;
  variant?: "default" | "success" | "danger" | "warning" | "accent";
  children: React.ReactNode;
}) {
  const colors = {
    default: "bg-muted text-muted-foreground",
    success: "bg-success/15 text-success",
    danger: "bg-destructive/15 text-destructive",
    warning: "bg-warning/15 text-warning",
    accent: "bg-primary/15 text-primary",
  };
  return (
    <span className={cn("inline-flex items-center rounded-sm px-1.5 py-0.5 text-[11px] font-medium", colors[variant], className)}>
      {children}
    </span>
  );
}
