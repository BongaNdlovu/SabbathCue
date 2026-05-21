import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { useServicePlanStore } from "@/stores/service-plan-store"
import {
  ListMusicIcon,
  SkipForwardIcon,
  ChevronLeftIcon,
  CheckIcon,
} from "lucide-react"

/**
 * Live Control panel — reads only {@link getServiceContext}, never the full plan.
 */
export function ServiceLiveContextPanel() {
  const context = useServicePlanStore((s) => s.serviceContext)
  const goToNextItem = useServicePlanStore((s) => s.goToNextItem)
  const goToPreviousItem = useServicePlanStore((s) => s.goToPreviousItem)
  const completeActiveItem = useServicePlanStore((s) => s.completeActiveItem)
  const skipActiveItem = useServicePlanStore((s) => s.skipActiveItem)
  const enqueuePreparedResources = useServicePlanStore((s) => s.enqueuePreparedResources)

  if (!context.planId || context.mode === "planning") return null

  return (
    <div
      className="grid gap-2 border-b border-border bg-muted/20 px-3 py-2 text-xs"
      data-slot="service-live-context"
    >
      <div className="flex items-center justify-between gap-2">
        <span className="font-medium">{context.planTitle}</span>
        <Badge variant="outline" className="text-[0.5rem] uppercase">
          {context.mode}
        </Badge>
      </div>
      <div className="grid grid-cols-2 gap-2">
        <div>
          <div className="text-[0.625rem] uppercase text-muted-foreground">Current</div>
          <div>{context.activeItem?.title ?? "—"}</div>
        </div>
        <div>
          <div className="text-[0.625rem] uppercase text-muted-foreground">Next</div>
          <div>{context.nextItem?.title ?? "—"}</div>
        </div>
      </div>
      {context.operatorNotes ? (
        <p className="text-muted-foreground">{context.operatorNotes}</p>
      ) : null}
      {context.expectedReferences.length > 0 && (
        <div className="flex flex-wrap gap-1">
          {context.expectedReferences.map((ref) => (
            <Badge key={ref} variant="secondary" className="text-[0.5rem]">
              {ref}
            </Badge>
          ))}
        </div>
      )}
      {context.mediaSummaries.length > 0 && (
        <div className="flex items-center gap-1 text-muted-foreground">
          <ListMusicIcon className="size-3" />
          <span>{context.mediaSummaries.length} media resource(s) tracked</span>
        </div>
      )}
      <div className="flex flex-wrap gap-1.5 pt-1">
        <Button
          size="xs"
          variant="outline"
          className="h-6 gap-1"
          onClick={() => void goToPreviousItem()}
        >
          <ChevronLeftIcon className="size-3" />
          Prev
        </Button>
        <Button
          size="xs"
          variant="outline"
          className="h-6 gap-1"
          onClick={() => void goToNextItem()}
        >
          <SkipForwardIcon className="size-3" />
          Next
        </Button>
        <Button
          size="xs"
          variant="outline"
          className="h-6 gap-1"
          onClick={() => void completeActiveItem()}
        >
          <CheckIcon className="size-3" />
          Done
        </Button>
        <Button size="xs" variant="ghost" className="h-6" onClick={() => void skipActiveItem()}>
          Skip
        </Button>
        <Button
          size="xs"
          variant="secondary"
          className="h-6"
          onClick={() => void enqueuePreparedResources()}
        >
          Queue prepared
        </Button>
      </div>
    </div>
  )
}
