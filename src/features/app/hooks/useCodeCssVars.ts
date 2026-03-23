import { useEffect } from "react";
import type { AppSettings } from "../../../types";

export function useCodeCssVars(appSettings: AppSettings) {
  useEffect(() => {
    if (typeof document === "undefined") {
      return;
    }
    const root = document.documentElement;
    root.style.setProperty("--chat-font-family", appSettings.chatFontFamily);
    root.style.setProperty("--code-font-family", appSettings.codeFontFamily);
    root.style.setProperty("--code-font-size", `${appSettings.codeFontSize}px`);
  }, [appSettings.chatFontFamily, appSettings.codeFontFamily, appSettings.codeFontSize]);
}
