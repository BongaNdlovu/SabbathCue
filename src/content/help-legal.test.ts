import { describe, expect, it } from "vitest"
import {
  COPYRIGHT_SECTIONS,
  HELP_GUIDE_SECTIONS,
  HELP_LEGAL_AGREEMENT_NOTICE,
  HELP_LEGAL_CLOSING_MESSAGE,
  HELP_LEGAL_COPYRIGHT_HOLDER,
  HELP_LEGAL_CREATOR,
  TERMS_SECTIONS,
} from "./help-legal"
import { APP_DISPLAY_NAME } from "@/lib/app-brand"

describe("help-legal content", () => {
  it("names the creator and copyright holder", () => {
    expect(HELP_LEGAL_CREATOR).toBe("Fanelesibonge Ndlovu")
    expect(HELP_LEGAL_COPYRIGHT_HOLDER).toBe(HELP_LEGAL_CREATOR)
  })

  it("states user agreement on download and use", () => {
    expect(HELP_LEGAL_AGREEMENT_NOTICE).toContain("download")
    expect(HELP_LEGAL_AGREEMENT_NOTICE).toContain("install")
    expect(HELP_LEGAL_AGREEMENT_NOTICE).toContain(APP_DISPLAY_NAME)
    expect(HELP_LEGAL_AGREEMENT_NOTICE.toLowerCase()).toContain("terms")
  })

  it("includes help guide, terms, and copyright sections", () => {
    expect(HELP_GUIDE_SECTIONS.length).toBeGreaterThanOrEqual(4)
    expect(TERMS_SECTIONS.length).toBeGreaterThanOrEqual(10)
    expect(COPYRIGHT_SECTIONS.length).toBeGreaterThanOrEqual(4)
  })

  it("keeps the help guide current for transcription and auto detection", () => {
    const guideText = HELP_GUIDE_SECTIONS.flatMap((section) => [
      section.title,
      ...section.items,
    ]).join(" ")

    expect(guideText).toContain("Vosk")
    expect(guideText).toContain("Deepgram")
    expect(guideText).toContain("Soniox")
    expect(guideText).toContain("Ellen White")
    expect(guideText).toContain("85%")
    expect(guideText).toContain("Semantic detection")
    expect(guideText).toContain("14 seconds")
    expect(guideText).toContain("Don't")
  })

  it("covers legal protections in terms", () => {
    const titles = TERMS_SECTIONS.map((section) => section.title).join(" ")
    expect(titles).toContain("Intellectual property")
    expect(titles).toContain("Disclaimer of warranties")
    expect(titles).toContain("Limitation of liability")
  })

  it("ends with a closing message and bible verse", () => {
    expect(HELP_LEGAL_CLOSING_MESSAGE.greeting).toContain("Thank you")
    expect(HELP_LEGAL_CLOSING_MESSAGE.verse.reference).toContain("Colossians")
    expect(HELP_LEGAL_CLOSING_MESSAGE.signOff).toContain(HELP_LEGAL_CREATOR)
  })
})
