import type {
  ServiceContext,
  ServiceContextHymnSummary,
  ServiceContextItem,
  ServiceContextMediaSummary,
  ServiceItem,
  ServicePlan,
  ScriptureRef,
} from "@/types/service-plan"
import { findNextServiceItem } from "./service-plan-validation"

function formatScriptureRef(ref: ScriptureRef): string | null {
  if (ref.reference?.trim()) return ref.reference.trim()
  if (!ref.book) return null
  const chapter = ref.chapter ?? ""
  const verse = ref.verse ? `:${ref.verse}` : ""
  return `${ref.book} ${chapter}${verse}`.trim()
}

function collectExpectedReferences(item: ServiceItem | null): string[] {
  if (!item) return []

  const refs: string[] = []
  for (const scripture of item.scriptureRefs) {
    const formatted = formatScriptureRef(scripture)
    if (formatted) refs.push(formatted)
  }
  for (const hymn of item.hymnRefs) {
    if (hymn.hymnNumber) {
      refs.push(hymn.title ? `#${hymn.hymnNumber} ${hymn.title}` : `Hymn ${hymn.hymnNumber}`)
    }
  }
  for (const media of item.mediaRefs) {
    if (media.label) refs.push(media.label)
  }
  return refs
}

function toContextItem(item: ServiceItem | null): ServiceContextItem | null {
  if (!item) return null
  return {
    id: item.id,
    title: item.title,
    kind: item.kind,
    notes: item.notes ?? "",
    expectedReferences: collectExpectedReferences(item),
  }
}

function hymnSummariesForItems(items: ServiceItem[]): ServiceContextHymnSummary[] {
  const seen = new Set<number>()
  const summaries: ServiceContextHymnSummary[] = []

  for (const item of items) {
    for (const hymn of item.hymnRefs) {
      if (!hymn.hymnNumber || seen.has(hymn.hymnNumber)) continue
      seen.add(hymn.hymnNumber)
      summaries.push({
        hymnNumber: hymn.hymnNumber,
        title: hymn.title ?? `Hymn ${hymn.hymnNumber}`,
      })
    }
  }

  return summaries
}

function mediaSummariesForItems(
  active: ServiceItem | null,
  next: ServiceItem | null,
): ServiceContextMediaSummary[] {
  const summaries: ServiceContextMediaSummary[] = []

  const append = (item: ServiceItem, scope: "active" | "next") => {
    for (const attachment of item.attachments) {
      if (attachment.kind !== "media" && attachment.kind !== "slide" && attachment.kind !== "deck") continue
      summaries.push({
        id: attachment.id,
        label: attachment.path ?? attachment.label,
        status: attachment.status,
        scope,
      })
    }
    for (const media of item.mediaRefs) {
      summaries.push({
        id: media.attachmentId,
        label: media.path ?? media.label,
        status: "pending",
        scope,
      })
    }
  }

  if (active) append(active, "active")
  if (next) append(next, "next")
  return summaries
}

export function buildServiceContext(plan: ServicePlan | null): ServiceContext {
  if (!plan) {
    return {
      planId: "",
      planTitle: "",
      planStatus: "draft",
      mode: "planning",
      activeItem: null,
      nextItem: null,
      operatorNotes: "",
      expectedReferences: [],
      hymnSummaries: [],
      mediaSummaries: [],
      outputTemplateId: null,
      performanceMode: false,
    }
  }

  const active = plan.items.find((item) => item.id === plan.activeItemId) ?? null
  const next = findNextServiceItem(plan.items, plan.activeItemId)
  const focusItems = [active, next].filter((item): item is ServiceItem => item !== null)

  return {
    planId: plan.id,
    planTitle: plan.title,
    planStatus: plan.status,
    mode: plan.mode,
    activeItem: toContextItem(active),
    nextItem: toContextItem(next),
    operatorNotes: active?.notes ?? "",
    expectedReferences: collectExpectedReferences(active),
    hymnSummaries: hymnSummariesForItems(focusItems),
    mediaSummaries: mediaSummariesForItems(active, next),
    outputTemplateId: active?.outputTemplateId ?? null,
    performanceMode: plan.status === "live" && plan.mode === "performance",
  }
}
