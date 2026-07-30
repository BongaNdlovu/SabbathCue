import { parsePositiveSpokenNumber } from "@/lib/spoken-number"
import { presentQueuedItem } from "@/lib/queue-presentation"
import { useQueueStore } from "@/stores/queue-store"

const QUEUE_ITEM_COMMAND_PATTERN =
  /^(?:please\s+)?(?:(?:show|present|display)\s+|go\s+to\s+)?item(?:\s+number)?\s+(.+)$/
const DUPLICATE_WINDOW_MS = 5000

let lastHandled: { itemId: string; at: number } | null = null

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
  lastHandled = null
}

export function handleQueueItemVoiceControl(
  text: string,
  now = Date.now()
): boolean {
  const itemNumber = parseQueueItemCommand(text)
  if (itemNumber === null) return false

  const queue = useQueueStore.getState()
  const index = itemNumber - 1
  const item = queue.items[index]
  if (!item) return false

  if (
    lastHandled?.itemId === item.id &&
    now - lastHandled.at <= DUPLICATE_WINDOW_MS
  ) {
    return true
  }

  queue.setActive(index)
  presentQueuedItem(item)
  lastHandled = { itemId: item.id, at: now }
  return true
}
