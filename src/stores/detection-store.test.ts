import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import {
  buildDetectionContextStack,
  buildHeldReferenceCandidates,
  useDetectionStore,
} from "./detection-store"
import { useSettingsStore } from "./settings-store"
import type { DetectionResult } from "@/types"

function makeDetection(
  overrides: Partial<DetectionResult> = {}
): DetectionResult {
  return {
    verse_ref: "John 3:16",
    verse_text: "For God so loved the world",
    book_name: "John",
    book_number: 43,
    chapter: 3,
    verse: 16,
    confidence: 0.96,
    source: "direct",
    auto_queued: true,
    transcript_snippet: "John three sixteen",
    is_chapter_only: false,
    ...overrides,
  }
}

describe("detection store", () => {
  let now = new Date("2026-05-19T00:00:00Z").getTime()

  beforeEach(() => {
    now = new Date("2026-05-19T00:00:00Z").getTime()
    vi.spyOn(Date, "now").mockImplementation(() => now)
    useDetectionStore.setState({
      detections: [],
    })
    useSettingsStore.setState({
      semanticDetectionEnabled: true,
      semanticConfidenceThreshold: 0.7,
    })
  })

  afterEach(() => {
    useDetectionStore.getState().clearDetections()
    vi.restoreAllMocks()
    vi.useRealTimers()
  })

  it("higher-confidence detection stays above a newer weaker detection", () => {
    const store = useDetectionStore.getState()

    // Add older high-confidence detection
    store.addDetection(
      makeDetection({ verse_ref: "Romans 8:1", confidence: 0.99 })
    )

    // Advance time
    now = new Date("2026-05-19T00:00:01Z").getTime()

    // Add newer lower-confidence detection
    store.addDetection(
      makeDetection({ verse_ref: "John 3:16", confidence: 0.85 })
    )

    const detections = useDetectionStore.getState().detections
    expect(detections[0].verse_ref).toBe("Romans 8:1")
    expect(detections[1].verse_ref).toBe("John 3:16")
  })

  it("direct hit outranks stronger semantic hit", () => {
    const store = useDetectionStore.getState()

    store.addDetection(
      makeDetection({
        verse_ref: "Strong Semantic",
        confidence: 0.91,
        source: "semantic",
      })
    )

    now = new Date("2026-05-19T00:00:01Z").getTime()
    store.addDetection(
      makeDetection({
        verse_ref: "Weak Direct",
        confidence: 0.84,
        source: "direct",
      })
    )

    const detections = useDetectionStore.getState().detections
    expect(detections[0].verse_ref).toBe("Weak Direct")
    expect(detections[1].verse_ref).toBe("Strong Semantic")
  })

  it("keeps a freshly spoken EGW detection even when the box is full of higher-confidence Bible hits", () => {
    const store = useDetectionStore.getState()

    // Box fills with five 100% Bible hits during a session.
    for (let i = 1; i <= 5; i += 1) {
      store.addDetection(
        makeDetection({
          verse_ref: `John ${i}:1`,
          book_number: 43,
          chapter: i,
          verse: 1,
          confidence: 1,
        })
      )
    }

    now += 1000
    // Then an Ellen White reference is spoken (lower confidence).
    store.addDetection(
      makeDetection({
        content_type: "egw",
        verse_ref: "Steps to Christ p.9 par.2",
        verse_text: "Nature and revelation testify of God's love.",
        book_name: "Steps to Christ",
        book_number: 2,
        chapter: 9,
        verse: 2,
        confidence: 0.94,
        egw_paragraph: {
          id: 1,
          book_number: 2,
          book_title: "Steps to Christ",
          chapter: 1,
          chapter_title: "God's Love for Man",
          paragraph: 2,
          page: 9,
          page_paragraph: 2,
          text: "Nature and revelation testify of God's love.",
        },
      })
    )

    const detections = useDetectionStore.getState().detections
    expect(detections.some((d) => d.content_type === "egw")).toBe(true)
  })

  it("dedupes hymn detections by hymn number and keeps verses distinct", () => {
    const store = useDetectionStore.getState()

    const hymn46 = (): DetectionResult =>
      makeDetection({
        content_type: "hymn",
        verse_ref: "Hymn 46",
        verse_text: "Holy, Holy, Holy",
        book_name: "Hymn",
        book_number: 0,
        chapter: 0,
        verse: 46,
        hymn: { number: 46, id: "hymn-46", title: "Holy, Holy, Holy" },
      })

    store.addDetection(hymn46())
    store.addDetection(hymn46())
    store.addDetection(makeDetection({ verse_ref: "John 3:16" }))

    const detections = useDetectionStore.getState().detections
    const hymns = detections.filter((d) => d.content_type === "hymn")
    expect(hymns).toHaveLength(1)
    expect(detections.filter((d) => d.verse_ref === "John 3:16")).toHaveLength(
      1
    )
  })

  it("near-tied direct hit wins over semantic", () => {
    const store = useDetectionStore.getState()

    store.addDetection(
      makeDetection({
        verse_ref: "Near Semantic",
        confidence: 0.9,
        source: "semantic",
      })
    )

    now = new Date("2026-05-19T00:00:01Z").getTime()
    store.addDetection(
      makeDetection({
        verse_ref: "Near Direct",
        confidence: 0.87,
        source: "direct",
      })
    )

    const detections = useDetectionStore.getState().detections
    expect(detections[0].verse_ref).toBe("Near Direct")
    expect(detections[1].verse_ref).toBe("Near Semantic")
  })

  it("duplicate verse refreshes recency and keeps best confidence", () => {
    const store = useDetectionStore.getState()

    // Add first detection with lower confidence
    store.addDetection(
      makeDetection({ verse_ref: "John 3:16", confidence: 0.85 })
    )

    // Add duplicate with higher confidence
    now = new Date("2026-05-19T00:00:01Z").getTime()
    store.addDetection(
      makeDetection({ verse_ref: "John 3:16", confidence: 0.96 })
    )

    const detections = useDetectionStore.getState().detections
    expect(detections).toHaveLength(1)
    expect(detections[0].confidence).toBe(0.96)
  })

  it("duplicate verse preserves text when new detection has empty text", () => {
    const store = useDetectionStore.getState()

    // Add first detection with text
    store.addDetection(
      makeDetection({
        verse_ref: "John 3:16",
        verse_text: "For God so loved the world",
      })
    )

    // Add duplicate with empty text
    now = new Date("2026-05-19T00:00:01Z").getTime()
    store.addDetection(
      makeDetection({
        verse_ref: "John 3:16",
        verse_text: "",
        confidence: 0.97,
      })
    )

    const detections = useDetectionStore.getState().detections
    expect(detections).toHaveLength(1)
    expect(detections[0].verse_text).toBe("For God so loved the world")
    expect(detections[0].confidence).toBe(0.97)
  })

  it("sorts by relevance score before recency", () => {
    const store = useDetectionStore.getState()

    store.addDetection(makeDetection({ verse_ref: "A", confidence: 0.9 }))

    now = new Date("2026-05-19T00:00:01Z").getTime()
    store.addDetection(makeDetection({ verse_ref: "B", confidence: 0.8 }))

    now = new Date("2026-05-19T00:00:01Z").getTime()
    store.addDetection(makeDetection({ verse_ref: "C", confidence: 0.85 }))

    const detections = useDetectionStore.getState().detections
    expect(detections[0].verse_ref).toBe("A")
    expect(detections[1].verse_ref).toBe("C")
    expect(detections[2].verse_ref).toBe("B")
  })

  it("duplicate semantic hit cannot replace a direct source label", () => {
    const store = useDetectionStore.getState()

    store.addDetection(
      makeDetection({
        verse_ref: "John 3:16",
        confidence: 0.9,
        source: "direct",
      })
    )

    now = new Date("2026-05-19T00:00:01Z").getTime()
    store.addDetection(
      makeDetection({
        verse_ref: "John 3:16",
        confidence: 0.95,
        source: "semantic",
      })
    )

    const detections = useDetectionStore.getState().detections
    expect(detections).toHaveLength(1)
    expect(detections[0].source).toBe("direct")
    expect(detections[0].confidence).toBe(0.95)
  })

  it("unresolved zero sentinels do not overwrite resolved verse coordinates", () => {
    const store = useDetectionStore.getState()

    store.addDetection(
      makeDetection({
        verse_ref: "John 3:16",
        book_number: 43,
        chapter: 3,
        verse: 16,
        source: "direct",
        confidence: 0.9,
      })
    )

    now = new Date("2026-05-19T00:00:01Z").getTime()
    store.addDetection(
      makeDetection({
        verse_ref: "John 3:16",
        book_number: 0,
        chapter: 0,
        verse: 0,
        book_name: "",
        verse_text: "",
        source: "semantic",
        confidence: 0.85,
      })
    )

    const detections = useDetectionStore.getState().detections
    expect(detections).toHaveLength(1)
    expect(detections[0].book_number).toBe(43)
    expect(detections[0].chapter).toBe(3)
    expect(detections[0].verse).toBe(16)
    expect(detections[0].source).toBe("direct")
    expect(detections[0].confidence).toBe(0.9)
  })

  it("unresolved zero sentinels do not overwrite resolved coords through batch addDetections path", () => {
    // The live transcription ingestion path calls addDetections, not addDetection.
    // addDetections has its own merge control-flow that must not let an
    // unresolved semantic hit (book_number=0) clobber an already-resolved
    // detection, even when the incoming confidence is higher.
    const store = useDetectionStore.getState()

    store.addDetection(
      makeDetection({
        verse_ref: "John 3:16",
        book_number: 43,
        chapter: 3,
        verse: 16,
        book_name: "Jn",
        verse_text: "old text",
        source: "semantic",
        confidence: 0.85,
      })
    )

    now = new Date("2026-05-19T00:00:01Z").getTime()

    // Batch ingestion: incoming has higher confidence and fresher text
    // but unresolved coordinates.
    store.addDetections([
      makeDetection({
        verse_ref: "John 3:16",
        book_number: 0,
        chapter: 0,
        verse: 0,
        book_name: "John",
        verse_text: "For God so loved the world",
        source: "semantic",
        confidence: 0.92,
      }),
    ])

    const detections = useDetectionStore.getState().detections
    expect(detections).toHaveLength(1)
    // Resolved coordinates survive (zero sentinels fall through)
    expect(detections[0].book_number).toBe(43)
    expect(detections[0].chapter).toBe(3)
    expect(detections[0].verse).toBe(16)
    // Fresher incoming text wins when non-empty
    expect(detections[0].book_name).toBe("John")
    expect(detections[0].verse_text).toBe("For God so loved the world")
    // Confidence is max of both
    expect(detections[0].confidence).toBe(0.92)
  })

  it("keeps at most 5 detections", () => {
    const store = useDetectionStore.getState()

    for (let i = 0; i < 9; i += 1) {
      now = new Date("2026-05-19T00:00:00Z").getTime() + i
      store.addDetection(
        makeDetection({ verse_ref: `Ref ${i}`, confidence: 0.8 })
      )
    }

    const detections = useDetectionStore.getState().detections
    expect(detections).toHaveLength(5)
    expect(detections[0].verse_ref).toBe("Ref 8")
    expect(detections.some((d) => d.verse_ref === "Ref 0")).toBe(false)
  })

  it("keeps recent detections until manually cleared and refreshes duplicates", () => {
    vi.restoreAllMocks()
    vi.useFakeTimers()
    vi.setSystemTime(new Date("2026-05-19T00:00:00Z"))
    useDetectionStore.getState().clearDetections()

    const store = useDetectionStore.getState()
    store.addDetections([
      makeDetection({ verse_ref: "John 3:16", confidence: 0.85 }),
    ])

    vi.advanceTimersByTime(60_000)
    expect(useDetectionStore.getState().detections).toHaveLength(1)

    store.addDetections([
      makeDetection({ verse_ref: "John 3:16", confidence: 0.96 }),
    ])
    vi.advanceTimersByTime(60_000)
    expect(useDetectionStore.getState().detections).toHaveLength(1)
    expect(useDetectionStore.getState().detections[0].confidence).toBe(0.96)

    store.clearDetections()
    expect(useDetectionStore.getState().detections).toHaveLength(0)
  })

  it("preserves resolved coordinates when stale state merges with incoming semantic hit", () => {
    const store = useDetectionStore.getState()
    const nowMs = Date.now()

    useDetectionStore.setState({
      detections: [
        {
          ...makeDetection({
            verse_ref: "John 3:16",
            source: "semantic",
            confidence: 0.7,
            book_number: 43,
            chapter: 3,
            verse: 16,
            verse_text: "Resolved verse text",
          }),
          received_at: nowMs - 60_000,
        },
      ],
    })

    store.addDetections([
      makeDetection({
        verse_ref: "John 3:16",
        source: "semantic",
        confidence: 0.8,
        book_number: 0,
        chapter: 0,
        verse: 0,
        verse_text: "Incoming semantic text",
      }),
    ])

    const merged = useDetectionStore.getState().detections[0]
    expect(merged.source).toBe("semantic")
    expect(merged.confidence).toBe(0.8)
    expect(merged.book_number).toBe(43)
    expect(merged.chapter).toBe(3)
    expect(merged.verse).toBe(16)
    expect(merged.verse_text).toBe("Incoming semantic text")
  })

  it("chapter-only placeholder yields to an explicit verse of the same chapter", () => {
    const store = useDetectionStore.getState()

    // "Revelation chapter 20" surfaces a chapter-only placeholder (verse 1).
    store.addDetections([
      makeDetection({
        verse_ref: "Revelation 20:1",
        book_name: "Revelation",
        book_number: 66,
        chapter: 20,
        verse: 1,
        confidence: 0.88,
        is_chapter_only: true,
      }),
    ])
    expect(useDetectionStore.getState().detections).toHaveLength(1)

    // The verse actually being read ("verse 12") then arrives; the placeholder
    // must stop competing so only the explicit verse remains.
    store.addDetections([
      makeDetection({
        verse_ref: "Revelation 20:12",
        book_name: "Revelation",
        book_number: 66,
        chapter: 20,
        verse: 12,
        confidence: 1,
        is_chapter_only: false,
      }),
    ])

    const refs = useDetectionStore.getState().detections.map((d) => d.verse_ref)
    expect(refs).toEqual(["Revelation 20:12"])
  })

  it("chapter-only placeholder yields when an explicit verse is added singly", () => {
    const store = useDetectionStore.getState()

    store.addDetection(
      makeDetection({
        verse_ref: "Revelation 20:1",
        book_name: "Revelation",
        book_number: 66,
        chapter: 20,
        verse: 1,
        confidence: 0.88,
        is_chapter_only: true,
      })
    )
    store.addDetection(
      makeDetection({
        verse_ref: "Revelation 20:12",
        book_name: "Revelation",
        book_number: 66,
        chapter: 20,
        verse: 12,
        confidence: 1,
        is_chapter_only: false,
      })
    )

    const refs = useDetectionStore.getState().detections.map((d) => d.verse_ref)
    expect(refs).toEqual(["Revelation 20:12"])
  })

  it("keeps suppressing a re-emitted chapter-only placeholder while the explicit verse stands", () => {
    const store = useDetectionStore.getState()

    store.addDetections([
      makeDetection({
        verse_ref: "Revelation 20:12",
        book_name: "Revelation",
        book_number: 66,
        chapter: 20,
        verse: 12,
        is_chapter_only: false,
      }),
    ])
    // The live detector keeps re-emitting "Revelation chapter 20" as a placeholder.
    store.addDetections([
      makeDetection({
        verse_ref: "Revelation 20:1",
        book_name: "Revelation",
        book_number: 66,
        chapter: 20,
        verse: 1,
        confidence: 0.88,
        is_chapter_only: true,
      }),
    ])

    const refs = useDetectionStore.getState().detections.map((d) => d.verse_ref)
    expect(refs).toEqual(["Revelation 20:12"])
  })

  it("does not supersede a chapter-only placeholder for a different chapter", () => {
    const store = useDetectionStore.getState()

    store.addDetections([
      makeDetection({
        verse_ref: "Genesis 3:1",
        book_name: "Genesis",
        book_number: 1,
        chapter: 3,
        verse: 1,
        confidence: 0.88,
        is_chapter_only: true,
      }),
      makeDetection({
        verse_ref: "Revelation 20:12",
        book_name: "Revelation",
        book_number: 66,
        chapter: 20,
        verse: 12,
        is_chapter_only: false,
      }),
    ])

    const refs = useDetectionStore.getState().detections.map((d) => d.verse_ref)
    expect(refs).toContain("Genesis 3:1")
    expect(refs).toContain("Revelation 20:12")
  })

  it("deduplicates the same resolved verse even when the label changes", () => {
    const store = useDetectionStore.getState()

    store.addDetections([
      makeDetection({
        verse_ref: "John 3:16",
        source: "direct",
        confidence: 0.9,
      }),
      makeDetection({
        verse_ref: "Jn 3:16",
        source: "semantic",
        confidence: 0.95,
      }),
    ])

    const detections = useDetectionStore.getState().detections
    expect(detections).toHaveLength(1)
    expect(detections[0].source).toBe("direct")
    expect(detections[0].confidence).toBe(0.95)
  })

  it("keeps batch duplicate merges aligned with single duplicate merges", () => {
    const store = useDetectionStore.getState()
    const existing = makeDetection({
      verse_ref: "John 3:16",
      source: "semantic",
      confidence: 0.7,
      verse_text: "Older text",
      transcript_snippet: "older snippet",
    })
    const incoming = makeDetection({
      verse_ref: "John 3:16",
      source: "semantic",
      confidence: 0.8,
      verse_text: "Newer text",
      transcript_snippet: "newer snippet",
    })

    store.addDetection(existing)
    store.addDetection(incoming)
    const singleMerged = useDetectionStore.getState().detections[0]

    store.clearDetections()
    store.addDetection(existing)
    store.addDetections([incoming])
    const batchMerged = useDetectionStore.getState().detections[0]

    expect(batchMerged.verse_text).toBe(singleMerged.verse_text)
    expect(batchMerged.transcript_snippet).toBe(singleMerged.transcript_snippet)
    expect(batchMerged.confidence).toBe(singleMerged.confidence)
  })

  it("does not treat chapter number substrings as matching chapter-only refs", () => {
    const store = useDetectionStore.getState()

    store.addDetections([
      makeDetection({
        verse_ref: "John 13",
        chapter: 13,
        verse: 1,
        is_chapter_only: true,
      }),
      makeDetection({
        verse_ref: "John 3",
        chapter: 3,
        verse: 1,
        is_chapter_only: true,
      }),
    ])

    const detections = useDetectionStore.getState().detections
    expect(detections).toHaveLength(2)
    expect(detections.map((d) => d.verse_ref).sort()).toEqual([
      "John 13",
      "John 3",
    ])
  })

  it("deduplicates EGW detections by book page and page paragraph", () => {
    const store = useDetectionStore.getState()
    const egwDetection = makeDetection({
      content_type: "egw",
      verse_ref: "Patriarchs and Prophets p.29 par.2",
      book_name: "Patriarchs and Prophets",
      book_number: 1,
      chapter: 29,
      verse: 2,
      verse_text: "The history of the great conflict.",
      egw_paragraph: {
        id: 12,
        book_number: 1,
        book_title: "Patriarchs and Prophets",
        chapter: 1,
        chapter_title: "Why Was Sin Permitted?",
        paragraph: 2,
        page: 29,
        page_paragraph: 2,
        text: "The history of the great conflict.",
      },
    })

    store.addDetection(egwDetection)
    now = new Date("2026-05-19T00:00:01Z").getTime()
    store.addDetection({
      ...egwDetection,
      verse_ref: "Patriarchs and Prophets page 29 paragraph 2",
      confidence: 0.99,
    })

    const detections = useDetectionStore.getState().detections
    expect(detections).toHaveLength(1)
    expect(detections[0].content_type).toBe("egw")
    expect(detections[0].confidence).toBe(0.99)
  })

  function makeEgwSemantic(
    overrides: Partial<DetectionResult> = {}
  ): DetectionResult {
    return makeDetection({
      content_type: "egw",
      source: "semantic",
      verse_ref: "The Desire of Ages p.480 par.1",
      book_name: "The Desire of Ages",
      book_number: 2,
      chapter: 480,
      verse: 1,
      confidence: 0.9,
      auto_queued: false,
      ...overrides,
    })
  }

  it("never lets an EGW hit displace a verse from the recent box", () => {
    const store = useDetectionStore.getState()
    for (let index = 0; index < 5; index += 1) {
      now = new Date("2026-05-19T00:00:00Z").getTime() + index * 1000
      store.addDetection(
        makeDetection({
          verse_ref: `John 3:${index + 1}`,
          chapter: 3,
          verse: index + 1,
        })
      )
    }

    now = new Date("2026-05-19T00:01:00Z").getTime()
    store.addDetection(makeEgwSemantic())

    const detections = useDetectionStore.getState().detections
    expect(detections).toHaveLength(5)
    expect(detections.some((d) => d.content_type === "egw")).toBe(false)
  })

  it("keeps at most one EGW card and sorts it after every verse", () => {
    const store = useDetectionStore.getState()
    store.addDetection(makeDetection({ verse_ref: "John 3:16" }))

    now = new Date("2026-05-19T00:00:01Z").getTime()
    store.addDetection(makeEgwSemantic())
    now = new Date("2026-05-19T00:00:02Z").getTime()
    store.addDetection(
      makeEgwSemantic({
        verse_ref: "The Desire of Ages p.481 par.2",
        chapter: 481,
        verse: 2,
        confidence: 0.92,
      })
    )

    const detections = useDetectionStore.getState().detections
    const egw = detections.filter((d) => d.content_type === "egw")
    expect(egw).toHaveLength(1)
    expect(egw[0].chapter).toBe(481)
    expect(detections[detections.length - 1].content_type).toBe("egw")
  })

  it("holds EGW to its own floor when the Bible slider is lowered", () => {
    useSettingsStore.setState({
      semanticDetectionEnabled: true,
      semanticConfidenceThreshold: 0.7,
    })
    const store = useDetectionStore.getState()

    store.addDetection(makeEgwSemantic({ confidence: 0.72 }))
    expect(useDetectionStore.getState().detections).toHaveLength(0)

    now = new Date("2026-05-19T00:00:01Z").getTime()
    store.addDetection(
      makeDetection({
        source: "semantic",
        confidence: 0.72,
        verse_ref: "John 3:17",
        verse: 17,
      })
    )
    expect(useDetectionStore.getState().detections).toHaveLength(1)
  })

  it("hides semantic hits below the app threshold but keeps direct and EGW below the floor", () => {
    const store = useDetectionStore.getState()

    store.addDetection(
      makeDetection({
        verse_ref: "Job 23:2",
        source: "semantic",
        confidence: 0.68,
      })
    )
    store.addDetection(
      makeDetection({
        verse_ref: "Matthew 7:2",
        source: "direct",
        confidence: 0.65,
      })
    )
    store.addDetection(
      makeDetection({
        content_type: "egw",
        verse_ref: "Steps to Christ p.9 par.2",
        source: "direct",
        confidence: 0.6,
      })
    )

    const refs = useDetectionStore.getState().detections.map((d) => d.verse_ref)
    expect(refs).not.toContain("Job 23:2")
    expect(refs).toContain("Matthew 7:2")
    expect(refs).toContain("Steps to Christ p.9 par.2")
  })

  it("keeps semantic hits at exactly the app threshold", () => {
    const store = useDetectionStore.getState()

    store.addDetection(
      makeDetection({
        verse_ref: "John 3:16",
        source: "semantic",
        confidence: 0.7,
      })
    )

    expect(useDetectionStore.getState().detections).toHaveLength(1)
  })

  it("drops semantic noise below the app threshold from a batch without evicting real hits", () => {
    const store = useDetectionStore.getState()

    store.addDetections([
      makeDetection({
        verse_ref: "Mark 15:4",
        source: "semantic",
        confidence: 0.68,
      }),
      makeDetection({
        verse_ref: "Daniel 7:9",
        source: "direct",
        confidence: 1,
      }),
    ])

    const refs = useDetectionStore.getState().detections.map((d) => d.verse_ref)
    expect(refs).toEqual(["Daniel 7:9"])
  })

  it("hides all semantic detections when semantic detection is disabled", () => {
    useSettingsStore.setState({ semanticDetectionEnabled: false })
    const store = useDetectionStore.getState()

    store.addDetections([
      makeDetection({
        verse_ref: "Romans 8:28",
        source: "semantic",
        confidence: 1,
      }),
      makeDetection({
        verse_ref: "Daniel 7:9",
        source: "direct",
        confidence: 0.8,
      }),
    ])

    const refs = useDetectionStore.getState().detections.map((d) => d.verse_ref)
    expect(refs).toEqual(["Daniel 7:9"])
  })

  it("drops existing semantic detections after semantic detection is disabled and a new batch arrives", () => {
    const store = useDetectionStore.getState()

    store.addDetections([
      makeDetection({
        verse_ref: "Romans 8:28",
        source: "semantic",
        confidence: 1,
      }),
    ])
    expect(
      useDetectionStore.getState().detections.map((d) => d.verse_ref)
    ).toEqual(["Romans 8:28"])

    useSettingsStore.setState({ semanticDetectionEnabled: false })
    now = new Date("2026-05-19T00:00:01Z").getTime()

    store.addDetections([
      makeDetection({
        verse_ref: "Daniel 7:9",
        source: "direct",
        confidence: 0.8,
      }),
    ])

    const refs = useDetectionStore.getState().detections.map((d) => d.verse_ref)
    expect(refs).toEqual(["Daniel 7:9"])
  })

  it("evictStale removes detections older than the TTL and keeps fresh ones", () => {
    const store = useDetectionStore.getState()

    store.addDetection(makeDetection({ verse_ref: "Old 1:1" }))

    now = new Date("2026-05-19T00:02:00Z").getTime()
    store.addDetection(makeDetection({ verse_ref: "Fresh 2:2" }))

    store.evictStale(now)

    const refs = useDetectionStore.getState().detections.map((d) => d.verse_ref)
    expect(refs).toContain("Fresh 2:2")
    expect(refs).not.toContain("Old 1:1")
  })

  it("evictStale keeps detections for 11 seconds", () => {
    const store = useDetectionStore.getState()

    store.addDetection(makeDetection({ verse_ref: "John 3:16" }))

    store.evictStale(now + 10_999)
    expect(useDetectionStore.getState().detections).toHaveLength(1)

    store.evictStale(now + 11_000)
    expect(useDetectionStore.getState().detections).toHaveLength(0)
  })

  it("evictStale is a no-op when nothing has expired", () => {
    const store = useDetectionStore.getState()
    store.addDetection(makeDetection({ verse_ref: "John 3:16" }))

    const before = useDetectionStore.getState().detections
    store.evictStale(now + 1_000)
    const after = useDetectionStore.getState().detections

    expect(after).toBe(before)
    expect(after).toHaveLength(1)
  })

  it("builds a deduplicated passage context stack from recent Bible detections", () => {
    const contextStack = buildDetectionContextStack([
      {
        ...makeDetection({
          verse_ref: "John 12:32",
          chapter: 12,
          verse: 32,
        }),
        received_at: now + 2,
      },
      {
        ...makeDetection({
          verse_ref: "John 3:16",
          chapter: 3,
          verse: 16,
        }),
        received_at: now + 1,
      },
      {
        ...makeDetection({
          verse_ref: "John 3:17",
          chapter: 3,
          verse: 17,
        }),
        received_at: now,
      },
      makeDetection({
        content_type: "hymn",
        verse_ref: "Hymn 46",
        book_number: 0,
        chapter: 0,
        verse: 46,
      }),
    ])

    expect(contextStack.map((entry) => entry.reference)).toEqual([
      "John 12",
      "John 3",
    ])
  })

  it("holds chapter-only and below-threshold Bible detections for review", () => {
    const held = buildHeldReferenceCandidates(
      [
        makeDetection({
          verse_ref: "John 3",
          verse: 1,
          confidence: 1,
          is_chapter_only: true,
        }),
        makeDetection({
          verse_ref: "Romans 4:22",
          book_name: "Romans",
          book_number: 45,
          chapter: 4,
          verse: 22,
          confidence: 0.84,
        }),
        makeDetection({
          verse_ref: "Revelation 14:7",
          book_name: "Revelation",
          book_number: 66,
          chapter: 14,
          verse: 7,
          confidence: 0.98,
        }),
      ],
      0.85,
      0.7
    )

    expect(held.map((candidate) => candidate.detection.verse_ref)).toEqual([
      "John 3",
      "Romans 4:22",
    ])
  })
})
