import { useEffect, useMemo, useRef, useState } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Badge } from "@/components/ui/badge"
import { PanelHeader } from "@/components/ui/panel-header"
import { PanelEmptyState } from "@/components/ui/panel-empty-state"
import { cn } from "@/lib/utils"
import { presentItem, selectPreviewItem } from "@/lib/presentation-workflow"
import { useQueueStore } from "@/stores/queue-store"
import { useHymnSlideStore } from "@/stores/hymn-slide-store"
import {
  defaultSelectedSectionIds,
  createHymnPresentationItem,
  createGroupedHymnQueueItems,
} from "@/services/hymnal/hymn-presentation"
import {
  getRecentHymns,
  addRecentHymn,
  getFavoriteHymns,
  toggleFavoriteHymn,
} from "@/services/hymnal/hymnal-history"
import { generateHymnScreens } from "@/services/hymnal/generate-hymn-screens"
import {
  getHymnById,
  getInitialHymns,
  searchHymns,
} from "@/services/hymnal/hymnal-repository"
import { SDA_HYMNAL_INDEX } from "@/data/sda-hymnal-index"
import type { Hymn, HymnSearchResult } from "@/types"
import {
  BookOpenTextIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
  HeartIcon,
  ListMusicIcon,
  PlayIcon,
  PlusIcon,
  SearchIcon,
  SendIcon,
  StarIcon,
} from "lucide-react"

export function HymnalPanel() {
  const [query, setQuery] = useState("")
  const [selectedHymn, setSelectedHymn] = useState<Hymn | null>(null)
  const [selectedSectionIds, setSelectedSectionIds] = useState<string[]>([])
  const [activeScreenIndex, setActiveScreenIndex] = useState(0)
  const [isLoadingHymn, setIsLoadingHymn] = useState(true)
  const [viewMode, setViewMode] = useState<"search" | "recent" | "favorites">("search")
  const [recentHymnIds, setRecentHymnIds] = useState<string[]>(() => getRecentHymns())
  const [favoriteHymnIds, setFavoriteHymnIds] = useState<string[]>(() => getFavoriteHymns())
  const panelRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    let cancelled = false
    getHymnById("sda-1")
      .then((hymn) => {
        if (cancelled || !hymn) return
        setSelectedHymn(hymn)
        setSelectedSectionIds(defaultSelectedSectionIds(hymn))
      })
      .finally(() => {
        if (!cancelled) setIsLoadingHymn(false)
      })
    return () => {
      cancelled = true
    }
  }, [])

  const results = useMemo<HymnSearchResult[]>(() => {
    if (viewMode === "favorites") {
      return favoriteHymnIds
        .map((id) => SDA_HYMNAL_INDEX.find((h) => h.id === id))
        .filter((h): h is NonNullable<typeof h> => h !== undefined)
    }
    if (viewMode === "recent" && !query.trim()) {
      return recentHymnIds
        .map((id) => SDA_HYMNAL_INDEX.find((h) => h.id === id))
        .filter((h): h is NonNullable<typeof h> => h !== undefined)
    }
    return query.trim() ? searchHymns(query, 24) : getInitialHymns(24)
  }, [favoriteHymnIds, query, recentHymnIds, viewMode])

  const screens = useMemo(
    () =>
      selectedHymn
        ? generateHymnScreens({
            hymn: selectedHymn,
            selectedSectionIds,
            maxLinesPerScreen: 4,
          })
        : [],
    [selectedHymn, selectedSectionIds],
  )

  const activeScreen = screens[Math.min(activeScreenIndex, Math.max(0, screens.length - 1))]
  const presentationDeck = useMemo(
    () => screens.map((screen) => createHymnPresentationItem(screen)),
    [screens],
  )

  const goToPreviousScreen = () => {
    setActiveScreenIndex((current) => Math.max(0, current - 1))
  }

  const goToNextScreen = () => {
    setActiveScreenIndex((current) => Math.min(screens.length - 1, current + 1))
  }

  const selectHymn = async (result: HymnSearchResult) => {
    setIsLoadingHymn(true)
    const hymn = await getHymnById(result.id)
    setIsLoadingHymn(false)
    if (!hymn) return
    setSelectedHymn(hymn)
    setSelectedSectionIds(defaultSelectedSectionIds(hymn))
    setActiveScreenIndex(0)
    addRecentHymn(result.id)
    setRecentHymnIds(getRecentHymns())
  }

  const toggleSection = (sectionId: string) => {
    setSelectedSectionIds((current) => {
      if (current.includes(sectionId)) {
        const next = current.filter((id) => id !== sectionId)
        return next.length > 0 ? next : current
      }
      const order = selectedHymn?.sections.map((section) => section.id) ?? []
      return [...current, sectionId].sort((a, b) => order.indexOf(a) - order.indexOf(b))
    })
    setActiveScreenIndex(0)
  }

  const skipAllRefrains = () => {
    if (!selectedHymn) return
    const refrainIds = selectedHymn.sections
      .filter((section) => section.kind === "refrain")
      .map((section) => section.id)
    const nonRefrainIds = selectedSectionIds.filter((id) => !refrainIds.includes(id))
    if (nonRefrainIds.length > 0) {
      setSelectedSectionIds(nonRefrainIds)
      setActiveScreenIndex(0)
    }
  }

  const restoreAllSections = () => {
    if (!selectedHymn) return
    setSelectedSectionIds(defaultSelectedSectionIds(selectedHymn))
    setActiveScreenIndex(0)
  }

  const repeatSelectedRefrains = () => {
    if (!selectedHymn) return
    const sectionsById = new Map(selectedHymn.sections.map((section) => [section.id, section]))
    let didRepeat = false

    const repeatedIds = selectedSectionIds.flatMap((sectionId) => {
      const section = sectionsById.get(sectionId)
      if (section?.kind !== "refrain") return [sectionId]
      didRepeat = true
      return [sectionId, sectionId]
    })

    if (didRepeat) {
      setSelectedSectionIds(repeatedIds)
      setActiveScreenIndex(0)
    }
  }

  const handleToggleFavorite = () => {
    if (!selectedHymn) return
    toggleFavoriteHymn(selectedHymn.id)
    setFavoriteHymnIds(getFavoriteHymns())
  }

  const previewActiveScreen = () => {
    if (!activeScreen) return
    selectPreviewItem(createHymnPresentationItem(activeScreen))
  }

  const presentActiveScreen = () => {
    if (!activeScreen) return
    useHymnSlideStore.getState().setDeck(presentationDeck, activeScreenIndex)
    presentItem(createHymnPresentationItem(activeScreen))
  }

  const queueScreens = () => {
    const queue = useQueueStore.getState()
    const queueItems = createGroupedHymnQueueItems(screens)
    queue.addItems(queueItems)
    useHymnSlideStore.getState().setDeck(presentationDeck, activeScreenIndex)
  }

  const selectedHymnIsFavorite = selectedHymn ? favoriteHymnIds.includes(selectedHymn.id) : false

  return (
    <div
      ref={panelRef}
      data-slot="hymnal-panel"
      className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-border bg-card"
      tabIndex={-1}
    >
      <PanelHeader title="SDA Hymnal" icon={<ListMusicIcon className="size-3" />} step={5}>
        <Badge variant="outline" className="h-5 text-[0.5625rem] uppercase">
          {screens.length} screens
        </Badge>
      </PanelHeader>

      <div className="flex min-h-0 flex-1">
        <div className="flex w-[300px] shrink-0 flex-col border-r border-border">
          <div className="border-b border-border p-2">
            <div className="relative">
              <SearchIcon className="pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
              <Input
                value={query}
                onChange={(event) => {
                  setQuery(event.target.value)
                  if (event.target.value.trim()) {
                    setViewMode("search")
                  }
                }}
                placeholder="Search number or title"
                className="h-8 pl-7 text-xs"
              />
            </div>
            <div className="mt-1.5 flex gap-1">
              <button
                onClick={() => setViewMode("search")}
                className={cn(
                  "flex flex-1 items-center justify-center gap-1 rounded px-2 py-1 text-[0.625rem] transition-colors",
                  viewMode === "search"
                    ? "bg-primary text-primary-foreground"
                    : "hover:bg-muted/50 text-muted-foreground",
                )}
              >
                <SearchIcon className="size-2.5" />
                Search
              </button>
              <button
                onClick={() => {
                  setViewMode("recent")
                  setQuery("")
                }}
                className={cn(
                  "flex flex-1 items-center justify-center gap-1 rounded px-2 py-1 text-[0.625rem] transition-colors",
                  viewMode === "recent"
                    ? "bg-primary text-primary-foreground"
                    : "hover:bg-muted/50 text-muted-foreground",
                )}
              >
                <StarIcon className="size-2.5" />
                Recent
              </button>
              <button
                onClick={() => {
                  setViewMode("favorites")
                  setQuery("")
                }}
                className={cn(
                  "flex flex-1 items-center justify-center gap-1 rounded px-2 py-1 text-[0.625rem] transition-colors",
                  viewMode === "favorites"
                    ? "bg-primary text-primary-foreground"
                    : "hover:bg-muted/50 text-muted-foreground",
                )}
              >
                <HeartIcon className="size-2.5" />
                Favorites
              </button>
            </div>
          </div>

          <div className="min-h-0 flex-1 overflow-y-auto p-1.5">
            {results.map((result) => (
              <button
                key={result.id}
                onClick={() => selectHymn(result)}
                className={cn(
                  "flex w-full flex-col gap-0.5 rounded-md px-2 py-1.5 text-left transition-colors",
                  selectedHymn?.id === result.id
                    ? "bg-lime-500/15 text-foreground"
                    : "hover:bg-muted/50",
                )}
              >
                <span className="truncate text-xs font-semibold">
                  #{result.number} {result.title}
                </span>
                <span className="line-clamp-1 text-[0.68rem] text-muted-foreground">
                  {result.firstLine ?? result.category ?? "SDA Hymnal"}
                </span>
              </button>
            ))}
          </div>
        </div>

        <div className="grid min-w-0 flex-1 grid-cols-[minmax(220px,0.8fr)_minmax(280px,1fr)]">
          <div className="flex min-w-0 flex-col border-r border-border">
            {selectedHymn ? (
              <>
                <div className="border-b border-border px-3 py-2">
                  <div className="flex items-start justify-between gap-2">
                    <div className="min-w-0">
                      <p className="truncate text-sm font-semibold">
                        #{selectedHymn.number} {selectedHymn.title}
                      </p>
                      <p className="truncate text-xs text-muted-foreground">
                        {selectedHymn.category ?? "SDA Hymnal"}
                      </p>
                    </div>
                    <button
                      onClick={handleToggleFavorite}
                      className={cn(
                        "shrink-0 rounded p-1 transition-colors",
                        selectedHymnIsFavorite
                          ? "text-red-500 hover:text-red-600"
                          : "text-muted-foreground hover:text-foreground",
                      )}
                      title={selectedHymnIsFavorite ? "Remove from favorites" : "Add to favorites"}
                    >
                      <HeartIcon
                        className={cn("size-4", selectedHymnIsFavorite && "fill-current")}
                      />
                    </button>
                  </div>
                </div>

                <div className="min-h-0 flex-1 overflow-y-auto p-2">
                  <div className="flex flex-col gap-1">
                    {selectedHymn.sections.map((section) => (
                      <label
                        key={section.id}
                        className="flex cursor-pointer items-start gap-2 rounded-md px-2 py-1.5 hover:bg-muted/50"
                      >
                        <input
                          type="checkbox"
                          checked={selectedSectionIds.includes(section.id)}
                          onChange={() => toggleSection(section.id)}
                          className="mt-0.5"
                        />
                        <span className="min-w-0">
                          <span className="block text-xs font-medium">
                            {section.kind === "refrain" && section.afterVerseNumber !== undefined
                              ? `Refrain after Verse ${section.afterVerseNumber}`
                              : section.label}
                          </span>
                          <span className="line-clamp-2 text-[0.68rem] text-muted-foreground">
                            {section.lines.join(" ")}
                          </span>
                        </span>
                      </label>
                    ))}
                  </div>
                  <div className="mt-2 flex gap-1 border-t border-border pt-2">
                    <Button
                      size="xs"
                      variant="ghost"
                      className="flex-1 text-[0.625rem]"
                      onClick={skipAllRefrains}
                      disabled={
                        !selectedHymn.sections.some(
                          (s) => s.kind === "refrain" && selectedSectionIds.includes(s.id),
                        )
                      }
                    >
                      Skip refrains
                    </Button>
                    <Button
                      size="xs"
                      variant="ghost"
                      className="flex-1 text-[0.625rem]"
                      onClick={repeatSelectedRefrains}
                      disabled={
                        !selectedHymn.sections.some(
                          (s) => s.kind === "refrain" && selectedSectionIds.includes(s.id),
                        )
                      }
                    >
                      Repeat refrain
                    </Button>
                    <Button
                      size="xs"
                      variant="ghost"
                      className="flex-1 text-[0.625rem]"
                      onClick={restoreAllSections}
                      disabled={selectedSectionIds.length === selectedHymn.sections.length}
                    >
                      Restore all
                    </Button>
                  </div>
                </div>
              </>
            ) : (
              <PanelEmptyState
                icon={<BookOpenTextIcon className="size-8" />}
                title={isLoadingHymn ? "Loading hymn" : "No hymn selected"}
                description={
                  isLoadingHymn
                    ? "Fetching the selected hymn text."
                    : "Search for a hymn to preview and queue screens."
                }
              />
            )}
          </div>

          <div className="flex min-w-0 flex-col">
            <div className="grid min-h-10 grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-2 border-b border-border px-3 py-1.5">
              <div className="flex items-center gap-1">
                <Button
                  size="icon-xs"
                  variant="ghost"
                  disabled={!activeScreen || activeScreenIndex === 0}
                  onClick={goToPreviousScreen}
                  title="Previous screen"
                >
                  <ChevronLeftIcon className="size-3" />
                </Button>
                <Button
                  size="icon-xs"
                  variant="ghost"
                  disabled={!activeScreen || activeScreenIndex === screens.length - 1}
                  onClick={goToNextScreen}
                  title="Next screen"
                >
                  <ChevronRightIcon className="size-3" />
                </Button>
              </div>
              <div className="min-w-0">
                <p className="truncate text-sm font-medium">
                  {activeScreen
                    ? `${activeScreen.sectionLabel} ${activeScreen.screenIndex + 1}/${activeScreen.totalScreens}`
                    : "No screen"}
                </p>
                <p className="truncate text-xs text-muted-foreground">Preview or queue hymn screens.</p>
              </div>
              <div className="flex shrink-0 items-center gap-1">
                <Button size="xs" variant="outline" disabled={!activeScreen} onClick={previewActiveScreen}>
                  <SendIcon className="mr-1 size-3" />
                  Preview
                </Button>
                <Button size="xs" variant="outline" disabled={screens.length === 0} onClick={queueScreens}>
                  <PlusIcon className="mr-1 size-3" />
                  Queue
                </Button>
                <Button size="xs" disabled={!activeScreen} onClick={presentActiveScreen}>
                  <PlayIcon className="mr-1 size-3" />
                  Live
                </Button>
              </div>
            </div>

            <div className="min-h-0 flex-1 overflow-y-auto p-3">
              {activeScreen ? (
                <div className="flex min-h-full flex-col gap-3">
                  <div className="flex aspect-video items-center justify-center rounded-md border border-border bg-black p-8 text-center">
                    <div className="max-w-[80%] space-y-3 text-balance text-2xl font-semibold leading-snug text-white">
                      {activeScreen.lines.map((line) => (
                        <p key={line}>{line}</p>
                      ))}
                    </div>
                  </div>

                  <div className="grid grid-cols-2 gap-1.5 xl:grid-cols-3">
                    {screens.map((screen, index) => (
                      <button
                        key={screen.id}
                        onClick={() => setActiveScreenIndex(index)}
                        className={cn(
                          "rounded-md border px-2 py-1.5 text-left text-xs transition-colors",
                          index === activeScreenIndex
                            ? "border-lime-500/50 bg-lime-500/15"
                            : "border-border hover:bg-muted/50",
                        )}
                      >
                        <span className="block truncate font-medium">
                          {index + 1}. {screen.sectionLabel}
                        </span>
                        <span className="line-clamp-1 text-[0.68rem] text-muted-foreground">
                          {screen.lines.join(" ")}
                        </span>
                      </button>
                    ))}
                  </div>
                </div>
              ) : (
                <PanelEmptyState
                  icon={<BookOpenTextIcon className="size-8" />}
                  title="No hymn screens"
                  description="Select at least one hymn section to generate screens."
                />
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}
