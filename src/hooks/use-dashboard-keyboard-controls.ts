import { useEffect } from "react"
import {
  commitPreviewToLive,
  presentItem,
  selectPreviewItem,
} from "@/lib/presentation-workflow"
import { useBroadcastStore } from "@/stores/broadcast-store"
import { useQueueStore } from "@/stores/queue-store"
import { useDashboardWorkspaceStore } from "@/stores/dashboard-workspace-store"
import { useHymnSlideStore } from "@/stores/hymn-slide-store"
import { useServicePlanStore } from "@/stores/service-plan-store"
import { useTranscriptStore } from "@/stores/transcript-store"
import { transcriptionActions } from "./use-transcription"

function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false

  if (target.isContentEditable) return true

  const tagName = target.tagName.toLowerCase()
  if (tagName === "input" || tagName === "textarea" || tagName === "select") {
    return true
  }

  return Boolean(
    target.closest(
      '[contenteditable="true"], input, textarea, select, [role="textbox"], [role="combobox"], [role="spinbutton"]',
    ),
  )
}

function selectQueueItem(delta: number): void {
  const queue = useQueueStore.getState()
  if (queue.items.length === 0) return

  const fallbackIndex = delta > 0 ? -1 : queue.items.length
  const nextIndex = Math.max(
    0,
    Math.min(queue.items.length - 1, (queue.activeIndex ?? fallbackIndex) + delta),
  )
  const item = queue.items[nextIndex]
  if (!item) return

  queue.setActive(nextIndex)
  selectPreviewItem(item.presentation)
}

function presentActiveQueueItem(): void {
  const queue = useQueueStore.getState()
  const index = queue.activeIndex ?? 0
  const item = queue.items[index]
  if (!item) return

  queue.setActive(index)
  presentItem(item.presentation)
}

function advanceLiveHymnSlide(delta: number): boolean {
  const broadcast = useBroadcastStore.getState()
  if (!broadcast.isLive || broadcast.liveItem?.kind !== "hymn") return false

  const queue = useQueueStore.getState()
  const activeQueueItem =
    queue.activeIndex === null ? null : queue.items[queue.activeIndex] ?? null

  if (activeQueueItem?.presentation.kind === "hymn" && activeQueueItem.hymnGroup) {
    const activeGroup = activeQueueItem.hymnGroup
    const targetItemIndex = activeGroup.itemIndex + delta
    const targetQueueIndex = queue.items.findIndex(
      (item) => {
        const group = item.hymnGroup
        return (
          item.presentation.kind === "hymn" &&
          group?.groupId === activeGroup.groupId &&
          group.itemIndex === targetItemIndex
        )
      },
    )
    const target = queue.items[targetQueueIndex]
    if (target) {
      queue.setActive(targetQueueIndex)
      presentItem(target.presentation)
      return true
    }
  }

  const hymnSlides = useHymnSlideStore.getState()
  if (hymnSlides.deck.length === 0) return false

  const liveScreenId = broadcast.liveItem.hymnSlide?.screenId
  const liveIndex = hymnSlides.deck.findIndex((item) => item.screenId === liveScreenId)
  const currentIndex = liveIndex === -1 ? hymnSlides.activeIndex : liveIndex
  const nextIndex = Math.max(0, Math.min(hymnSlides.deck.length - 1, currentIndex + delta))
  const next = hymnSlides.deck[nextIndex]
  if (!next || nextIndex === currentIndex) return true

  hymnSlides.setDeck(hymnSlides.deck, nextIndex)
  presentItem(next)
  return true
}

export function useDashboardKeyboardControls(): void {
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.repeat || isEditableTarget(event.target)) return

      const key = event.key.toLowerCase()
      const mod = event.ctrlKey || event.metaKey

      if (!mod && !event.altKey && !event.shiftKey && event.key === "ArrowRight") {
        if (advanceLiveHymnSlide(1)) event.preventDefault()
        return
      }

      if (!mod && !event.altKey && !event.shiftKey && event.key === "ArrowLeft") {
        if (advanceLiveHymnSlide(-1)) event.preventDefault()
        return
      }

      if (event.altKey && key === "1") {
        event.preventDefault()
        useDashboardWorkspaceStore.getState().setWorkspace("live")
        useServicePlanStore.getState().closePlanner()
        return
      }

      if (event.altKey && key === "2") {
        event.preventDefault()
        useDashboardWorkspaceStore.getState().setWorkspace("service-plans")
        useServicePlanStore.getState().openPlanner()
        return
      }

      if (event.altKey && key === "3") {
        event.preventDefault()
        useDashboardWorkspaceStore.getState().setWorkspace("hymns")
        useServicePlanStore.getState().closePlanner()
        return
      }

      if (mod && event.key === "Enter" && event.shiftKey) {
        event.preventDefault()
        presentActiveQueueItem()
        return
      }

      if (mod && event.key === "Enter") {
        event.preventDefault()
        commitPreviewToLive()
        return
      }

      if (mod && key === "l") {
        event.preventDefault()
        const broadcast = useBroadcastStore.getState()
        broadcast.setLive(!broadcast.isLive)
        return
      }

      if (mod && key === "m") {
        event.preventDefault()
        if (useTranscriptStore.getState().isTranscribing) {
          void transcriptionActions.stop()
        } else {
          void transcriptionActions.start()
        }
        return
      }

      if (event.altKey && event.key === "ArrowRight") {
        event.preventDefault()
        void useServicePlanStore.getState().goToNextItem()
        return
      }

      if (event.altKey && event.key === "ArrowLeft") {
        event.preventDefault()
        void useServicePlanStore.getState().goToPreviousItem()
        return
      }

      if (event.altKey && event.key === "ArrowDown") {
        event.preventDefault()
        selectQueueItem(1)
        return
      }

      if (event.altKey && event.key === "ArrowUp") {
        event.preventDefault()
        selectQueueItem(-1)
      }
    }

    window.addEventListener("keydown", handleKeyDown)
    return () => window.removeEventListener("keydown", handleKeyDown)
  }, [])
}
