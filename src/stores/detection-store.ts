import { create } from "zustand"
import type { DetectionResult } from "@/types"

interface DetectionWithMeta {
  detection: DetectionResult
  received_at: number
}

interface DetectionResultWithMeta extends DetectionResult {
  received_at?: number
}

const MAX_RECENT_DETECTIONS = 12

interface DetectionState {
  detections: DetectionResult[]
  autoMode: boolean
  confidenceThreshold: number

  addDetection: (detection: DetectionResult) => void
  addDetections: (detections: DetectionResult[]) => void
  setDetections: (detections: DetectionResult[]) => void
  removeDetection: (verseRef: string) => void
  clearDetections: () => void
  setAutoMode: (auto: boolean) => void
  setConfidenceThreshold: (threshold: number) => void
}

export const useDetectionStore = create<DetectionState>((set) => ({
  detections: [],
  autoMode: false,
  confidenceThreshold: 0.8,

  addDetection: (detection) =>
    set((state) => {
      const now = Date.now()
      const existingIndex = state.detections.findIndex((d) => d.verse_ref === detection.verse_ref)
      
      if (existingIndex >= 0) {
        const existing = state.detections[existingIndex] as DetectionResultWithMeta
        // On duplicate: keep newest received_at, max confidence, prefer non-empty newest verse_text
        const updated: DetectionResultWithMeta = {
          ...detection,
          confidence: Math.max(existing.confidence, detection.confidence),
          verse_text: detection.verse_text || existing.verse_text,
          received_at: now,
        }
        const newDetections = [...state.detections] as DetectionResultWithMeta[]
        newDetections[existingIndex] = updated
        // Sort by received_at desc, then confidence desc
        newDetections.sort((a, b) => {
          const aTime = a.received_at || now
          const bTime = b.received_at || now
          if (bTime !== aTime) return bTime - aTime
          return b.confidence - a.confidence
        })
        return { detections: newDetections.slice(0, MAX_RECENT_DETECTIONS) as DetectionResult[] }
      }
      
      // New detection
      const withMeta: DetectionResultWithMeta = { ...detection, received_at: now }
      const newDetections = [withMeta, ...state.detections] as DetectionResultWithMeta[]
      newDetections.sort((a, b) => {
        const aTime = a.received_at || now
        const bTime = b.received_at || now
        if (bTime !== aTime) return bTime - aTime
        return b.confidence - a.confidence
      })
      return { detections: newDetections.slice(0, MAX_RECENT_DETECTIONS) as DetectionResult[] }
    }),
  addDetections: (incoming) =>
    set((state) => {
      const now = Date.now()
      const map = new Map<string, DetectionWithMeta>()
      
      // Add incoming with received_at
      for (const d of incoming) {
        const existing = map.get(d.verse_ref)
        if (!existing) {
          map.set(d.verse_ref, { detection: d, received_at: now })
        } else {
          // Keep max confidence, prefer non-empty verse_text, newest received_at
          map.set(d.verse_ref, {
            detection: {
              ...d,
              confidence: Math.max(existing.detection.confidence, d.confidence),
              verse_text: d.verse_text || existing.detection.verse_text,
            },
            received_at: now,
          })
        }
      }
      
      // Merge existing detections
      for (const d of state.detections) {
        const existing = map.get(d.verse_ref)
        const dReceivedAt = (d as DetectionResultWithMeta).received_at || 0
        if (!existing) {
          map.set(d.verse_ref, { detection: d, received_at: dReceivedAt })
        } else {
          // Keep max confidence, prefer non-empty verse_text, newest received_at
          if (d.confidence > existing.detection.confidence) {
            map.set(d.verse_ref, {
              detection: {
                ...existing.detection,
                confidence: d.confidence,
                verse_text: d.verse_text || existing.detection.verse_text,
              },
              received_at: Math.max(existing.received_at, dReceivedAt),
            })
          } else if (dReceivedAt > existing.received_at) {
            map.set(d.verse_ref, {
              detection: {
                ...existing.detection,
                verse_text: d.verse_text || existing.detection.verse_text,
              },
              received_at: dReceivedAt,
            })
          }
        }
      }
      
      // Sort by received_at desc, then confidence desc
      const sorted = [...map.values()]
        .sort((a, b) => {
          if (b.received_at !== a.received_at) return b.received_at - a.received_at
          return b.detection.confidence - a.detection.confidence
        })
        .map((item) => ({ ...item.detection, received_at: item.received_at } as DetectionResultWithMeta))
        .slice(0, MAX_RECENT_DETECTIONS) as DetectionResult[]
      
      return { detections: sorted }
    }),
  setDetections: (detections) => set({ detections }),
  removeDetection: (verseRef) =>
    set((state) => ({
      detections: state.detections.filter((d) => d.verse_ref !== verseRef),
    })),
  clearDetections: () => set({ detections: [] }),
  setAutoMode: (autoMode) => set({ autoMode }),
  setConfidenceThreshold: (confidenceThreshold) =>
    set({ confidenceThreshold }),
}))
