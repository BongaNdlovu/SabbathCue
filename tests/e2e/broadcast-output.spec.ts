import { expect, test } from "@playwright/test"

const theme = {
  builtin: true,
  pinned: false,
  createdAt: 0,
  updatedAt: 0,
  id: "e2e-preview-match",
  name: "E2E Preview Match",
  resolution: { width: 1280, height: 720 },
  background: {
    type: "solid",
    color: "#111827",
    gradient: null,
    image: null,
  },
  textBox: {
    enabled: false,
    color: "#000000",
    opacity: 0,
    borderRadius: 0,
    padding: 0,
  },
  verseText: {
    fontFamily: "serif",
    fontSize: 56,
    fontWeight: 400,
    color: "#ffffff",
    horizontalAlign: "center",
    verticalAlign: "top",
    textTransform: "none",
    textDecoration: "none",
    lineHeight: 1.35,
    letterSpacing: 0,
    shadow: null,
    outline: null,
  },
  verseNumbers: {
    visible: true,
    fontSize: 18,
    color: "#fbbf24",
    superscript: true,
  },
  reference: {
    fontFamily: "sans-serif",
    fontSize: 36,
    fontWeight: 600,
    color: "#fbbf24",
    horizontalAlign: "center",
    verticalAlign: "top",
    textTransform: "none",
    textDecoration: "none",
    uppercase: false,
    letterSpacing: 0,
    position: "above",
  },
  layout: {
    anchor: "center",
    offsetX: 0,
    offsetY: 0,
    padding: { top: 48, right: 72, bottom: 48, left: 72 },
    textAlign: "center",
    backgroundWidth: 100,
    backgroundHeight: 100,
    textAreaWidth: 84,
    textAreaHeight: 76,
    referenceGap: 24,
  },
  transition: {
    type: "fade",
    duration: 300,
    easing: "ease-in-out",
    direction: "up",
  },
}

const verse = {
  reference: "John 3:16 (KJV)",
  segments: [{ verseNumber: 16, text: "For God so loved the world." }],
}

test("broadcast output paints the same payload committed from preview", async ({ page }) => {
  await page.goto("/broadcast-output.html?output=main")

  const canvas = page.locator("canvas")
  await expect(canvas).toBeVisible()

  await page.waitForFunction(() => Boolean(window.__SABBATHCUE_BROADCAST_TEST__))
  await page.evaluate(
    ({ theme, verse }) => {
      window.__SABBATHCUE_BROADCAST_TEST__?.render({ theme, verse })
    },
    { theme, verse },
  )

  await expect
    .poll(async () =>
      page.evaluate(() => {
        const canvas = document.querySelector("canvas")
        const ctx = canvas?.getContext("2d")
        if (!canvas || !ctx) return 0

        const imageData = ctx.getImageData(0, 0, canvas.width, canvas.height).data
        let nonBackgroundPixels = 0
        for (let i = 0; i < imageData.length; i += 4 * 100) {
          const r = imageData[i]
          const g = imageData[i + 1]
          const b = imageData[i + 2]
          if (!(r === 17 && g === 24 && b === 39)) nonBackgroundPixels++
        }
        return nonBackgroundPixels
      }),
    )
    .toBeGreaterThan(25)

  const metrics = await page.evaluate(() => {
    const canvas = document.querySelector("canvas")
    const ctx = canvas?.getContext("2d")
    if (!canvas || !ctx) return null

    const { data } = ctx.getImageData(0, 0, canvas.width, canvas.height)
    let brightPixels = 0
    let goldPixels = 0

    for (let i = 0; i < data.length; i += 4 * 100) {
      const r = data[i]
      const g = data[i + 1]
      const b = data[i + 2]
      if (r > 220 && g > 220 && b > 220) brightPixels++
      if (r > 200 && g > 140 && b < 80) goldPixels++
    }

    return {
      width: canvas.width,
      height: canvas.height,
      brightPixels,
      goldPixels,
    }
  })

  expect(metrics).toEqual(
    expect.objectContaining({
      width: 1280,
      height: 720,
    }),
  )
  expect(metrics?.brightPixels).toBeGreaterThan(5)
  expect(metrics?.goldPixels).toBeGreaterThan(1)
})
