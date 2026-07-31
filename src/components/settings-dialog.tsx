import { useEffect, type ComponentType } from "react"
import {
  MicIcon,
  TvIcon,
  KeyIcon,
  BookOpenIcon,
  RadioIcon,
  HelpCircleIcon,
  UserIcon,
  BrainCircuitIcon,
  CastIcon,
  PaletteIcon,
  SparklesIcon,
} from "lucide-react"
import {
  useSettingsNavigationStore,
  type SettingsSection,
} from "@/lib/settings-dialog"
import { cn } from "@/lib/utils"
import { AudioSection } from "@/components/settings/sections/AudioSection"
import { SpeechSection } from "@/components/settings/sections/SpeechSection"
import { BibleSection } from "@/components/settings/sections/BibleSection"
import { DisplayModeSection } from "@/components/settings/sections/DisplayModeSection"
import { BroadcastSection } from "@/components/settings/sections/BroadcastSection"
import { ThemeSection } from "@/components/settings/sections/ThemeSection"
import { RemoteControlSection } from "@/components/settings/sections/RemoteControlSection"
import { ApiKeysSection } from "@/components/settings/sections/ApiKeysSection"
import { AiRankingSection } from "@/components/settings/sections/AiRankingSection"
import { AccountSection } from "@/components/settings/sections/AccountSection"
import { HelpSection } from "@/components/settings/sections/HelpSection"

const navItems: { name: string; id: SettingsSection; icon: React.ReactNode }[] = [
  { name: "Audio", id: "audio", icon: <MicIcon strokeWidth={2} /> },
  {
    name: "Speech Recognition",
    id: "speech",
    icon: <BrainCircuitIcon strokeWidth={2} />,
  },
  {
    name: "AI Ranking",
    id: "ai-ranking",
    icon: <SparklesIcon strokeWidth={2} />,
  },
  { name: "Bible", id: "bible", icon: <BookOpenIcon strokeWidth={2} /> },
  { name: "Display Mode", id: "display", icon: <TvIcon strokeWidth={2} /> },
  {
    name: "Broadcast Settings",
    id: "broadcast",
    icon: <CastIcon strokeWidth={2} />,
  },
  {
    name: "Theme Settings",
    id: "themes",
    icon: <PaletteIcon strokeWidth={2} />,
  },
  { name: "Remote Control", id: "remote", icon: <RadioIcon strokeWidth={2} /> },
  { name: "API Keys", id: "api-keys", icon: <KeyIcon strokeWidth={2} /> },
  { name: "Account", id: "account", icon: <UserIcon strokeWidth={2} /> },
  { name: "Help", id: "help", icon: <HelpCircleIcon strokeWidth={2} /> },
]

const sectionTitles: Record<SettingsSection, string> = {
  audio: "Audio",
  speech: "Speech Recognition",
  "ai-ranking": "AI Candidate Ranking",
  bible: "Bible Translation",
  display: "Display Mode",
  broadcast: "Broadcast Settings",
  themes: "Theme Settings",
  remote: "Remote Control",
  "api-keys": "API Keys",
  account: "Account",
  help: "Help",
}

const sectionComponents: Record<SettingsSection, ComponentType> = {
  audio: AudioSection,
  speech: SpeechSection,
  "ai-ranking": AiRankingSection,
  bible: BibleSection,
  display: DisplayModeSection,
  broadcast: BroadcastSection,
  themes: ThemeSection,
  remote: RemoteControlSection,
  "api-keys": ApiKeysSection,
  account: AccountSection,
  help: HelpSection,
}

const sectionDataTour: Partial<Record<SettingsSection, string>> = {
  audio: "settings-section-audio",
  speech: "settings-section-speech",
  "ai-ranking": "settings-section-ai-ranking",
  bible: "settings-section-bible",
  display: "settings-section-display",
  broadcast: "settings-section-broadcast",
  themes: "settings-section-themes",
  remote: "settings-section-remote",
  "api-keys": "settings-section-api-keys",
  account: "settings-section-account",
  help: "settings-section-help",
}

export function SettingsPage() {
  const activeSection = useSettingsNavigationStore((s) => s.activeSection)
  const setActiveSection = useSettingsNavigationStore((s) => s.setActiveSection)
  const pendingScroll = useSettingsNavigationStore((s) => s.pendingScroll)
  const clearPendingScroll = useSettingsNavigationStore(
    (s) => s.clearPendingScroll,
  )

  const ActiveContent = sectionComponents[activeSection]

  useEffect(() => {
    if (!pendingScroll) return
    const el = document.getElementById(`settings-section-${activeSection}`)
    el?.scrollIntoView({ behavior: "smooth", block: "start" })
    clearPendingScroll()
  }, [activeSection, pendingScroll, clearPendingScroll])

  return (
    <div className="view-pane flex flex-col gap-5" data-tour="settings">
      <div className="glass-panel p-5">
        <h2 className="mb-2 text-2xl font-bold text-foreground">
          Configuration and Hardware Setup
        </h2>
        <p className="text-sm text-muted-foreground">
          Manage audio capture feeds, interface endpoints, downstream
          configurations, and keyboard shortcuts.
        </p>
      </div>

      <div className="grid min-h-0 flex-1 grid-cols-1 gap-5 xl:grid-cols-[220px_1fr]">
        <nav
          className="glass-panel flex flex-col gap-1 p-3"
          aria-label="Settings sections"
        >
          {navItems.map((item) => (
            <button
              key={item.id}
              type="button"
              onClick={() => setActiveSection(item.id)}
              className={cn(
                "btn-tab flex items-center gap-2 rounded-lg border border-transparent px-3 py-2.5 text-left text-xs",
                activeSection === item.id && "active",
                activeSection === item.id
                  ? "text-[var(--accent)]"
                  : "text-muted-foreground hover:bg-[var(--shell-bg-sunken)] hover:text-foreground",
              )}
            >
              {item.icon}
              <span>{item.name}</span>
            </button>
          ))}
        </nav>

        <section
          id={`settings-section-${activeSection}`}
          data-tour={sectionDataTour[activeSection]}
          className="glass-panel min-h-0 overflow-y-auto p-5 scrollbar-thin"
        >
          <h3 className="mb-4 border-b border-[var(--border-subtle)] pb-2 font-mono text-xs font-bold uppercase tracking-wider text-foreground">
            {sectionTitles[activeSection]}
          </h3>
          <ActiveContent />
        </section>
      </div>
    </div>
  )
}
