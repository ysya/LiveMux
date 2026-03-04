import { useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import { open } from "@tauri-apps/plugin-dialog"
import { listen } from "@tauri-apps/api/event"
import { useTranslation } from "react-i18next"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import { Progress } from "@/components/ui/progress"
import { Badge } from "@/components/ui/badge"
import { Folder, FolderOutput, Loader2, Check, X } from "lucide-react"
import type { BatchConfig, BatchProgress } from "@/types/livemux"

export function DirForm() {
  const { t } = useTranslation()
  const [directory, setDirectory] = useState<string | null>(null)
  const [outputDir, setOutputDir] = useState<string | null>(null)
  const [recursive, setRecursive] = useState(true)
  const [exifMatch, setExifMatch] = useState(false)
  const [incremental, setIncremental] = useState(true)
  const [copyUnmuxed, setCopyUnmuxed] = useState(false)
  const [overwrite, setOverwrite] = useState(false)
  const [deleteVideo, setDeleteVideo] = useState(false)
  const [status, setStatus] = useState<"idle" | "running" | "done" | "error">("idle")
  const [errorMsg, setErrorMsg] = useState("")
  const [progressItems, setProgressItems] = useState<BatchProgress[]>([])
  const [progressPercent, setProgressPercent] = useState(0)

  async function pickDir() {
    const selected = await open({ directory: true })
    if (selected) setDirectory(selected as string)
  }

  async function pickOutputDir() {
    const selected = await open({ directory: true })
    if (selected) setOutputDir(selected as string)
  }

  async function handleBatch() {
    if (!directory) return
    setStatus("running")
    setErrorMsg("")
    setProgressItems([])
    setProgressPercent(0)

    const unlisten = await listen<BatchProgress>("mux-progress", (event) => {
      const p = event.payload
      setProgressItems((prev) => [...prev, p])
      setProgressPercent(Math.round((p.current / p.total) * 100))
    })

    const config: BatchConfig = {
      directory,
      output_dir: outputDir,
      recursive,
      exif_match: exifMatch,
      incremental,
      copy_unmuxed: copyUnmuxed,
      delete_video: deleteVideo,
      delete_temp: true,
      overwrite,
    }

    try {
      await invoke("mux_directory", { config })
      setStatus("done")
    } catch (e) {
      setStatus("error")
      setErrorMsg(String(e))
    } finally {
      unlisten()
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("batch.title")}</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        {/* Directory pickers */}
        <div className="space-y-2">
          <Button variant="outline" onClick={pickDir} className="w-full justify-start">
            <Folder className="mr-2 h-4 w-4" />
            {directory ? directory : t("batch.selectInput")}
          </Button>
          <Button variant="outline" onClick={pickOutputDir} className="w-full justify-start">
            <FolderOutput className="mr-2 h-4 w-4" />
            {outputDir ? outputDir : t("batch.selectOutput")}
          </Button>
        </div>

        {/* Options */}
        <div className="grid grid-cols-2 gap-3">
          <OptionCheckbox
            checked={recursive}
            onChange={setRecursive}
            label={t("batch.recursive")}
            description={t("batch.recursiveDesc")}
          />
          <OptionCheckbox
            checked={exifMatch}
            onChange={setExifMatch}
            label={t("batch.exifMatch")}
            description={t("batch.exifMatchDesc")}
          />
          <OptionCheckbox
            checked={incremental}
            onChange={setIncremental}
            label={t("batch.incremental")}
            description={t("batch.incrementalDesc")}
          />
          <OptionCheckbox
            checked={copyUnmuxed}
            onChange={setCopyUnmuxed}
            label={t("batch.copyUnmuxed")}
            description={t("batch.copyUnmuxedDesc")}
          />
          <OptionCheckbox
            checked={overwrite}
            onChange={setOverwrite}
            label={t("batch.overwrite")}
            description={t("batch.overwriteDesc")}
          />
          <OptionCheckbox
            checked={deleteVideo}
            onChange={setDeleteVideo}
            label={t("batch.deleteVideos")}
            description={t("batch.deleteVideosDesc")}
          />
        </div>

        {/* Action */}
        <Button
          onClick={handleBatch}
          disabled={!directory || status === "running"}
          className="w-full"
        >
          {status === "running" && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
          {status === "running" ? t("batch.processing") : t("batch.startBatch")}
        </Button>

        {/* Progress */}
        {status === "running" && <Progress value={progressPercent} />}

        {status === "done" && (
          <div className="flex items-center gap-2 text-sm text-success">
            <Check className="h-4 w-4" /> {t("batch.batchComplete", { count: progressItems.length })}
          </div>
        )}
        {status === "error" && (
          <div className="text-sm text-destructive flex items-center gap-2">
            <X className="h-4 w-4" /> {errorMsg}
          </div>
        )}

        {/* Progress list */}
        {progressItems.length > 0 && (
          <div className="max-h-48 overflow-y-auto space-y-1 text-xs">
            {progressItems.map((item, i) => (
              <div key={i} className="flex items-center gap-2">
                {item.success ? (
                  <Badge variant="secondary" className="text-success">{t("batch.ok")}</Badge>
                ) : (
                  <Badge variant="destructive">{t("batch.err")}</Badge>
                )}
                <span className="truncate">{fileName(item.file)}</span>
                {item.error && !item.success && (
                  <span className="text-muted-foreground truncate">{item.error}</span>
                )}
              </div>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  )
}

function OptionCheckbox({
  checked,
  onChange,
  label,
  description,
}: {
  checked: boolean
  onChange: (v: boolean) => void
  label: string
  description: string
}) {
  return (
    <label className="flex items-start gap-2 text-sm">
      <Checkbox
        checked={checked}
        onCheckedChange={(v: boolean | "indeterminate") => onChange(!!v)}
        className="mt-0.5"
      />
      <div>
        <span>{label}</span>
        <p className="text-xs text-muted-foreground">{description}</p>
      </div>
    </label>
  )
}

function fileName(path: string): string {
  return path.split(/[/\\]/).pop() || path
}
