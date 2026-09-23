import * as React from "react";
import { cn } from "../../lib/utils";

export const Input = React.forwardRef<HTMLInputElement, React.InputHTMLAttributes<HTMLInputElement>>(
  ({ className, ...props }, ref) => (
    <input
      ref={ref}
      className={cn(
        "flex h-8 w-full rounded-md border border-border bg-card px-2.5 text-[13px] outline-none placeholder:text-muted-foreground focus:border-primary",
        className,
      )}
      {...props}
    />
  ),
);
Input.displayName = "Input";
