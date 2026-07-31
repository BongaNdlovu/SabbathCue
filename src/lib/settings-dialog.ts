import { create } from "zustand"
import { useDashboardWorkspaceStore } from "@/stores/dashboard-workspace-store"

export type SettingsSection =
  | "audio"
  | "speech"
  | "ai-ranking"
  | "bible"
  | "display"
  | "broadcast"
  | "themes"
  | "api-keys"
  | "remote"
  | "account"
  | "help"

interface SettingsNavigationState {
  activeSection: SettingsSection
  pendingScroll: boolean
  openSettings: (section?: SettingsSection) => void
  setActiveSection: (section: SettingsSection) => void
  clearPendingScroll: () => void
}

const useSettingsNavigationStore = create<SettingsNavigationState>((set) => ({
  activeSection: "audio",
  pendingScroll: false,
  openSettings: (section) =>
    set((state) => ({
      activeSection: section ?? state.activeSection,
      pendingScroll: true,
    })),
  setActiveSection: (activeSection) =>
    set({ activeSection, pendingScroll: true }),
  clearPendingScroll: () => set({ pendingScroll: false }),
}))

export function openSettings(section?: SettingsSection) {
  const nav = useSettingsNavigationStore.getState()
  nav.openSettings(section)
  useDashboardWorkspaceStore.getState().setWorkspace("settings")
}

/** @deprecated Use useSettingsNavigationStore */
export const useSettingsDialogStore = useSettingsNavigationStore

export { useSettingsNavigationStore }
