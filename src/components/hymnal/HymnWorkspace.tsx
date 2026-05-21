import { lazy, Suspense, useState } from "react"
import { Button } from "@/components/ui/button"

const LazyHymnalPanel = lazy(() =>
  import("@/components/panels/hymnal-panel").then((mod) => ({
    default: mod.HymnalPanel,
  })),
)

const LazySongSlidesWorkspace = lazy(() =>
  import("./SongSlidesWorkspace").then((mod) => ({
    default: mod.SongSlidesWorkspace,
  })),
)

type HymnWorkspaceMode = "library" | "slides"

export function HymnWorkspace() {
  const [mode, setMode] = useState<HymnWorkspaceMode>("library")

  return (
    <div className="flex h-full min-h-0 flex-col gap-2 p-3" data-slot="hymn-workspace">
      <div className="flex items-center gap-1">
        <Button
          size="xs"
          variant={mode === "library" ? "default" : "outline"}
          aria-pressed={mode === "library"}
          onClick={() => setMode("library")}
        >
          Hymnal
        </Button>
        <Button
          size="xs"
          variant={mode === "slides" ? "default" : "outline"}
          aria-pressed={mode === "slides"}
          onClick={() => setMode("slides")}
        >
          Song Slides
        </Button>
      </div>

      <div className="min-h-0 flex-1">
        {mode === "library" ? (
          <Suspense fallback={<div className="h-full rounded-lg border border-border bg-card" />}>
            <LazyHymnalPanel />
          </Suspense>
        ) : (
          <Suspense fallback={<div className="h-full rounded-lg border border-border bg-card" />}>
            <LazySongSlidesWorkspace />
          </Suspense>
        )}
      </div>
    </div>
  )
}
