import { useState, useRef, useEffect } from "react"
import { invoke } from "@tauri-apps/api/core"
import { open } from "@tauri-apps/plugin-dialog"
import { listen } from "@tauri-apps/api/event"
import { useTranslation } from "react-i18next"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import { Progress } from "@/components/ui/progress"
import { Badge } from "@/components/ui/badge"
import { Folder, FolderOutput, Loader2, Check, X } from "lucide-react"
import type { BatchConfig, BatchProgress } from "@/types/livemux"

interface PhaseEvent {
  phase: "scanning" | "processing"
  total: number | null
}

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
  const [phase, setPhase] = useState<"idle" | "scanning" | "processing">("idle")
  const [errorMsg, setErrorMsg] = useState("")
  const [progressItems, setProgressItems] = useState<BatchProgress[]>([])
  const [progressPercent, setProgressPercent] = useState(0)
  const [currentProcessing, setCurrentProcessing] = useState<{ current: number; total: number; file: string } | null>(null)
  const [pairsFound, setPairsFound] = useState<number | null>(null)
  const listRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight
    }
  }, [progressItems])

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
    setPhase("scanning")
    setErrorMsg("")
    setProgressItems([])
    setProgressPercent(0)
    setCurrentProcessing(null)
    setPairsFound(null)

    const unlistenPhase = await listen<PhaseEvent>("mux-phase", (event) => {
      setPhase(event.payload.phase)
      if (event.payload.total !== null) {
        setPairsFound(event.payload.total)
      }
    })

    const unlisten = await listen<BatchProgress>("mux-progress", (event) => {
      const p = event.payload
      setPhase("processing")
      if (p.status === "processing") {
        // File is starting — show current file, update progress to reflect completed so far
        setCurrentProcessing({ current: p.current, total: p.total, file: p.file })
        setProgressPercent(Math.round(((p.current - 1) / p.total) * 100))
      } else {
        // File completed — add to results, clear current
        setCurrentProcessing(null)
        setProgressItems((prev) => [...prev, p])
        setProgressPercent(Math.round((p.current / p.total) * 100))
      }
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
      unlistenPhase()
      setPhase("idle")
    }
  }

  return (
    <Card>
      <CardContent className="space-y-4 pt-6">
        {/* Directory pickers */}
        <div className="space-y-2">
          <Button variant="outline" onClick={pickDir} disabled={status === "running"} className="w-full justify-start">
            <Folder className="mr-2 h-4 w-4" />
            {directory ? directory : t("batch.selectInput")}
          </Button>
          <Button variant="outline" onClick={pickOutputDir} disabled={status === "running"} className="w-full justify-start">
            <FolderOutput className="mr-2 h-4 w-4" />
            {outputDir ? outputDir : t("batch.selectOutput")}
          </Button>
        </div>

        {/* Options */}
        <div className={`grid grid-cols-2 gap-3 ${status === "running" ? "opacity-50 pointer-events-none" : ""}`}>
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
        {status === "running" && (
          <div className="space-y-2">
            {/* Phase indicator */}
            {phase === "scanning" && (
              <div className="flex items-center gap-2 text-sm text-muted-foreground">
                <Loader2 className="h-4 w-4 animate-spin shrink-0" />
                <span>{t("batch.scanning")}</span>
              </div>
            )}
            {phase === "processing" && pairsFound === 0 && (
              <div className="text-sm text-yellow-600 dark:text-yellow-400">
                {t("batch.noPairs")}
              </div>
            )}
            {phase === "processing" && (pairsFound ?? 0) > 0 && (
              <>
                <div className="flex items-center gap-2">
                  <Progress value={progressPercent} className="flex-1" />
                  <span className="text-xs text-muted-foreground tabular-nums shrink-0">{progressPercent}%</span>
                </div>
                {currentProcessing && (
                  <div className="flex items-center gap-2 text-xs text-muted-foreground">
                    <Loader2 className="h-3 w-3 animate-spin shrink-0" />
                    <span className="truncate">
                      [{currentProcessing.current}/{currentProcessing.total}] {fileName(currentProcessing.file)}
                    </span>
                  </div>
                )}
              </>
            )}
          </div>
        )}

        {status === "done" && (
          <div className="flex items-center gap-2 text-sm text-success">
            <Check className="h-4 w-4" />
            {t("batch.batchComplete", {
              succeeded: progressItems.filter((i) => i.success).length,
              total: progressItems.length,
            })}
          </div>
        )}
        {status === "error" && (
          <div className="text-sm text-destructive flex items-center gap-2">
            <X className="h-4 w-4" /> {errorMsg}
          </div>
        )}

        {/* Progress list */}
        {progressItems.length > 0 && (
          <div ref={listRef} className="max-h-48 overflow-y-auto space-y-1 text-xs">
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
