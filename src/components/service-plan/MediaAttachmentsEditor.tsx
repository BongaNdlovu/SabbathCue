import { open } from "@tauri-apps/plugin-dialog"
import { Button } from "@/components/ui/button"
import { invokeTauri } from "@/lib/tauri-runtime"
import type { ServiceAttachment } from "@/types/service-plan"

const SUPPORTED_ATTACHMENT_EXTENSIONS = [
  "png",
  "jpg",
  "jpeg",
  "webp",
  "gif",
  "mp4",
  "mov",
  "webm",
  "pdf",
]

interface ServiceAttachmentValidation {
  label: string
  kind: ServiceAttachment["kind"]
  sizeBytes: number
}

interface MediaAttachmentsEditorProps {
  attachments: ServiceAttachment[]
  onChange: (attachments: ServiceAttachment[]) => void
}

async function createAttachmentFromPath(path: string): Promise<ServiceAttachment | null> {
  try {
    const validated = await invokeTauri<ServiceAttachmentValidation>(
      "validate_service_attachment_path",
      { path },
    )
    return {
      id: crypto.randomUUID(),
      kind: validated.kind,
      label: validated.label,
      path,
      status: "pending",
      sizeBytes: validated.sizeBytes,
    }
  } catch {
    return null
  }
}

export function MediaAttachmentsEditor({ attachments, onChange }: MediaAttachmentsEditorProps) {
  const attachFiles = async () => {
    let selected: string | string[] | null
    try {
      selected = await open({
        multiple: true,
        filters: [
          {
            name: "Slides and media",
            extensions: SUPPORTED_ATTACHMENT_EXTENSIONS,
          },
        ],
      })
    } catch {
      return
    }

    const paths = Array.isArray(selected) ? selected : selected ? [selected] : []
    if (paths.length === 0) return

    const selectedAttachments = (
      await Promise.all(paths.map((path) => createAttachmentFromPath(path)))
    ).filter((attachment): attachment is ServiceAttachment => attachment !== null)

    if (selectedAttachments.length === 0) return
    onChange([...attachments, ...selectedAttachments])
  }

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <span className="text-[0.625rem] font-medium uppercase tracking-wide text-muted-foreground">
          Slides and media
        </span>
        <Button size="sm" variant="outline" className="h-7 text-xs" onClick={() => void attachFiles()}>
          Attach files
        </Button>
      </div>
      {attachments.length === 0 ? (
        <p className="text-xs text-muted-foreground">No slide or media files attached.</p>
      ) : (
        <div className="space-y-1 text-xs">
          {attachments.map((attachment) => (
            <div key={attachment.id} className="flex items-center justify-between rounded-md border border-border px-2 py-1">
          <span className="truncate">
                {attachment.label} <span className="text-muted-foreground">({attachment.kind})</span>
                {typeof attachment.sizeBytes === "number" && (
                  <span className="text-muted-foreground">
                    {" "}
                    - {(attachment.sizeBytes / (1024 * 1024)).toFixed(1)} MB
                  </span>
                )}
              </span>
              <Button
                size="icon-xs"
                variant="ghost"
                onClick={() => onChange(attachments.filter((entry) => entry.id !== attachment.id))}
              >
                x
              </Button>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
