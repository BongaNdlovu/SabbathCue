import { parsePositiveSpokenNumber } from "@/lib/spoken-number"
import { presentQueuedItem } from "@/lib/queue-presentation"
import { recordWorkflowTrace } from "@/lib/workflow-trace"
import { getBroadcastLiveStore } from "@/stores/broadcast/live-store"
import { useQueueStore } from "@/stores/queue-store"

const QUEUE_ITEM_COMMAND_PATTERN =
  /^(?:please\s+)?(?:(?:show|present|display)\s+|go\s+to\s+)?item(?:\s+number)?\s+(.+)$/
function normalizeTranscript(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9\s-]/g, " ")
    .replace(/\s+/g, " ")
    .trim()
}

export function parseQueueItemCommand(text: string): number | null {
  const normalized = normalizeTranscript(text)
  const match = normalized.match(QUEUE_ITEM_COMMAND_PATTERN)
  if (!match) return null

  return parsePositiveSpokenNumber(match[1])
}

export function resetQueueVoiceControlState(): void {
  // Kept for test and service reset callers. Queue voice control has no
  // module-local state: live identity belongs to the broadcast store.
}

export function handleQueueItemVoiceControl(text: string): boolean {
  const itemNumber = parseQueueItemCommand(text)
  if (itemNumber === null) return false

  const queue = useQueueStore.getState()
  const index = itemNumber - 1
  const item = queue.items[index]
  if (!item) {
    recordWorkflowTrace("queue.voice", "Queue voice command out of range", {
      outcome: "out_of_range",
      itemNumber,
      queueLength: queue.items.length,
    })
    return false
  }

  const live = getBroadcastLiveStore()
  if (live.isLive && live.liveQueueItemId === item.id) {
    recordWorkflowTrace("queue.voice", "Queue voice command already live", {
      outcome: "already_live",
      itemNumber,
      itemId: item.id,
    })
    return true
  }

  queue.setActive(index)
  presentQueuedItem(item)
  recordWorkflowTrace("queue.voice", "Queue voice command presented", {
    outcome: "presented",
    itemNumber,
    itemId: item.id,
  })
  return true
}
