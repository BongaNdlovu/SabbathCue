import { useCallback, useState } from "react"

const SAVED_KEY_DISPLAY = "Saved in secure keychain"

export interface ApiKeySettingsConfig {
  hasKey: boolean
  setHasKey: (value: boolean) => void
  save: (apiKey: string) => Promise<{ hasKey: boolean; error?: string }>
  clear: () => Promise<{ error?: string }>
  validate: () => Promise<{ valid: boolean; error?: string }>
  /** Optional side effect after a successful save (e.g. restart active STT). */
  onSaved?: () => Promise<void> | void
  /** Shown after a successful key test. Defaults to the transcription wording. */
  validationSuccessMessage?: string
}

const DEFAULT_VALIDATION_SUCCESS =
  "Connection verified — this key is ready for live transcription."

export function useApiKeySettings({
  hasKey,
  setHasKey,
  save,
  clear,
  validate,
  onSaved,
  validationSuccessMessage = DEFAULT_VALIDATION_SUCCESS,
}: ApiKeySettingsConfig) {
  const [keyValue, setKeyValue] = useState("")
  const [editingSavedKey, setEditingSavedKey] = useState(false)
  const [saved, setSaved] = useState(false)
  const [keyError, setKeyError] = useState<string | null>(null)
  const [validating, setValidating] = useState(false)
  const [keyVerified, setKeyVerified] = useState(false)
  const [validationMessage, setValidationMessage] = useState<string | null>(null)

  const displayedKeyValue =
    hasKey && !editingSavedKey && !keyValue ? SAVED_KEY_DISPLAY : keyValue
  const keyActionLabel = hasKey ? "Update" : "Save"

  const handleSaveKey = useCallback(async () => {
    setKeyError(null)
    setKeyVerified(false)
    setValidationMessage(null)
    const result = await save(keyValue)
    setHasKey(result.hasKey)
    if (result.error) {
      setKeyError(result.error)
      return
    }
    if (result.hasKey) {
      setKeyValue("")
      setEditingSavedKey(false)
      setSaved(true)
      setTimeout(() => setSaved(false), 2000)
      await onSaved?.()
    }
  }, [keyValue, save, setHasKey, onSaved])

  const handleKeyAction = useCallback(async () => {
    if (hasKey && !editingSavedKey && !keyValue) {
      setEditingSavedKey(true)
      return
    }
    await handleSaveKey()
  }, [editingSavedKey, handleSaveKey, hasKey, keyValue])

  const handleClearKey = useCallback(async () => {
    setKeyError(null)
    const result = await clear()
    if (result.error) {
      setKeyError(result.error)
      return
    }
    setHasKey(false)
    setKeyValue("")
    setEditingSavedKey(false)
    setKeyVerified(false)
    setValidationMessage(null)
  }, [clear, setHasKey])

  const handleValidateKey = useCallback(async () => {
    setValidating(true)
    setValidationMessage(null)
    try {
      const result = await validate()
      setKeyVerified(result.valid)
      setValidationMessage(
        result.valid
          ? validationSuccessMessage
          : result.error || "The provider could not verify this key."
      )
    } finally {
      setValidating(false)
    }
  }, [validate, validationSuccessMessage])

  return {
    keyValue,
    setKeyValue,
    editingSavedKey,
    setEditingSavedKey,
    saved,
    keyError,
    validating,
    keyVerified,
    validationMessage,
    displayedKeyValue,
    keyActionLabel,
    handleKeyAction,
    handleClearKey,
    handleValidateKey,
  }
}
