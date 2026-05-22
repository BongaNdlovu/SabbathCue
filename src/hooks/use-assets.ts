import { useCallback, useEffect, useState } from "react"
import { invoke } from "@tauri-apps/api/core"

export interface AssetStatus {
  bible_db: boolean
  vosk_model: boolean
  vosk_worker: boolean
  onnx_model: boolean
  tokenizer: boolean
  embeddings: boolean
  embedding_ids: boolean
  semantic_ready: boolean
  ndi_sdk: boolean
}

export function useAssets() {
  const [status, setStatus] = useState<AssetStatus | null>(null)
  const [loading, setLoading] = useState(true)

  const refresh = useCallback(async () => {
    setLoading(true)
    try {
      setStatus(await invoke<AssetStatus>("asset_status"))
    } catch {
      setStatus(null)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    const timeoutId = setTimeout(() => {
      void refresh()
    }, 0)

    return () => clearTimeout(timeoutId)
  }, [refresh])

  return { status, loading, refresh }
}
