import { cn } from "../../lib/utils";

export function Select({
  className,
  children,
  ...props
}: React.SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <select
      className={cn(
        "h-8 w-full rounded-md border border-border bg-card px-2.5 text-[13px] outline-none focus:border-primary",
        className,
      )}
      {...props}
    >
      {children}
    </select>
  );
}
