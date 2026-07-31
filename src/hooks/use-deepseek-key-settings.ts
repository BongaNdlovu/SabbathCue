import { useApiKeySettings } from "@/hooks/use-api-key-settings"
import { createProviderKeyActions } from "@/lib/stt-key-settings"
import { useSettingsStore } from "@/stores/settings-store"

const deepseekKeyActions = createProviderKeyActions({
  label: "DeepSeek",
  setCommand: "set_deepseek_api_key",
  hasCommand: "has_deepseek_api_key",
  clearCommand: "clear_deepseek_api_key",
  validateCommand: "validate_deepseek_api_key",
})

export async function saveDeepseekApiKey(
  apiKey: string
): Promise<{ hasKey: boolean; error?: string }> {
  return deepseekKeyActions.saveApiKey(apiKey)
}

export async function clearDeepseekApiKey(): Promise<{ error?: string }> {
  return deepseekKeyActions.clearApiKey()
}

export function useDeepseekKeySettings() {
  const hasDeepseekApiKey = useSettingsStore((s) => s.hasDeepseekApiKey)
  const setHasDeepseekApiKey = useSettingsStore((s) => s.setHasDeepseekApiKey)

  const keySettings = useApiKeySettings({
    hasKey: hasDeepseekApiKey,
    setHasKey: setHasDeepseekApiKey,
    save: saveDeepseekApiKey,
    clear: clearDeepseekApiKey,
    validate: deepseekKeyActions.validateApiKey,
    validationSuccessMessage:
      "Connection verified — this key is ready for AI candidate ranking.",
  })

  return {
    hasDeepseekApiKey,
    ...keySettings,
  }
}
