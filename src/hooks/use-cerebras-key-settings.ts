import { useApiKeySettings } from "@/hooks/use-api-key-settings"
import { createProviderKeyActions } from "@/lib/stt-key-settings"
import { useSettingsStore } from "@/stores/settings-store"

const cerebrasKeyActions = createProviderKeyActions({
  label: "Cerebras",
  setCommand: "set_cerebras_api_key",
  hasCommand: "has_cerebras_api_key",
  clearCommand: "clear_cerebras_api_key",
  validateCommand: "validate_cerebras_api_key",
})

export async function saveCerebrasApiKey(
  apiKey: string
): Promise<{ hasKey: boolean; error?: string }> {
  return cerebrasKeyActions.saveApiKey(apiKey)
}

export async function clearCerebrasApiKey(): Promise<{ error?: string }> {
  return cerebrasKeyActions.clearApiKey()
}

export function useCerebrasKeySettings() {
  const hasCerebrasApiKey = useSettingsStore((s) => s.hasCerebrasApiKey)
  const setHasCerebrasApiKey = useSettingsStore((s) => s.setHasCerebrasApiKey)

  const keySettings = useApiKeySettings({
    hasKey: hasCerebrasApiKey,
    setHasKey: setHasCerebrasApiKey,
    save: saveCerebrasApiKey,
    clear: clearCerebrasApiKey,
    validate: cerebrasKeyActions.validateApiKey,
    validationSuccessMessage:
      "Connection verified — this key is ready for AI candidate ranking.",
  })

  return {
    hasCerebrasApiKey,
    ...keySettings,
  }
}
