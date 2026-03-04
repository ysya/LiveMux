import { useState, useEffect } from "react"
import { invoke } from "@tauri-apps/api/core"
import { open } from "@tauri-apps/plugin-dialog"
import { listen } from "@tauri-apps/api/event"
import { useTranslation } from "react-i18next"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Progress } from "@/components/ui/progress"
import { Badge } from "@/components/ui/badge"
import { Folder, Loader2, Check, X, Smartphone } from "lucide-react"

interface AdbPushProgress {
  current: number
  total: number
  file: string
  status: "pushing" | "done" | "error"
  error: string | null
}

export function AdbPushForm() {
  const { t } = useTranslation()
  const [sourceDir, setSourceDir] = useState<string | null>(null)
  const [targetDir, setTargetDir] = useState("/sdcard/DCIM/LiveMux/")
  const [adbOk, setAdbOk] = useState<boolean | null>(null)
  const [formStatus, setFormStatus] = useState<"idle" | "running" | "done" | "error">("idle")
  const [errorMsg, setErrorMsg] = useState("")
  const [completedItems, setCompletedItems] = useState<AdbPushProgress[]>([])
  const [currentFile, setCurrentFile] = useState<AdbPushProgress | null>(null)
  const [progressPercent, setProgressPercent] = useState(0)

  useEffect(() => {
    invoke<boolean>("check_adb").then(setAdbOk).catch(() => setAdbOk(false))
  }, [])

  async function pickSourceDir() {
    const selected = await open({ directory: true })
    if (selected) setSourceDir(selected as string)
  }

  async function handlePush() {
    if (!sourceDir || !targetDir) return
    setFormStatus("running")
    setErrorMsg("")
    setCompletedItems([])
    setCurrentFile(null)
    setProgressPercent(0)

    const unlisten = await listen<AdbPushProgress>("adb-push-progress", (event) => {
      const p = event.payload
      if (p.status === "pushing") {
        setCurrentFile(p)
        // Progress: (completed files + 0.5 for in-progress) / total
        setProgressPercent(Math.round(((p.current - 1) / p.total) * 100))
      } else {
        setCurrentFile(null)
        setCompletedItems((prev) => [...prev, p])
        setProgressPercent(Math.round((p.current / p.total) * 100))
      }
    })

    try {
      await invoke("adb_push_directory", {
        sourceDir,
        targetDir,
      })
      setFormStatus("done")
    } catch (e) {
      setFormStatus("error")
      setErrorMsg(String(e))
    } finally {
      unlisten()
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("adb.title")}</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        {adbOk === false && (
          <div className="p-3 rounded-md bg-yellow-500/10 text-yellow-600 dark:text-yellow-400 text-sm">
            {t("adb.adbNotFound")}
          </div>
        )}

        {/* Source directory */}
        <Button variant="outline" onClick={pickSourceDir} className="w-full justify-start">
          <Folder className="mr-2 h-4 w-4" />
          {sourceDir ? sourceDir : t("adb.selectSource")}
        </Button>

        {/* Target path */}
        <div className="space-y-1">
          <label className="text-sm text-muted-foreground">{t("adb.targetPath")}</label>
          <div className="flex items-center gap-2">
            <Smartphone className="h-4 w-4 text-muted-foreground shrink-0" />
            <input
              type="text"
              value={targetDir}
              onChange={(e) => setTargetDir(e.target.value)}
              className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-xs transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            />
          </div>
        </div>

        {/* Action */}
        <Button
          onClick={handlePush}
          disabled={!sourceDir || !targetDir || formStatus === "running"}
          className="w-full"
        >
          {formStatus === "running" && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
          {formStatus === "running" ? t("adb.pushing") : t("adb.push")}
        </Button>

        {/* Progress */}
        {formStatus === "running" && (
          <div className="space-y-2">
            <Progress value={progressPercent} />
            {currentFile && (
              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <Loader2 className="h-3 w-3 animate-spin shrink-0" />
                <span className="truncate">
                  [{currentFile.current}/{currentFile.total}] {fileName(currentFile.file)}
                </span>
              </div>
            )}
          </div>
        )}

        {formStatus === "done" && (
          <div className="flex items-center gap-2 text-sm text-success">
            <Check className="h-4 w-4" /> {t("adb.complete", { count: completedItems.length })}
          </div>
        )}
        {formStatus === "error" && (
          <div className="text-sm text-destructive flex items-center gap-2">
            <X className="h-4 w-4" /> {errorMsg}
          </div>
        )}

        {/* Completed list */}
        {completedItems.length > 0 && (
          <div className="max-h-48 overflow-y-auto space-y-1 text-xs">
            {completedItems.map((item, i) => (
              <div key={i} className="flex items-center gap-2">
                {item.status === "done" ? (
                  <Badge variant="secondary" className="text-success">{t("adb.ok")}</Badge>
                ) : (
                  <Badge variant="destructive">{t("adb.err")}</Badge>
                )}
                <span className="truncate">{fileName(item.file)}</span>
                {item.error && (
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

function fileName(path: string): string {
  return path.split(/[/\\]/).pop() || path
}
