import {
  CheckCircle2Icon,
  CheckIcon,
  ExternalLinkIcon,
  Loader2Icon,
} from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { useDeepseekKeySettings } from "@/hooks/use-deepseek-key-settings"
import { useSettingsStore } from "@/stores/settings-store"

const SETUP_STEPS = [
  "Create a DeepSeek platform account and open the API keys page.",
  "Create a new key and copy it before closing the dialog.",
  "Paste it below and save — it is stored in this computer's credential manager.",
]

function RankingToggle() {
  const hasDeepseekApiKey = useSettingsStore((s) => s.hasDeepseekApiKey)
  const deepseekRankingEnabled = useSettingsStore(
    (s) => s.deepseekRankingEnabled
  )
  const setDeepseekRankingEnabled = useSettingsStore(
    (s) => s.setDeepseekRankingEnabled
  )

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-start justify-between gap-3">
        <div className="flex flex-col gap-1">
          <label
            htmlFor="ai-ranking-toggle"
            className="text-xs font-medium tracking-wider text-muted-foreground uppercase"
          >
            AI candidate ranking
          </label>
          <p className="text-[0.625rem] text-muted-foreground">
            When several passages match an indirect reference, DeepSeek picks
            the closest one from the candidates SabbathCue already found. It
            marks a suggestion for you — nothing is projected automatically, and
            it can never introduce a passage that is not in your library.
          </p>
        </div>
        <Switch
          id="ai-ranking-toggle"
          aria-label="AI candidate ranking"
          checked={deepseekRankingEnabled}
          disabled={!hasDeepseekApiKey}
          onCheckedChange={setDeepseekRankingEnabled}
        />
      </div>
      {hasDeepseekApiKey ? null : (
        <p className="text-[0.625rem] text-muted-foreground">
          Save a DeepSeek API key below to enable ranking.
        </p>
      )}
    </div>
  )
}

function DeepseekKeyFields() {
  const settings = useDeepseekKeySettings()
  const setDeepseekRankingEnabled = useSettingsStore(
    (s) => s.setDeepseekRankingEnabled
  )

  const editingExistingKey =
    settings.hasDeepseekApiKey &&
    !settings.editingSavedKey &&
    !settings.keyValue

  const handleRemoveKey = async () => {
    await settings.handleClearKey()
    // Ranking cannot run without a key; leaving the toggle on would misreport
    // the feature as active.
    setDeepseekRankingEnabled(false)
  }

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <label className="text-xs font-medium tracking-wider text-muted-foreground uppercase">
          DeepSeek API Key
        </label>
        {settings.hasDeepseekApiKey ? (
          <Badge variant="outline" className="gap-1 text-[0.5rem]">
            {settings.keyVerified ? (
              <CheckCircle2Icon className="size-2.5 text-emerald-500" />
            ) : null}
            {settings.keyVerified ? "Key verified" : "Key configured"}
          </Badge>
        ) : null}
      </div>
      <div className="flex gap-2">
        <Input
          type={editingExistingKey ? "text" : "password"}
          placeholder="Enter your DeepSeek API key..."
          value={settings.displayedKeyValue}
          readOnly={editingExistingKey}
          onChange={(e) => {
            settings.setEditingSavedKey(true)
            settings.setKeyValue(e.target.value)
          }}
          className="flex-1 text-xs"
        />
        <Button size="sm" onClick={() => void settings.handleKeyAction()}>
          {settings.saved ? (
            <>
              <CheckIcon className="size-3" />
              Saved
            </>
          ) : (
            settings.keyActionLabel
          )}
        </Button>
        {settings.hasDeepseekApiKey ? (
          <Button
            size="sm"
            variant="outline"
            onClick={() => void handleRemoveKey()}
          >
            Remove
          </Button>
        ) : null}
        {settings.hasDeepseekApiKey ? (
          <Button
            size="sm"
            variant="secondary"
            disabled={settings.validating}
            onClick={() => void settings.handleValidateKey()}
          >
            {settings.validating ? (
              <Loader2Icon className="size-3 animate-spin" />
            ) : (
              <CheckCircle2Icon className="size-3" />
            )}
            {settings.validating ? "Testing" : "Test key"}
          </Button>
        ) : null}
      </div>
      {settings.keyError ? (
        <p className="text-[0.625rem] text-red-500">{settings.keyError}</p>
      ) : null}
      {settings.validationMessage ? (
        <p
          className={`text-[0.625rem] ${
            settings.keyVerified ? "text-emerald-600" : "text-red-500"
          }`}
          role="status"
        >
          {settings.validationMessage}
        </p>
      ) : null}
      <div className="flex flex-col gap-1.5 text-[0.625rem] text-muted-foreground">
        <p>
          Optional. SabbathCue detects references without it. Get a key at{" "}
          <a
            href="https://platform.deepseek.com"
            target="_blank"
            rel="noreferrer"
            className="inline-flex items-center gap-1 text-primary underline underline-offset-2"
          >
            Open DeepSeek console
            <ExternalLinkIcon className="size-2.5" />
          </a>
          :
        </p>
        <ol className="ml-3 flex list-decimal flex-col gap-0.5">
          {SETUP_STEPS.map((step) => (
            <li key={step}>{step}</li>
          ))}
        </ol>
        <p>
          When ranking runs, SabbathCue sends only a short phrase from the
          sermon and the shortlisted references — never the full transcript.
          Turn the toggle off to keep everything on this device.
        </p>
      </div>
    </div>
  )
}

export function AiRankingSection() {
  return (
    <div className="flex flex-col gap-6">
      <RankingToggle />
      <DeepseekKeyFields />
    </div>
  )
}
