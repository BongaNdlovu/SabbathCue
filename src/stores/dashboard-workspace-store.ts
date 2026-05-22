import { create } from "zustand"

export type DashboardWorkspace =
  | "live"
  | "service-plans"
  | "hymns"
  | "live-service"
  | "live-hymns"

interface DashboardWorkspaceState {
  workspace: DashboardWorkspace
  setWorkspace: (workspace: DashboardWorkspace) => void
}

export const useDashboardWorkspaceStore = create<DashboardWorkspaceState>(
  (set) => ({
    workspace: "live",
    setWorkspace: (workspace) => set({ workspace }),
  })
)
