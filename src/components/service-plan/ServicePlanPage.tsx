import { lazy, Suspense, useEffect, useMemo } from "react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { PanelHeader } from "@/components/ui/panel-header"
import { SERVICE_PLAN_TEMPLATES } from "@/lib/service-plan/service-plan-templates"
import { useServicePlanStore } from "@/stores/service-plan-store"
import {
  CalendarClockIcon,
  ClipboardListIcon,
  PlayIcon,
  RadioIcon,
  SkipForwardIcon,
} from "lucide-react"
import { ServiceTimeline } from "./ServiceTimeline"
import { ServiceItemDetailsPanel } from "./ServiceItemDetailsPanel"

export { ServiceLiveContextPanel } from "./ServiceLiveContextPanel"

const LazyServicePlanEditor = lazy(async () => ({
  default: ServicePlanEditor,
}))

function ServicePlanEditor() {
  const activePlan = useServicePlanStore((s) => s.activePlan)
  const serviceContext = useServicePlanStore((s) => s.serviceContext)
  const updatePlanTitle = useServicePlanStore((s) => s.updatePlanTitle)
  const updateItem = useServicePlanStore((s) => s.updateItem)
  const setActiveItem = useServicePlanStore((s) => s.setActiveItem)
  const deleteItem = useServicePlanStore((s) => s.deleteItem)
  const duplicateItem = useServicePlanStore((s) => s.duplicateItem)
  const reorderItems = useServicePlanStore((s) => s.reorderItems)
  const markItemReady = useServicePlanStore((s) => s.markItemReady)
  const completeActiveItem = useServicePlanStore((s) => s.completeActiveItem)
  const skipActiveItem = useServicePlanStore((s) => s.skipActiveItem)
  const goToNextItem = useServicePlanStore((s) => s.goToNextItem)
  const goToPreviousItem = useServicePlanStore((s) => s.goToPreviousItem)
  const startPractice = useServicePlanStore((s) => s.startPractice)
  const startLiveService = useServicePlanStore((s) => s.startLiveService)
  const completeService = useServicePlanStore((s) => s.completeService)
  const enqueuePreparedResources = useServicePlanStore((s) => s.enqueuePreparedResources)
  const practicePreviewActiveItem = useServicePlanStore((s) => s.practicePreviewActiveItem)
  const generatePostServiceReport = useServicePlanStore((s) => s.generatePostServiceReport)
  const addItem = useServicePlanStore((s) => s.addItem)
  const pendingReport = useServicePlanStore((s) => s.pendingReport)
  const lastReport = useServicePlanStore((s) => s.lastReport)

  const selectedItem = useMemo(
    () => activePlan?.items.find((item) => item.id === activePlan.activeItemId) ?? null,
    [activePlan],
  )

  if (!activePlan) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        No active service plan.
      </div>
    )
  }

  return (
    <div className="flex h-full min-h-0 flex-col" data-slot="service-plan-editor">
      <PanelHeader title={activePlan.title} icon={<ClipboardListIcon className="size-4" />}>
        <Badge variant="outline" className="text-[0.5625rem] uppercase">
          {activePlan.status}
        </Badge>
      </PanelHeader>

      <div className="border-b border-border px-3 py-2">
        <Input
          value={activePlan.title}
          onChange={(event) => updatePlanTitle(event.target.value)}
          className="h-8"
        />
      </div>

      <div className="grid min-h-0 flex-1 grid-cols-[minmax(0,1.1fr)_minmax(0,0.9fr)]">
        <div className="flex min-h-0 flex-col border-r border-border">
          <div className="flex items-center justify-between border-b border-border px-3 py-2">
            <span className="text-xs font-medium text-muted-foreground">Timeline</span>
            <Button
              size="sm"
              variant="outline"
              className="h-7 text-xs"
              disabled={serviceContext.performanceMode}
              onClick={() =>
                addItem({
                  title: "New item",
                  kind: "general",
                  scriptureRefs: [],
                  hymnRefs: [],
                  mediaRefs: [],
                  attachments: [],
                  checklist: [],
                })
              }
            >
              Add item
            </Button>
          </div>
          <div className="min-h-0 flex-1 p-2">
            <ServiceTimeline
              items={activePlan.items}
              activeItemId={activePlan.activeItemId}
              performanceMode={serviceContext.performanceMode}
              onSelect={(itemId) => void setActiveItem(itemId)}
              onDuplicate={duplicateItem}
              onDelete={deleteItem}
              onMarkReady={markItemReady}
              onComplete={() => void completeActiveItem()}
              onReorder={reorderItems}
            />
          </div>
        </div>

        <ServiceItemDetailsPanel
          item={selectedItem}
          serviceContext={serviceContext}
          onPatchItem={(patch) => {
            if (!selectedItem) return
            updateItem(selectedItem.id, patch)
          }}
          onEnqueuePrepared={() => void enqueuePreparedResources()}
          onPracticePreview={() => void practicePreviewActiveItem()}
        />
      </div>

      <div className="flex flex-wrap items-center gap-2 border-t border-border px-3 py-2">
        <Button size="sm" variant="outline" onClick={() => void startPractice()}>
          <PlayIcon className="size-3.5" />
          Practice
        </Button>
        <Button size="sm" onClick={() => void startLiveService()}>
          <RadioIcon className="size-3.5" />
          Start service
        </Button>
        <Button size="sm" variant="outline" onClick={() => void goToPreviousItem()}>
          Previous
        </Button>
        <Button size="sm" variant="outline" onClick={() => void goToNextItem()}>
          <SkipForwardIcon className="size-3.5" />
          Next
        </Button>
        <Button size="sm" variant="outline" onClick={() => void skipActiveItem()}>
          Skip
        </Button>
        <Button size="sm" variant="secondary" onClick={() => void completeService()}>
          Complete service
        </Button>
        {pendingReport && (
          <Button size="sm" variant="outline" onClick={generatePostServiceReport}>
            <CalendarClockIcon className="size-3.5" />
            Generate report
          </Button>
        )}
      </div>
      {lastReport && (
        <div className="border-t border-border px-3 py-2 text-xs text-muted-foreground">
          <div className="font-medium text-foreground">Post-service report</div>
          <div>
            {lastReport.completedItems}/{lastReport.totalItems} items completed -{" "}
            {lastReport.skippedItems} skipped - ~{lastReport.durationEstimateMinutes} min planned
          </div>
        </div>
      )}
    </div>
  )
}

export function ServicePlanSummaryWidget() {
  const summaries = useServicePlanStore((s) => s.summaries)
  const loadPlan = useServicePlanStore((s) => s.loadPlan)
  const openPlanner = useServicePlanStore((s) => s.openPlanner)

  if (summaries.length === 0) {
    return (
      <div className="rounded-md border border-dashed border-border px-3 py-4 text-xs text-muted-foreground">
        No service plans yet.
      </div>
    )
  }

  return (
    <div className="space-y-2" data-slot="service-plan-summary">
      {summaries.slice(0, 4).map((summary) => (
        <button
          key={summary.id}
          type="button"
          className="flex w-full items-center justify-between rounded-md border border-border px-3 py-2 text-left text-xs hover:bg-muted/40"
          onClick={() => void loadPlan(summary.id)}
        >
          <span className="font-medium">{summary.title}</span>
          <span className="text-muted-foreground">
            {summary.completedCount}/{summary.itemCount}
          </span>
        </button>
      ))}
      <Button size="sm" variant="outline" className="w-full" onClick={openPlanner}>
        Open planner
      </Button>
    </div>
  )
}

export function ServicePlanLibraryPanel() {
  const summaries = useServicePlanStore((s) => s.summaries)
  const createFromTemplate = useServicePlanStore((s) => s.createFromTemplate)
  const loadPlan = useServicePlanStore((s) => s.loadPlan)
  const hydrate = useServicePlanStore((s) => s.hydrate)
  const isHydrated = useServicePlanStore((s) => s.isHydrated)

  useEffect(() => {
    void hydrate()
  }, [hydrate])

  return (
    <div
      className="flex min-h-0 flex-col overflow-hidden rounded-lg border border-border bg-card"
      data-slot="service-plan-page"
    >
      <div className="space-y-3 overflow-y-auto p-3">
        <PanelHeader title="Service Plan" icon={<ClipboardListIcon className="size-4" />} />
        <ServicePlanSummaryWidget />
        <div className="grid gap-2">
          {SERVICE_PLAN_TEMPLATES.map((template) => (
            <Button
              key={template.id}
              variant="outline"
              size="sm"
              className="h-auto justify-start py-2 text-left"
              onClick={() => void createFromTemplate(template.id)}
            >
              <div>
                <div className="text-xs font-medium">{template.label}</div>
                <div className="text-[0.625rem] text-muted-foreground">{template.description}</div>
              </div>
            </Button>
          ))}
        </div>
        {isHydrated && summaries.length > 0 && (
          <div className="space-y-1">
            <div className="text-[0.625rem] font-medium uppercase text-muted-foreground">
              Recent plans
            </div>
            {summaries.map((summary) => (
              <Button
                key={summary.id}
                variant="ghost"
                size="sm"
                className="w-full justify-between"
                onClick={() => void loadPlan(summary.id)}
              >
                <span>{summary.title}</span>
                <Badge variant="outline">{summary.status}</Badge>
              </Button>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}

export function ServicePlanWorkspace() {
  const activePlan = useServicePlanStore((s) => s.activePlan)

  return (
    <div
      className="grid h-full min-h-0 gap-2 p-3 lg:grid-cols-[320px_minmax(0,1fr)]"
      data-slot="service-plan-workspace"
    >
      <ServicePlanLibraryPanel />
      <div className="min-h-0 overflow-hidden rounded-lg border border-border bg-card">
        {activePlan ? (
          <Suspense
            fallback={
              <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
                Loading planner...
              </div>
            }
          >
            <LazyServicePlanEditor />
          </Suspense>
        ) : (
          <div className="flex h-full items-center justify-center px-6 text-center text-sm text-muted-foreground">
            Choose a recent plan or create one from a template to begin.
          </div>
        )}
      </div>
    </div>
  )
}

export function ServicePlanDialog() {
  return <ServicePlanWorkspace />
}

export function ServicePlanPage() {
  return <ServicePlanWorkspace />
}

export const LazyServicePlanLibraryPanel = lazy(async () => ({
  default: ServicePlanLibraryPanel,
}))

export const LazyServicePlanWorkspace = lazy(async () => ({
  default: ServicePlanWorkspace,
}))

export const LazyServicePlanDialog = lazy(async () => ({
  default: ServicePlanDialog,
}))

export const LazyServicePlanPage = lazy(async () => ({
  default: ServicePlanPage,
}))
