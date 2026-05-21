import { useMemo, useRef, useState } from "react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { PanelEmptyState } from "@/components/ui/panel-empty-state"
import { PanelHeader } from "@/components/ui/panel-header"
import { presentItem, selectPreviewItem } from "@/lib/presentation-workflow"
import { cn } from "@/lib/utils"
import { useHymnSlideStore } from "@/stores/hymn-slide-store"
import { useQueueStore } from "@/stores/queue-store"
import type { HymnPresentationItemData, QueueItem } from "@/types"
import {
  FileUpIcon,
  ListMusicIcon,
  PlayIcon,
  PlusIcon,
  SendIcon,
  TextCursorInputIcon,
} from "lucide-react"

const SAMPLE_SLIDES = `Amazing Grace

Amazing grace, how sweet the sound
That saved a soul like me

I once was lost, but now am found
Was blind, but now I see`

function splitSlides(text: string): string[][] {
  return text
    .split(/\n\s*(?:---+|\n)\s*\n/g)
    .map((block) =>
      block
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean),
    )
    .filter((lines) => lines.length > 0)
}

function fileNameWithoutExtension(name: string): string {
  return name.replace(/\.[^.]+$/, "").trim()
}

function createSongSlideItem(
  title: string,
  lines: string[],
  index: number,
  total: number,
): HymnPresentationItemData {
  const trimmedTitle = title.trim() || "Custom Song"
  return {
    kind: "hymn",
    hymnId: `custom-song-${trimmedTitle.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`,
    hymnNumber: 0,
    hymnTitle: trimmedTitle,
    screenId: `custom-song-${index + 1}-${total}`,
    slideIndex: index,
    slideCount: total,
    reference: `${trimmedTitle} - Slide ${index + 1}/${total}`,
    segments: lines.map((text) => ({ text })),
  }
}

function createQueueItems(deck: HymnPresentationItemData[]): QueueItem[] {
  if (deck.length === 0) return []

  const groupId = `custom-song-${crypto.randomUUID()}`
  const groupLabel = `${deck[0].hymnTitle} - ${deck.length} slides`
  return deck.map((presentation, index) => ({
    id: `custom-song-slide-${crypto.randomUUID()}`,
    presentation,
    confidence: 1,
    source: "hymn",
    added_at: Date.now(),
    hymnGroup: {
      groupId,
      groupLabel,
      itemIndex: index + 1,
      itemCount: deck.length,
    },
  }))
}

export function SongSlidesWorkspace() {
  const [title, setTitle] = useState("Custom Song")
  const [rawSlides, setRawSlides] = useState(SAMPLE_SLIDES)
  const [activeIndex, setActiveIndex] = useState(0)
  const fileInputRef = useRef<HTMLInputElement>(null)

  const slideLines = useMemo(() => splitSlides(rawSlides), [rawSlides])
  const deck = useMemo(
    () =>
      slideLines.map((lines, index) =>
        createSongSlideItem(title, lines, index, slideLines.length),
      ),
    [slideLines, title],
  )
  const activeSlide = deck[Math.min(activeIndex, Math.max(0, deck.length - 1))]

  const registerDeck = (index: number) => {
    useHymnSlideStore.getState().setDeck(deck, index)
  }

  const handlePreview = () => {
    if (!activeSlide) return
    registerDeck(activeIndex)
    selectPreviewItem(activeSlide)
  }

  const handleLive = () => {
    if (!activeSlide) return
    registerDeck(activeIndex)
    presentItem(activeSlide)
  }

  const handleQueueAll = () => {
    const queueItems = createQueueItems(deck)
    useQueueStore.getState().addItems(queueItems)
    registerDeck(activeIndex)
  }

  const handleUpload = async (file: File | undefined) => {
    if (!file) return
    const text = await file.text()
    setTitle(fileNameWithoutExtension(file.name) || "Custom Song")
    setRawSlides(text)
    setActiveIndex(0)
  }

  return (
    <div
      className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-border bg-card"
      data-slot="song-slides-workspace"
    >
      <PanelHeader title="Song Slides" icon={<ListMusicIcon className="size-3" />}>
        <Badge variant="outline" className="h-5 text-[0.5625rem] uppercase">
          {deck.length} slides
        </Badge>
      </PanelHeader>

      <div className="grid min-h-0 flex-1 grid-cols-[minmax(280px,0.8fr)_minmax(320px,1fr)]">
        <div className="flex min-h-0 flex-col border-r border-border">
          <div className="space-y-2 border-b border-border p-3">
            <label className="text-[0.625rem] font-medium uppercase text-muted-foreground">
              Song title
            </label>
            <Input value={title} onChange={(event) => setTitle(event.target.value)} />
            <div className="flex items-center gap-2">
              <input
                ref={fileInputRef}
                type="file"
                accept=".txt,.md,.text"
                className="hidden"
                onChange={(event) => void handleUpload(event.target.files?.[0])}
              />
              <Button
                size="sm"
                variant="outline"
                onClick={() => fileInputRef.current?.click()}
              >
                <FileUpIcon className="size-3.5" />
                Upload text
              </Button>
              <span className="text-[0.625rem] text-muted-foreground">
                Blank lines or --- create new slides.
              </span>
            </div>
          </div>

          <div className="flex min-h-0 flex-1 flex-col p-3">
            <label className="mb-2 text-[0.625rem] font-medium uppercase text-muted-foreground">
              Slide text
            </label>
            <textarea
              value={rawSlides}
              onChange={(event) => {
                setRawSlides(event.target.value)
                setActiveIndex(0)
              }}
              spellCheck
              className="min-h-0 flex-1 resize-none rounded-md border border-input bg-background p-3 text-sm leading-relaxed outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
            />
          </div>
        </div>

        <div className="flex min-h-0 flex-col">
          <div className="grid min-h-10 grid-cols-[minmax(0,1fr)_auto] items-center gap-2 border-b border-border px-3 py-1.5">
            <div className="min-w-0">
              <p className="truncate text-sm font-medium">{title || "Custom Song"}</p>
              <p className="truncate text-xs text-muted-foreground">
                {activeSlide ? `Slide ${activeSlide.slideIndex + 1}/${activeSlide.slideCount}` : "No slides"}
              </p>
            </div>
            <div className="flex shrink-0 items-center gap-1">
              <Button size="xs" variant="outline" disabled={!activeSlide} onClick={handlePreview}>
                <SendIcon className="mr-1 size-3" />
                Preview
              </Button>
              <Button size="xs" variant="outline" disabled={deck.length === 0} onClick={handleQueueAll}>
                <PlusIcon className="mr-1 size-3" />
                Queue
              </Button>
              <Button size="xs" disabled={!activeSlide} onClick={handleLive}>
                <PlayIcon className="mr-1 size-3" />
                Live
              </Button>
            </div>
          </div>

          <div className="min-h-0 flex-1 overflow-y-auto p-3">
            {activeSlide ? (
              <div className="flex min-h-full flex-col gap-3">
                <div className="relative flex aspect-video items-center justify-center rounded-md border border-border bg-black p-8 text-center">
                  <span className="absolute right-3 top-3 rounded bg-white/10 px-2 py-0.5 text-[0.625rem] font-semibold text-white/80">
                    {activeSlide.slideIndex + 1}/{activeSlide.slideCount}
                  </span>
                  <div className="max-w-[80%] space-y-3 text-balance text-2xl font-semibold leading-snug text-white">
                    {activeSlide.segments.map((segment) => (
                      <p key={segment.text}>{segment.text}</p>
                    ))}
                  </div>
                </div>

                <div className="grid grid-cols-2 gap-1.5 xl:grid-cols-3">
                  {deck.map((slide, index) => (
                    <button
                      key={slide.screenId}
                      type="button"
                      onClick={() => setActiveIndex(index)}
                      className={cn(
                        "rounded-md border px-2 py-1.5 text-left text-xs transition-colors",
                        index === activeIndex
                          ? "border-lime-500/50 bg-lime-500/15"
                          : "border-border hover:bg-muted/50",
                      )}
                    >
                      <span className="block truncate font-medium">
                        {index + 1}. Slide
                      </span>
                      <span className="line-clamp-1 text-[0.68rem] text-muted-foreground">
                        {slide.segments.map((segment) => segment.text).join(" ")}
                      </span>
                    </button>
                  ))}
                </div>
              </div>
            ) : (
              <PanelEmptyState
                icon={<TextCursorInputIcon className="size-8" />}
                title="No song slides"
                description="Type or upload text to generate editable song slides."
              />
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
