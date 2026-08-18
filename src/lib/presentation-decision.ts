import type { DetectionResult } from "@/types"

export type PresentationAuthorization = NonNullable<
  DetectionResult["authorization"]
>
export type DetectionJob = NonNullable<DetectionResult["job"]>

export function detectionAuthorization(
  detection: DetectionResult
): PresentationAuthorization {
  return detection.authorization ?? "suggestion"
}

export function detectionJob(detection: DetectionResult): DetectionJob {
  if (detection.job) return detection.job
  if (detection.source === "direct") return "citation"
  return "quotation"
}

export function mayPreview(detection: DetectionResult): boolean {
  return (
    detectionAuthorization(detection) === "preview-authorized" ||
    detectionAuthorization(detection) === "reading-authorized" ||
    detectionAuthorization(detection) === "live-authorized"
  )
}

export function mayStartReading(detection: DetectionResult): boolean {
  return (
    detectionJob(detection) === "citation" &&
    (detectionAuthorization(detection) === "reading-authorized" ||
      detectionAuthorization(detection) === "live-authorized")
  )
}

export function mayGoLive(detection: DetectionResult): boolean {
  return detectionAuthorization(detection) === "live-authorized"
}

export function mayAutoQueue(detection: DetectionResult): boolean {
  return (
    detection.auto_queued &&
    detectionJob(detection) === "citation" &&
    (detectionAuthorization(detection) === "reading-authorized" ||
      detectionAuthorization(detection) === "live-authorized")
  )
}
