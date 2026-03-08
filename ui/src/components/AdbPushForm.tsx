import { useState, useEffect, useRef, useCallback } from "react"
import { invoke } from "@tauri-apps/api/core"
import { open } from "@tauri-apps/plugin-dialog"
import { listen } from "@tauri-apps/api/event"
import { useTranslation } from "react-i18next"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Progress } from "@/components/ui/progress"
import { Badge } from "@/components/ui/badge"
import {
  Folder,
  FolderOpen,
  Loader2,
  Check,
  X,
  Smartphone,
  RefreshCw,
  ChevronRight,
  ChevronDown,
} from "lucide-react"

interface AdbDevice {
  serial: string
  model: string
}

interface DeviceDir {
  name: string
  has_children: boolean
}

interface AdbPushProgress {
  current: number
  total: number
  file: string
  status: "pushing" | "done" | "error"
  error: string | null
}

interface PhaseEvent {
  phase: "scanning" | "processing"
  total: number | null
}

interface DirNode {
  name: string
  path: string
  children: DirNode[] | null // null = not loaded yet
  expanded: boolean
}

export function AdbPushForm() {
  const { t } = useTranslation()
  const [devices, setDevices] = useState<AdbDevice[]>([])
  const [selectedDevice, setSelectedDevice] = useState<string | null>(null)
  const [devicesLoading, setDevicesLoading] = useState(false)
  const [sourceDir, setSourceDir] = useState<string | null>(null)
  const [targetDir, setTargetDir] = useState("/sdcard/DCIM/LiveMux/")
  const [fileCount, setFileCount] = useState<number | null>(null)
  const [formStatus, setFormStatus] = useState<"idle" | "running" | "done" | "error">("idle")
  const [phase, setPhase] = useState<"idle" | "scanning" | "processing">("idle")
  const [errorMsg, setErrorMsg] = useState("")
  const [completedItems, setCompletedItems] = useState<AdbPushProgress[]>([])
  const [currentFile, setCurrentFile] = useState<AdbPushProgress | null>(null)
  const [progressPercent, setProgressPercent] = useState(0)
  const [showBrowser, setShowBrowser] = useState(false)
  const listRef = useRef<HTMLDivElement>(null)

  const refreshDevices = useCallback(async () => {
    setDevicesLoading(true)
    try {
      const list = await invoke<AdbDevice[]>("list_adb_devices")
      setDevices(list)
      if (list.length > 0 && !list.some((d) => d.serial === selectedDevice)) {
        setSelectedDevice(list[0].serial)
      }
      if (list.length === 0) {
        setSelectedDevice(null)
      }
    } catch {
      setDevices([])
      setSelectedDevice(null)
    } finally {
      setDevicesLoading(false)
    }
  }, [selectedDevice])

  useEffect(() => {
    refreshDevices()
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight
    }
  }, [completedItems])

  async function pickSourceDir() {
    const selected = await open({ directory: true })
    if (selected) {
      setSourceDir(selected as string)
      try {
        const count = await invoke<number>("count_image_files", { directory: selected })
        setFileCount(count)
      } catch {
        setFileCount(null)
      }
    }
  }

  async function handlePush() {
    if (!sourceDir || !targetDir || !selectedDevice) return
    setFormStatus("running")
    setPhase("scanning")
    setErrorMsg("")
    setCompletedItems([])
    setCurrentFile(null)
    setProgressPercent(0)

    const unlistenPhase = await listen<PhaseEvent>("adb-push-phase", (event) => {
      setPhase(event.payload.phase)
    })

    const unlisten = await listen<AdbPushProgress>("adb-push-progress", (event) => {
      const p = event.payload
      setPhase("processing")
      if (p.status === "pushing") {
        setCurrentFile(p)
        setProgressPercent(Math.round(((p.current - 1) / p.total) * 100))
      } else {
        setCurrentFile(null)
        setCompletedItems((prev) => [...prev, p])
        setProgressPercent(Math.round((p.current / p.total) * 100))
      }
    })

    try {
      await invoke("adb_push_directory", {
        serial: selectedDevice,
        sourceDir,
        targetDir,
      })
      setFormStatus("done")
    } catch (e) {
      setFormStatus("error")
      setErrorMsg(String(e))
    } finally {
      unlisten()
      unlistenPhase()
      setPhase("idle")
    }
  }

  function handleBrowseSelect(path: string) {
    setTargetDir(path.endsWith("/") ? path : path + "/")
    setShowBrowser(false)
  }

  const noDevice = devices.length === 0

  return (
    <Card>
      <CardContent className="space-y-4 pt-6">
        {/* Device selector */}
        <div className="space-y-1">
          <label className="text-sm text-muted-foreground">{t("adb.device")}</label>
          <div className="flex items-center gap-2">
            <Smartphone className="h-4 w-4 text-muted-foreground shrink-0" />
            <select
              value={selectedDevice ?? ""}
              onChange={(e) => setSelectedDevice(e.target.value || null)}
              disabled={formStatus === "running" || noDevice}
              className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-xs transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:opacity-50"
            >
              {noDevice && <option value="">{t("adb.noDevice")}</option>}
              {devices.map((d) => (
                <option key={d.serial} value={d.serial}>
                  {d.model} ({d.serial})
                </option>
              ))}
            </select>
            <Button
              variant="outline"
              size="icon"
              onClick={refreshDevices}
              disabled={devicesLoading || formStatus === "running"}
              className="shrink-0"
              title={t("adb.refreshDevices")}
            >
              <RefreshCw className={`h-4 w-4 ${devicesLoading ? "animate-spin" : ""}`} />
            </Button>
          </div>
        </div>

        {/* Source directory */}
        <div className="space-y-1">
          <Button
            variant="outline"
            onClick={pickSourceDir}
            disabled={formStatus === "running"}
            className="w-full justify-start"
          >
            <Folder className="mr-2 h-4 w-4" />
            {sourceDir ? sourceDir : t("adb.selectSource")}
          </Button>
          {sourceDir && fileCount !== null && (
            <p
              className={`text-xs px-1 ${fileCount === 0 ? "text-yellow-600 dark:text-yellow-400" : "text-muted-foreground"}`}
            >
              {fileCount === 0 ? t("adb.noFiles") : t("adb.fileCount", { count: fileCount })}
            </p>
          )}
        </div>

        {/* Target path */}
        <div className="space-y-1">
          <label className="text-sm text-muted-foreground">{t("adb.targetPath")}</label>
          <div className="flex items-center gap-2">
            <input
              type="text"
              value={targetDir}
              onChange={(e) => setTargetDir(e.target.value)}
              disabled={formStatus === "running"}
              className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-xs transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:opacity-50"
            />
            <Button
              variant="outline"
              size="icon"
              onClick={() => setShowBrowser(!showBrowser)}
              disabled={formStatus === "running" || !selectedDevice}
              className="shrink-0"
              title={t("adb.browse")}
            >
              <FolderOpen className="h-4 w-4" />
            </Button>
          </div>
        </div>

        {/* Device folder browser */}
        {showBrowser && selectedDevice && (
          <DeviceFolderBrowser
            serial={selectedDevice}
            onSelect={handleBrowseSelect}
            onCancel={() => setShowBrowser(false)}
          />
        )}

        {/* Action */}
        <Button
          onClick={handlePush}
          disabled={!sourceDir || !targetDir || !selectedDevice || formStatus === "running"}
          className="w-full"
        >
          {formStatus === "running" && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
          {formStatus === "running" ? t("adb.pushing") : t("adb.push")}
        </Button>

        {/* Progress */}
        {formStatus === "running" && phase === "scanning" && (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Loader2 className="h-4 w-4 animate-spin shrink-0" />
            <span>{t("adb.scanning")}</span>
          </div>
        )}
        {formStatus === "running" && phase === "processing" && (
          <div className="space-y-2">
            <div className="flex items-center gap-2">
              <Progress value={progressPercent} className="flex-1" />
              <span className="text-xs text-muted-foreground tabular-nums shrink-0">
                {progressPercent}%
              </span>
            </div>
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
            <Check className="h-4 w-4" />
            {t("adb.complete", {
              succeeded: completedItems.filter((i) => i.status === "done").length,
              total: completedItems.length,
            })}
          </div>
        )}
        {formStatus === "error" && (
          <div className="text-sm text-destructive flex items-center gap-2">
            <X className="h-4 w-4" /> {errorMsg}
          </div>
        )}

        {/* Completed list */}
        {completedItems.length > 0 && (
          <div ref={listRef} className="max-h-48 overflow-y-auto space-y-1 text-xs">
            {completedItems.map((item, i) => (
              <div key={i} className="flex items-center gap-2">
                {item.status === "done" ? (
                  <Badge variant="secondary" className="text-success">
                    {t("adb.ok")}
                  </Badge>
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

function DeviceFolderBrowser({
  serial,
  onSelect,
  onCancel,
}: {
  serial: string
  onSelect: (path: string) => void
  onCancel: () => void
}) {
  const { t } = useTranslation()
  const [tree, setTree] = useState<DirNode[]>([])
  const [loading, setLoading] = useState(true)
  const [selectedPath, setSelectedPath] = useState("/sdcard/")

  useEffect(() => {
    loadRoot()
  }, [serial]) // eslint-disable-line react-hooks/exhaustive-deps

  async function loadRoot() {
    setLoading(true)
    try {
      const dirs = await invoke<DeviceDir[]>("list_device_dirs", {
        serial,
        path: "/sdcard/",
      })
      setTree(
        dirs.map((d) => ({
          name: d.name,
          path: `/sdcard/${d.name}`,
          children: null,
          expanded: false,
        }))
      )
    } catch {
      setTree([])
    } finally {
      setLoading(false)
    }
  }

  async function toggleNode(node: DirNode, path: number[]) {
    if (node.expanded) {
      // Collapse
      updateNode(path, { ...node, expanded: false })
      return
    }

    // Expand and load children if needed
    if (node.children === null) {
      const dirs = await invoke<DeviceDir[]>("list_device_dirs", {
        serial,
        path: node.path + "/",
      })
      const children: DirNode[] = dirs.map((d) => ({
        name: d.name,
        path: `${node.path}/${d.name}`,
        children: null,
        expanded: false,
      }))
      updateNode(path, { ...node, children, expanded: true })
    } else {
      updateNode(path, { ...node, expanded: true })
    }
  }

  function updateNode(path: number[], newNode: DirNode) {
    setTree((prev) => {
      const next = [...prev]
      let current = next
      for (let i = 0; i < path.length - 1; i++) {
        const idx = path[i]
        const parent = { ...current[idx] }
        parent.children = [...(parent.children || [])]
        current[idx] = parent
        current = parent.children
      }
      current[path[path.length - 1]] = newNode
      return next
    })
  }

  function renderTree(nodes: DirNode[], depth: number, path: number[]) {
    return nodes.map((node, idx) => {
      const currentPath = [...path, idx]
      const isSelected = selectedPath === node.path
      return (
        <div key={node.path}>
          <div
            className={`flex items-center gap-1 py-0.5 px-1 rounded cursor-pointer hover:bg-accent text-sm ${
              isSelected ? "bg-accent" : ""
            }`}
            style={{ paddingLeft: `${depth * 16 + 4}px` }}
            onClick={() => {
              setSelectedPath(node.path)
              toggleNode(node, currentPath)
            }}
          >
            {node.expanded ? (
              <ChevronDown className="h-3 w-3 shrink-0 text-muted-foreground" />
            ) : (
              <ChevronRight className="h-3 w-3 shrink-0 text-muted-foreground" />
            )}
            <Folder className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
            <span className="truncate">{node.name}</span>
          </div>
          {node.expanded && node.children && renderTree(node.children, depth + 1, currentPath)}
        </div>
      )
    })
  }

  return (
    <div className="border rounded-md p-2 space-y-2">
      <div className="text-xs text-muted-foreground font-medium">/sdcard/</div>
      <div className="max-h-48 overflow-y-auto">
        {loading ? (
          <div className="flex items-center gap-2 text-sm text-muted-foreground p-2">
            <Loader2 className="h-4 w-4 animate-spin" />
            {t("adb.loadingDirs")}
          </div>
        ) : tree.length === 0 ? (
          <div className="text-sm text-muted-foreground p-2">{t("adb.noDirs")}</div>
        ) : (
          renderTree(tree, 0, [])
        )}
      </div>
      <div className="flex items-center gap-2 pt-1 border-t">
        <span className="text-xs text-muted-foreground truncate flex-1">{selectedPath}/</span>
        <Button variant="ghost" size="sm" onClick={onCancel}>
          {t("adb.cancel")}
        </Button>
        <Button size="sm" onClick={() => onSelect(selectedPath)}>
          {t("adb.select")}
        </Button>
      </div>
    </div>
  )
}

function fileName(path: string): string {
  return path.split(/[/\\]/).pop() || path
}
