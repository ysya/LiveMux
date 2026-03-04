import i18n from "i18next"
import { initReactI18next } from "react-i18next"
import en from "./en.json"
import zhTW from "./zh-TW.json"

// Detect browser language, map zh-* variants to zh-TW
function detectLanguage(): string {
  const lang = navigator.language
  if (lang.startsWith("zh")) return "zh-TW"
  return "en"
}

i18n.use(initReactI18next).init({
  resources: {
    en: { translation: en },
    "zh-TW": { translation: zhTW },
  },
  lng: detectLanguage(),
  fallbackLng: "en",
  interpolation: {
    escapeValue: false,
  },
})

export default i18n
