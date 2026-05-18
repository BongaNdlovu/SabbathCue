import { APP_DISPLAY_NAME } from "./app-brand";

export const SITE = {
  name: APP_DISPLAY_NAME,
  legalName: "BongaNdlovu",
  tagline: "Your Pastor speaks. SabbathCue finds the verse.",
  shortDescription:
    "Real-time AI Bible verse detection for live sermons. Free, open-source, broadcast-ready via NDI.",
  description:
    "SabbathCue listens to a live sermon audio feed, transcribes speech in real time, detects Bible verse references (both explicit citations and quoted passages), and renders them as broadcast-ready overlays via NDI for live production.",
  url: "https://github.com/BongaNdlovu/SabbathCue",
  locale: "en_US",
  twitterHandle: "",
  founded: "2025",
  category: "ChurchSoftware",
  operatingSystems: ["Windows", "macOS"],
  repo: {
    owner: "BongaNdlovu",
    name: "SabbathCue",
    url: "https://github.com/BongaNdlovu/SabbathCue",
    releasesLatest: "https://github.com/BongaNdlovu/SabbathCue/releases/latest",
    discussions: "https://github.com/BongaNdlovu/SabbathCue/discussions",
    stars: { fallback: 0 },
  },
  socials: {
    github: "https://github.com/BongaNdlovu/SabbathCue",
  },
  stats: {
    languages: "2+",
    translations: "6+",
  },
} as const;

export async function getGitHubStars(): Promise<number> {
  try {
    const headers: Record<string, string> = {
      Accept: "application/vnd.github+json",
    };
    const token = process.env.GITHUB_TOKEN;
    if (token) headers.Authorization = `Bearer ${token}`;

    const res = await fetch(
      `https://api.github.com/repos/${SITE.repo.owner}/${SITE.repo.name}`,
      { headers }
    );
    if (!res.ok) return SITE.repo.stars.fallback;
    const data = (await res.json()) as { stargazers_count?: number };
    return typeof data.stargazers_count === "number"
      ? data.stargazers_count
      : SITE.repo.stars.fallback;
  } catch {
    return SITE.repo.stars.fallback;
  }
}
