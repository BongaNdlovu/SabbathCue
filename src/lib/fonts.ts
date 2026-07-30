/**
 * Font families offered to the operator in the broadcast designer.
 *
 * Every entry must be either bundled (declared as an `@font-face` in
 * `src/index.css`, sourced from a `@fontsource*` package) or a genuine
 * OS-installed system family. `fonts.test.ts` enforces that by parsing
 * `index.css` rather than restating the list, so a family named here but never
 * installed fails the suite instead of silently rendering as a fallback face.
 */

/**
 * Families we do not bundle because the OS already provides them. Canvas can
 * draw these offline with no loading step.
 */
export const SYSTEM_FONT_FAMILIES = [
  "Georgia",
  "Arial",
  "Helvetica",
  "Times New Roman",
  "Courier New",
] as const

export interface FontFamilyGroup {
  label: string
  families: string[]
}

/**
 * Grouped for the picker: an operator scanning mid-service reads a labelled
 * list far faster than one flat run of twenty names.
 */
export const FONT_FAMILY_GROUPS: FontFamilyGroup[] = [
  {
    label: "Serif",
    families: [
      "Source Serif 4 Variable",
      "EB Garamond Variable",
      "Cormorant Garamond",
      "Libre Baskerville",
      "Fraunces Variable",
      "Playfair Display",
      "Marcellus",
    ],
  },
  {
    label: "Sans",
    families: [
      "Geist Variable",
      "Plus Jakarta Sans Variable",
      "Outfit Variable",
      "Manrope Variable",
      "Sora Variable",
    ],
  },
  {
    label: "Display",
    families: ["Oswald Variable"],
  },
  {
    label: "System",
    families: [...SYSTEM_FONT_FAMILIES],
  },
]

/** Flat list in group order, for consumers that do not render groups. */
export const FONT_FAMILIES: string[] = FONT_FAMILY_GROUPS.flatMap(
  (group) => group.families
)
