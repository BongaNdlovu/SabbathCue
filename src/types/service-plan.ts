export type ServicePlanStatus = "draft" | "practice" | "live" | "completed" | "archived"

export type ServiceMode = "planning" | "practice" | "performance"

export type ServiceItemStatus = "pending" | "ready" | "active" | "completed" | "skipped"

export type ServiceItemKind =
  | "general"
  | "scripture"
  | "hymn"
  | "media"
  | "slide"
  | "announcement"

export type ServiceAttachmentStatus = "pending" | "ready" | "failed" | "preloading"

export interface ServiceAttachment {
  id: string
  kind: "media" | "slide" | "document" | "deck"
  label: string
  path?: string
  status: ServiceAttachmentStatus
  thumbnailUrl?: string
  mimeType?: string
  sizeBytes?: number
  deckId?: string
}

export interface ServiceChecklistItem {
  id: string
  label: string
  done: boolean
}

export interface ScriptureRef {
  book?: string
  chapter?: number
  verse?: number
  reference?: string
}

export interface HymnRef {
  hymnId?: string
  hymnNumber?: number
  title?: string
  deckId?: string
}

export interface MediaRef {
  attachmentId: string
  label: string
  path?: string
}

export interface ServiceItem {
  id: string
  order: number
  title: string
  kind: ServiceItemKind
  durationMinutes?: number
  notes?: string
  status: ServiceItemStatus
  scriptureRefs: ScriptureRef[]
  hymnRefs: HymnRef[]
  mediaRefs: MediaRef[]
  attachments: ServiceAttachment[]
  checklist: ServiceChecklistItem[]
  outputTemplateId?: string
  slideDeckIds?: string[]
}

export interface ServicePlanSummary {
  id: string
  title: string
  status: ServicePlanStatus
  scheduledAt?: string
  itemCount: number
  completedCount: number
  templateId?: string
  updatedAt: number
}

export interface ServiceEventLogEntry {
  id: string
  at: number
  type: "item_activated" | "item_completed" | "item_skipped" | "mode_changed" | "note"
  message: string
}

export interface ServicePlan {
  id: string
  title: string
  status: ServicePlanStatus
  mode: ServiceMode
  templateId?: string
  scheduledAt?: string
  createdAt: number
  updatedAt: number
  activeItemId: string | null
  items: ServiceItem[]
  eventLog: ServiceEventLogEntry[]
  reportGeneratedAt?: number
}

export interface ServiceContextItem {
  id: string
  title: string
  kind: ServiceItemKind
  notes: string
  expectedReferences: string[]
}

export interface ServiceContextHymnSummary {
  hymnNumber: number
  title: string
}

export interface ServiceContextMediaSummary {
  id: string
  label: string
  status: ServiceAttachmentStatus
  scope: "active" | "next"
}

/** Live-facing contract — must stay small; never embed full ServicePlan. */
export interface ServiceContext {
  planId: string
  planTitle: string
  planStatus: ServicePlanStatus
  mode: ServiceMode
  activeItem: ServiceContextItem | null
  nextItem: ServiceContextItem | null
  operatorNotes: string
  expectedReferences: string[]
  hymnSummaries: ServiceContextHymnSummary[]
  mediaSummaries: ServiceContextMediaSummary[]
  outputTemplateId: string | null
  performanceMode: boolean
}

export interface PreparedQueueResource {
  label: string
  reference: string
  presentationKind: "scripture" | "hymn" | "media" | "slide"
}

export interface ServicePlanReport {
  planId: string
  title: string
  generatedAt: number
  completedItems: number
  skippedItems: number
  totalItems: number
  durationEstimateMinutes: number
  eventHighlights: string[]
  itemSummaries: Array<{
    title: string
    status: ServiceItem["status"]
    kind: ServiceItemKind
  }>
}

export interface ServicePlanRepository {
  listSummaries(): Promise<ServicePlanSummary[]>
  loadPlan(id: string): Promise<ServicePlan | null>
  savePlan(plan: ServicePlan): Promise<void>
  duplicatePlan(id: string): Promise<ServicePlan | null>
  archivePlan(id: string): Promise<void>
  deletePlan(id: string): Promise<void>
}
