import { expect, test } from "@playwright/test"

test.describe("STT disconnect and late transcript events", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/?e2e=1", { waitUntil: "domcontentloaded" })
    await page.waitForFunction(() => Boolean(window.__SABBATHCUE_OPERATOR_E2E__), null, {
      timeout: 15_000,
    })
  })

  test("provider disconnect drops late partials and finals", async ({ page }) => {
    await page.evaluate(() => {
      const harness = window.__SABBATHCUE_OPERATOR_E2E__!
      harness.transcription.clearTimeline()
      harness.transcription.connect()
      harness.transcription.partial("For God so loved the world")
    })

    await expect
      .poll(async () =>
        page.evaluate(() => window.__SABBATHCUE_OPERATOR_E2E__!.snapshot().transcriptPartial)
      )
      .toBe("For God so loved the world")

    await page.evaluate(() => {
      const harness = window.__SABBATHCUE_OPERATOR_E2E__!
      harness.transcription.disconnect()
      harness.transcription.partial("this late partial must not resurrect the line")
      harness.transcription.final("this late final must not become a segment")
    })

    const snapshot = await page.evaluate(() => window.__SABBATHCUE_OPERATOR_E2E__!.snapshot())
    expect(snapshot.connectionStatus).toBe("disconnected")
    expect(snapshot.transcriptPartial).toBe("For God so loved the world")
    expect(snapshot.lastTranscriptFinal).not.toBe("this late final must not become a segment")
  })

  test("reconnect accepts a new utterance after the previous provider dropped", async ({
    page,
  }) => {
    await page.evaluate(() => {
      const harness = window.__SABBATHCUE_OPERATOR_E2E__!
      harness.transcription.connect()
      harness.transcription.partial("stale in-flight text")
      harness.transcription.disconnect()
      harness.transcription.connect()
      harness.transcription.partial("The Lord is my shepherd")
      harness.transcription.final("The Lord is my shepherd I shall not want")
    })

    await expect
      .poll(async () =>
        page.evaluate(() => {
          const snapshot = window.__SABBATHCUE_OPERATOR_E2E__!.snapshot()
          return {
            connectionStatus: snapshot.connectionStatus,
            transcriptPartial: snapshot.transcriptPartial,
            lastTranscriptFinal: snapshot.lastTranscriptFinal,
          }
        })
      )
      .toEqual(
        expect.objectContaining({
          connectionStatus: "connected",
          transcriptPartial: "",
          lastTranscriptFinal: "The Lord is my shepherd I shall not want",
        })
      )
  })
})
