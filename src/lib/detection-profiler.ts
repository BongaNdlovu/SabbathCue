interface DetectionProfilerStats {
  eventName: string
  count: number
  lastAt: number
  totalItems: number
  totalDurationMs: number
  maxDurationMs: number
}

const statsByEvent = new Map<string, DetectionProfilerStats>()
const LOG_INTERVAL_MS = 5000

export function profileDetectionEvent<T>(
  eventName: "verse_detections" | "reading_mode_verse",
  itemCount: number,
  run: () => T,
): T {
  if (!import.meta.env.DEV) return run()

  const startedAt = performance.now()
  try {
    return run()
  } finally {
    const duration = performance.now() - startedAt
    const now = Date.now()
    const current =
      statsByEvent.get(eventName) ??
      {
        eventName,
        count: 0,
        lastAt: 0,
        totalItems: 0,
        totalDurationMs: 0,
        maxDurationMs: 0,
      }

    current.count += 1
    current.totalItems += itemCount
    current.totalDurationMs += duration
    current.maxDurationMs = Math.max(current.maxDurationMs, duration)

    if (now - current.lastAt >= LOG_INTERVAL_MS) {
      current.lastAt = now
      console.debug("[detection-profiler]", {
        eventName: current.eventName,
        events: current.count,
        avgItems: Number((current.totalItems / current.count).toFixed(2)),
        avgDurationMs: Number((current.totalDurationMs / current.count).toFixed(2)),
        maxDurationMs: Number(current.maxDurationMs.toFixed(2)),
      })
    }

    statsByEvent.set(eventName, current)
  }
}
