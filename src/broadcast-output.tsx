import { createRoot } from "react-dom/client"
import { useRef } from "react"
import { useBroadcastOutputRuntime } from "@/hooks/use-broadcast-output-runtime"

/** Read output ID from URL query param (?output=main or ?output=alt). Defaults to "main". */
const outputId = new URLSearchParams(window.location.search).get("output") ?? "main"

function BroadcastCanvas() {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  useBroadcastOutputRuntime({ canvasRef, outputId })

  return (
    <canvas
      ref={canvasRef}
      style={{
        width: "100vw",
        height: "100vh",
        display: "block",
        objectFit: "contain",
      }}
    />
  )
}

const root = document.getElementById("broadcast-root")!
createRoot(root).render(<BroadcastCanvas />)
