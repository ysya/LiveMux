import { useEffect, useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import { useTranslation } from "react-i18next"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { MuxForm } from "@/components/MuxForm"
import { DirForm } from "@/components/DirForm"
import { AdbPushForm } from "@/components/AdbPushForm"
import { Toaster } from "@/components/ui/sonner"
import { Button } from "@/components/ui/button"

function App() {
  const { t, i18n } = useTranslation()
  const [exiftoolOk, setExiftoolOk] = useState<boolean | null>(null)
  const [ffmpegOk, setFfmpegOk] = useState<boolean | null>(null)

  useEffect(() => {
    invoke("check_exiftool")
      .then(() => setExiftoolOk(true))
      .catch(() => setExiftoolOk(false))
    invoke<boolean>("check_ffmpeg")
      .then((ok) => setFfmpegOk(ok))
      .catch(() => setFfmpegOk(false))
  }, [])

  function toggleLanguage() {
    const next = i18n.language === "zh-TW" ? "en" : "zh-TW"
    i18n.changeLanguage(next)
  }

  return (
    <div className="p-6 max-w-xl mx-auto">
      <div className="flex justify-end mb-2">
        <Button variant="ghost" size="sm" onClick={toggleLanguage}>
          {i18n.language === "zh-TW" ? "EN" : "中文"}
        </Button>
      </div>

      {exiftoolOk === false && (
        <div className="mb-4 p-3 rounded-md bg-destructive/10 text-destructive text-sm">
          {t("app.exiftoolError")}
        </div>
      )}

      {ffmpegOk === false && (
        <div className="mb-4 p-3 rounded-md bg-yellow-500/10 text-yellow-600 dark:text-yellow-400 text-sm">
          {t("app.ffmpegWarning")}
        </div>
      )}

      <Tabs defaultValue="mux">
        <TabsList className="mb-4">
          <TabsTrigger value="mux">{t("app.tabMux")}</TabsTrigger>
          <TabsTrigger value="dir">{t("app.tabBatch")}</TabsTrigger>
          <TabsTrigger value="adb">{t("app.tabAdb")}</TabsTrigger>
        </TabsList>
        <TabsContent value="mux">
          <MuxForm />
        </TabsContent>
        <TabsContent value="dir">
          <DirForm />
        </TabsContent>
        <TabsContent value="adb">
          <AdbPushForm />
        </TabsContent>
      </Tabs>

      <Toaster />
    </div>
  )
}

export default App
