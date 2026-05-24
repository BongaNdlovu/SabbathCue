import { buildServiceContext } from "@/lib/service-plan/service-context"
import { selectPreviewItem } from "@/lib/presentation-workflow"
import { generateHymnScreens } from "@/services/hymnal/generate-hymn-screens"
import {
  createHymnPresentationItem,
  defaultSelectedSectionIds,
} from "@/services/hymnal/hymn-presentation"
import { getHymnByNumber } from "@/services/hymnal/hymnal-repository"
import { mediaPreloadManager } from "@/services/media/media-preload-manager"
import type { ServiceContext, ServiceItem, ServicePlan } from "@/types/service-plan"

export function syncServiceContext(plan: ServicePlan | null): ServiceContext {
  const context = buildServiceContext(plan)
  if (plan) {
    mediaPreloadManager.syncFromContext(context)
  } else {
    mediaPreloadManager.releaseAll()
  }
  return context
}

export function releaseCompletedItemMedia(item: ServiceItem | undefined): void {
  if (!item) return
  for (const attachment of item.attachments) {
    mediaPreloadManager.releaseCompletedItem(attachment.id)
  }
  for (const media of item.mediaRefs) {
    mediaPreloadManager.releaseCompletedItem(media.attachmentId)
  }
}

export function releaseAllServiceMedia(): void {
  mediaPreloadManager.releaseAll()
}

export async function previewFirstHymnForItem(item: ServiceItem): Promise<void> {
  for (const hymnRef of item.hymnRefs) {
    if (!hymnRef.hymnNumber) continue
    try {
      const hymn = await getHymnByNumber(hymnRef.hymnNumber)
      if (!hymn) continue
      const screens = generateHymnScreens({
        hymn,
        selectedSectionIds: defaultSelectedSectionIds(hymn),
        maxLinesPerScreen: 4,
      })
      const first = screens[0]
      if (!first) continue
      selectPreviewItem(createHymnPresentationItem(first))
      return
    } catch {
      // Practice preview failure is non-fatal.
    }
  }
}
