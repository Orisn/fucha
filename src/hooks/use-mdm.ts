import { useCallback, useEffect, useRef, useState } from "react";
import { fetchList, isMock, onMdmList } from "@/lib/mdm/client";
import type { MdmList } from "@/lib/mdm/types";

/** How often the list is fetched besides the core's pushes: a safety net for
 * a missed event, not the way the list updates. The mock has no events, so
 * it polls as the old client did. */
const FALLBACK_MS = isMock ? 1_000 : 5_000;

export function useMdm() {
  const [data, setData] = useState<MdmList | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [connected, setConnected] = useState<boolean | null>(null);
  const refreshing = useRef(false);

  const show = useCallback((list: MdmList) => {
    setData(list);
    setError(null);
    setConnected(true);
  }, []);

  const refresh = useCallback(async () => {
    if (refreshing.current) return;
    refreshing.current = true;
    try {
      show(await fetchList());
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      setConnected(false);
    } finally {
      refreshing.current = false;
    }
  }, [show]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    // The core pushes a snapshot at most four times a second; the fallback
    // fetch only covers a missed or unsupported event.
    void onMdmList((list) => {
      if (!cancelled) show(list);
    }).then((stop) => {
      if (cancelled) stop();
      else unlisten = stop;
    });

    const refreshIfVisible = () => {
      if (!document.hidden) void refresh();
    };

    void refreshIfVisible();
    const timer = window.setInterval(refreshIfVisible, FALLBACK_MS);
    const onFocus = () => refreshIfVisible();
    document.addEventListener("visibilitychange", refreshIfVisible);
    window.addEventListener("focus", onFocus);

    return () => {
      cancelled = true;
      unlisten?.();
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", refreshIfVisible);
      window.removeEventListener("focus", onFocus);
    };
  }, [refresh, show]);

  return { data, error, connected, refresh };
}
