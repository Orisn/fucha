import { useCallback, useEffect, useState } from "react";
import { browserIntegration } from "@/lib/mdm/client";
import type { BrowserIntegration } from "@/lib/mdm/types";

export function useBrowserIntegration() {
  const [integration, setIntegration] = useState<BrowserIntegration | null>(
    null,
  );

  const refresh = useCallback(async () => {
    try {
      setIntegration(await browserIntegration());
    } catch {
      // The local API is unreachable; keep the last known state.
    }
  }, []);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => {
      if (!document.hidden) void refresh();
    }, 10_000);
    const onFocus = () => void refresh();
    window.addEventListener("focus", onFocus);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("focus", onFocus);
    };
  }, [refresh]);

  return { integration, refresh };
}
