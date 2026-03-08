import { useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import { open } from "@tauri-apps/plugin-dialog"
import { useTranslation } from "react-i18next"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import { Badge } from "@/components/ui/badge"
import { FileImage, FileVideo, Loader2, Check, X } from "lucide-react"
import type { MuxConfig } from "@/types/livemux"

export function MuxForm() {
  const { t } = useTranslation()
  const [imagePath, setImagePath] = useState<string | null>(null)
  const [videoPath, setVideoPath] = useState<string | null>(null)
  const [overwrite, setOverwrite] = useState(false)
  const [deleteVideo, setDeleteVideo] = useState(false)
  const [keepTemp, setKeepTemp] = useState(false)
  const [noXmp, setNoXmp] = useState(false)
  const [status, setStatus] = useState<"idle" | "running" | "success" | "error">("idle")
  const [errorMsg, setErrorMsg] = useState("")

  async function pickImage() {
    const selected = await open({
      multiple: false,
      filters: [{ name: t("mux.filterImages"), extensions: ["heic", "heif", "avif", "jpg", "jpeg"] }],
    })
    if (selected) setImagePath(selected as string)
  }

  async function pickVideo() {
    const selected = await open({
      multiple: false,
      filters: [{ name: t("mux.filterVideos"), extensions: ["mov", "mp4"] }],
    })
    if (selected) setVideoPath(selected as string)
  }

  async function handleMux() {
    if (!imagePath || !videoPath) return
    setStatus("running")
    setErrorMsg("")

    const config: MuxConfig = {
      image_path: imagePath,
      video_path: videoPath,
      output_path: null,
      output_directory: null,
      delete_video: deleteVideo,
      delete_temp: !keepTemp,
      overwrite,
      no_xmp: noXmp,
    }

    try {
      await invoke("mux_single", { config })
      setStatus("success")
    } catch (e) {
      setStatus("error")
      setErrorMsg(String(e))
    }
  }

  return (
    <Card>
      <CardContent className="space-y-4 pt-6">
        {/* File pickers */}
        <div className="space-y-2">
          <div className="flex items-center gap-2">
            <Button variant="outline" onClick={pickImage} className="flex-1 justify-start">
              <FileImage className="mr-2 h-4 w-4" />
              {imagePath ? fileName(imagePath) : t("mux.selectImage")}
            </Button>
            {imagePath && <Badge variant="secondary">HEIC/JPG</Badge>}
          </div>
          <div className="flex items-center gap-2">
            <Button variant="outline" onClick={pickVideo} className="flex-1 justify-start">
              <FileVideo className="mr-2 h-4 w-4" />
              {videoPath ? fileName(videoPath) : t("mux.selectVideo")}
            </Button>
            {videoPath && <Badge variant="secondary">MOV/MP4</Badge>}
          </div>
        </div>

        {/* Options */}
        <div className="grid grid-cols-2 gap-3">
          <OptionCheckbox
            checked={overwrite}
            onChange={setOverwrite}
            label={t("mux.overwrite")}
            description={t("mux.overwriteDesc")}
          />
          <OptionCheckbox
            checked={deleteVideo}
            onChange={setDeleteVideo}
            label={t("mux.deleteVideo")}
            description={t("mux.deleteVideoDesc")}
          />
          <OptionCheckbox
            checked={keepTemp}
            onChange={setKeepTemp}
            label={t("mux.keepTemp")}
            description={t("mux.keepTempDesc")}
          />
          <OptionCheckbox
            checked={noXmp}
            onChange={setNoXmp}
            label={t("mux.skipXmp")}
            description={t("mux.skipXmpDesc")}
          />
        </div>

        {/* Action */}
        <Button
          onClick={handleMux}
          disabled={!imagePath || !videoPath || status === "running"}
          className="w-full"
        >
          {status === "running" && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
          {status === "running" ? t("mux.processing") : t("mux.mux")}
        </Button>

        {/* Result */}
        {status === "success" && (
          <div className="flex items-center gap-2 text-sm text-success">
            <Check className="h-4 w-4" /> {t("mux.done")}
          </div>
        )}
        {status === "error" && (
          <div className="flex items-center gap-2 text-sm text-destructive">
            <X className="h-4 w-4" /> {errorMsg}
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
