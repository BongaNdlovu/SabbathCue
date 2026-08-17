import {
  CheckCircle2Icon,
  CheckIcon,
  ExternalLinkIcon,
  Loader2Icon,
} from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { Switch } from "@/components/ui/switch"
import { useCerebrasKeySettings } from "@/hooks/use-cerebras-key-settings"
import { useDeepseekKeySettings } from "@/hooks/use-deepseek-key-settings"
import {
  useSettingsStore,
  type AiRankingProvider,
} from "@/stores/settings-store"

const DEEPSEEK_SETUP_STEPS = [
  "Create a DeepSeek platform account and open the API keys page.",
  "Create a new key and copy it before closing the dialog.",
  "Paste it below and save — it is stored in this computer's credential manager.",
]

const CEREBRAS_SETUP_STEPS = [
  "Create a Cerebras Cloud account and open the API keys page.",
  "Create a new API key and copy it before closing the dialog.",
  "Paste it below and save — it is stored in this computer's credential manager.",
]

function ProviderOption({
  value,
  activeProvider,
  title,
  description,
  badge,
}: {
  value: AiRankingProvider
  activeProvider: AiRankingProvider
  title: string
  description: string
  badge?: string
}) {
  const isChecked = activeProvider === value
  return (
    <label
      className={`flex cursor-pointer items-start gap-3 rounded-lg border p-3 transition-colors ${
        isChecked
          ? "border-primary/60 bg-primary/5 ring-1 ring-primary/20"
          : "border-border hover:border-muted-foreground/30"
      }`}
    >
      <RadioGroupItem value={value} className="mt-0.5" id={`provider-${value}`} />
      <div className="flex flex-col gap-1">
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-xs font-medium text-foreground">{title}</span>
          {badge ? (
            <Badge variant="outline" className="text-[0.625rem]">
              {badge}
            </Badge>
          ) : null}
        </div>
        <p className="text-[0.625rem] text-muted-foreground">{description}</p>
      </div>
    </label>
  )
}

function RankingToggle() {
  const hasDeepseekApiKey = useSettingsStore((s) => s.hasDeepseekApiKey)
  const hasCerebrasApiKey = useSettingsStore((s) => s.hasCerebrasApiKey)
  const aiRankingProvider = useSettingsStore((s) => s.aiRankingProvider)
  const aiRankingEnabled = useSettingsStore((s) => s.aiRankingEnabled)
  const setAiRankingEnabled = useSettingsStore((s) => s.setAiRankingEnabled)

  const hasActiveProviderKey =
    aiRankingProvider === "deepseek" ? hasDeepseekApiKey : hasCerebrasApiKey
  const activeProviderLabel =
    aiRankingProvider === "deepseek" ? "DeepSeek" : "Cerebras"

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
            When several passages match an indirect reference, the selected AI
            ranker picks the closest one from the candidates SabbathCue already
            found. It marks a suggestion for you — nothing is projected
            automatically, and it can never introduce a passage that is not in
            your library.
          </p>
        </div>
        <Switch
          id="ai-ranking-toggle"
          aria-label="AI candidate ranking"
          checked={aiRankingEnabled}
          disabled={!hasActiveProviderKey}
          onCheckedChange={setAiRankingEnabled}
        />
      </div>
      {hasActiveProviderKey ? null : (
        <p className="text-[0.625rem] text-muted-foreground">
          Save a {activeProviderLabel} API key below to enable ranking.
        </p>
      )}
    </div>
  )
}

function ProviderSelector() {
  const aiRankingProvider = useSettingsStore((s) => s.aiRankingProvider)
  const setAiRankingProvider = useSettingsStore((s) => s.setAiRankingProvider)

  return (
    <div className="flex flex-col gap-2">
      <label className="text-xs font-medium tracking-wider text-muted-foreground uppercase">
        Ranking Provider
      </label>
      <RadioGroup
        value={aiRankingProvider}
        onValueChange={(val) => setAiRankingProvider(val as AiRankingProvider)}
        className="grid grid-cols-1 gap-2 sm:grid-cols-2"
        aria-label="Ranking Provider"
      >
        <ProviderOption
          value="deepseek"
          activeProvider={aiRankingProvider}
          title="DeepSeek"
          description="Streaming candidate ranker with reasoning disabled."
          badge="Default"
        />
        <ProviderOption
          value="cerebras"
          activeProvider={aiRankingProvider}
          title="Cerebras GPT-OSS-120B"
          description="Ultra-fast structured schema ranking with low-effort reasoning."
        />
      </RadioGroup>
    </div>
  )
}

function DeepseekKeyFields() {
  const settings = useDeepseekKeySettings()
  const aiRankingProvider = useSettingsStore((s) => s.aiRankingProvider)
  const setAiRankingEnabled = useSettingsStore((s) => s.setAiRankingEnabled)

  const editingExistingKey =
    settings.hasDeepseekApiKey &&
    !settings.editingSavedKey &&
    !settings.keyValue

  const handleRemoveKey = async () => {
    await settings.handleClearKey()
    if (aiRankingProvider === "deepseek") {
      setAiRankingEnabled(false)
    }
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
          {DEEPSEEK_SETUP_STEPS.map((step) => (
            <li key={step}>{step}</li>
          ))}
        </ol>
        <p>
          When ranking runs, SabbathCue sends only a short phrase from the
          sermon and up to eight locally detected reference-and-verse candidate
          packs — never the full transcript. Turn the toggle off to keep
          everything on this device.
        </p>
      </div>
    </div>
  )
}

function CerebrasKeyFields() {
  const settings = useCerebrasKeySettings()
  const aiRankingProvider = useSettingsStore((s) => s.aiRankingProvider)
  const setAiRankingEnabled = useSettingsStore((s) => s.setAiRankingEnabled)

  const editingExistingKey =
    settings.hasCerebrasApiKey &&
    !settings.editingSavedKey &&
    !settings.keyValue

  const handleRemoveKey = async () => {
    await settings.handleClearKey()
    if (aiRankingProvider === "cerebras") {
      setAiRankingEnabled(false)
    }
  }

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <label className="text-xs font-medium tracking-wider text-muted-foreground uppercase">
          Cerebras API Key
        </label>
        {settings.hasCerebrasApiKey ? (
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
          placeholder="Enter your Cerebras API key..."
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
        {settings.hasCerebrasApiKey ? (
          <Button
            size="sm"
            variant="outline"
            onClick={() => void handleRemoveKey()}
          >
            Remove
          </Button>
        ) : null}
        {settings.hasCerebrasApiKey ? (
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
            href="https://cloud.cerebras.ai"
            target="_blank"
            rel="noreferrer"
            className="inline-flex items-center gap-1 text-primary underline underline-offset-2"
          >
            Open Cerebras console
            <ExternalLinkIcon className="size-2.5" />
          </a>
          :
        </p>
        <ol className="ml-3 flex list-decimal flex-col gap-0.5">
          {CEREBRAS_SETUP_STEPS.map((step) => (
            <li key={step}>{step}</li>
          ))}
        </ol>
        <p>
          When ranking runs, SabbathCue sends only a short phrase from the
          sermon and up to eight locally detected reference-and-verse candidate
          packs — never the full transcript. Turn the toggle off to keep
          everything on this device.
        </p>
      </div>
    </div>
  )
}

export function AiRankingSection() {
  const aiRankingProvider = useSettingsStore((s) => s.aiRankingProvider)

  return (
    <div className="flex flex-col gap-6">
      <RankingToggle />
      <ProviderSelector />
      {aiRankingProvider === "cerebras" ? (
        <CerebrasKeyFields />
      ) : (
        <DeepseekKeyFields />
      )}
    </div>
  )
}
