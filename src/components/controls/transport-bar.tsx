import { lazy, Suspense, useState } from "react"
import { Badge } from "@/components/ui/badge"
import { CastIcon, MoonIcon, PaletteIcon, SunIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { SettingsDialog } from "@/components/settings-dialog"
import { useBroadcastStore } from "@/stores/broadcast-store"
import { useTheme } from "@/components/theme-provider"
import { APP_DISPLAY_NAME } from "@/lib/app-brand"

const LazyBroadcastSettings = lazy(() =>
  import("@/components/broadcast/broadcast-settings").then((mod) => ({
    default: mod.BroadcastSettings,
  })),
)

const LazyThemeDesigner = lazy(() =>
  import("@/components/broadcast/theme-designer").then((mod) => ({
    default: mod.ThemeDesigner,
  })),
)

export function TransportBar() {
  const { theme, setTheme } = useTheme()
  const [broadcastOpen, setBroadcastOpen] = useState(false)
  const [broadcastSettingsMounted, setBroadcastSettingsMounted] = useState(false)
  const [themeDesignerMounted, setThemeDesignerMounted] = useState(false)

  return (
    <div
      data-slot="transport-bar"
      className="col-span-4 flex h-14 items-center justify-between border-b border-border  bg-card px-3"
    >
      <div className="flex items-center gap-2.5">
        <span className="text-sm font-semibold tracking-tight text-foreground">
          {APP_DISPLAY_NAME}
        </span>
        <Badge variant="outline" className="text-[0.5625rem] uppercase">
          Free
        </Badge>
      </div>

      <div className="flex items-center gap-3">
        <Button
          variant="ghost"
          size="icon-sm"
          title="Toggle theme"
          onClick={() => setTheme(theme === "dark" ? "light" : "dark")}
        >
          {theme === "dark" ? (
            <SunIcon className="size-3.5" />
          ) : (
            <MoonIcon className="size-3.5" />
          )}
        </Button>
        <Button
          variant="ghost"
          size="icon-sm"
          title="Broadcast Settings"
          data-tour="broadcast"
          onClick={() => {
            setBroadcastSettingsMounted(true)
            setBroadcastOpen(true)
          }}
        >
          <CastIcon className="size-3.5" />
        </Button>
        {broadcastSettingsMounted && (
          <Suspense fallback={null}>
            <LazyBroadcastSettings
              open={broadcastOpen}
              onOpenChange={setBroadcastOpen}
            />
          </Suspense>
        )}
        <Button
          variant="ghost"
          size="icon-sm"
          title="Theme Designer"
          data-tour="theme"
          onClick={() => {
            setThemeDesignerMounted(true)
            useBroadcastStore.getState().setDesignerOpen(true)
          }}
        >
          <PaletteIcon className="size-3.5" />
        </Button>
        {themeDesignerMounted && (
          <Suspense fallback={null}>
            <LazyThemeDesigner />
          </Suspense>
        )}
        <SettingsDialog />
      </div>
    </div>
  )
}
