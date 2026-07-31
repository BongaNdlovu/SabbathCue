import { presentItem, selectPreviewItem } from "@/lib/presentation-workflow"
import { restorePresentationDeckForQueueItem } from "@/lib/queued-presentation-deck"
import { useEgwSlideStore } from "@/stores/egw-slide-store"
import { useHymnSlideStore } from "@/stores/hymn-slide-store"
import { useSermonSlideStore } from "@/stores/sermon-slide-store"
import { getBroadcastLiveStore } from "@/stores/broadcast/live-store"
import type { QueueItem } from "@/types"

export function previewQueuedItem(item: QueueItem): void {
  restorePresentationDeckForQueueItem(item)
  selectPreviewItem(item.presentation)
}

export function presentQueuedItem(item: QueueItem): void {
  restorePresentationDeckForQueueItem(item)
  presentItem(item.presentation)
  getBroadcastLiveStore().setLiveQueueItemId(item.id)
}

export function previewQueuedItemAtEnd(item: QueueItem): void {
  restorePresentationDeckForQueueItem(item)
  const kind = item.presentation.kind

  if (kind === "egw") {
    const store = useEgwSlideStore.getState()
    const last = store.deck[store.deck.length - 1]
    if (last) {
      store.setDeck(store.deck, store.deck.length - 1)
      selectPreviewItem(last)
      return
    }
  }

  if (kind === "slideDeck") {
    const store = useSermonSlideStore.getState()
    const last = store.deck[store.deck.length - 1]
    if (last) {
      store.setDeck(store.deck, store.deck.length - 1, store.activeItemId)
      selectPreviewItem(last)
      return
    }
  }

  if (kind === "hymn") {
    const store = useHymnSlideStore.getState()
    const last = store.deck[store.deck.length - 1]
    if (last) {
      store.setDeck(store.deck, store.deck.length - 1)
      selectPreviewItem(last)
      return
    }
  }

  selectPreviewItem(item.presentation)
}

/**
 * Present a queue item positioned on its LAST slide. Used when walking the
 * queue backward (ArrowLeft at the first slide of the live item) so the
 * previous item opens on its final slide, mirroring PowerPoint navigation.
 * Falls back to presenting the item as-is for single-slide kinds.
 */
export function presentQueuedItemAtEnd(item: QueueItem): void {
  restorePresentationDeckForQueueItem(item)
  const kind = item.presentation.kind

  if (kind === "egw") {
    const store = useEgwSlideStore.getState()
    const last = store.deck[store.deck.length - 1]
    if (last) {
      store.setDeck(store.deck, store.deck.length - 1)
      presentItem(last)
      getBroadcastLiveStore().setLiveQueueItemId(item.id)
      return
    }
  }

  if (kind === "slideDeck") {
    const store = useSermonSlideStore.getState()
    const last = store.deck[store.deck.length - 1]
    if (last) {
      store.setDeck(store.deck, store.deck.length - 1, store.activeItemId)
      presentItem(last)
      getBroadcastLiveStore().setLiveQueueItemId(item.id)
      return
    }
  }

  if (kind === "hymn") {
    const store = useHymnSlideStore.getState()
    const last = store.deck[store.deck.length - 1]
    if (last) {
      store.setDeck(store.deck, store.deck.length - 1)
      presentItem(last)
      getBroadcastLiveStore().setLiveQueueItemId(item.id)
      return
    }
  }

  presentItem(item.presentation)
  getBroadcastLiveStore().setLiveQueueItemId(item.id)
}
