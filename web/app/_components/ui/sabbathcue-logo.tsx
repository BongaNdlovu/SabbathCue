import { cn } from "../../_lib/utils";
import { APP_DISPLAY_NAME } from "../../_lib/app-brand";

export function SabbathCueLogo({
  className,
  wordmarkClassName,
  size = "md",
}: {
  className?: string;
  wordmarkClassName?: string;
  size?: "sm" | "md" | "lg";
}) {
  const iconSize =
    size === "sm" ? "size-6" : size === "lg" ? "size-10" : "size-[34px]";
  const textSize =
    size === "sm"
      ? "text-lg tracking-[-0.6px] leading-6"
      : size === "lg"
        ? "text-[28px] tracking-[-1px] leading-[36px]"
        : "text-2xl tracking-[-0.8px] leading-[32px]";

  return (
    <span className={cn("inline-flex items-center gap-2", className)}>
      <span
        className={cn(
          "flex items-center justify-center rounded-md bg-primary/10 font-semibold text-primary",
          iconSize
        )}
        aria-hidden
      >
        <span className="text-[0.55em] leading-none tracking-tight">SC</span>
      </span>
      <span className={cn("font-medium text-foreground", textSize, wordmarkClassName)}>
        {APP_DISPLAY_NAME}
      </span>
    </span>
  );
}
