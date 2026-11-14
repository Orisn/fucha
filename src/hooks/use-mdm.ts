import { useCallback, useEffect, useRef, useState } from "react";
import { fetchList } from "@/lib/mdm/client";
import type { MdmList } from "@/lib/mdm/types";

export function useMdm() {
  const [data, setData] = useState<MdmList | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [connected, setConnected] = useState<boolean | null>(null);
  const refreshing = useRef(false);

  const refresh = useCallback(async () => {
    if (refreshing.current) return;
    refreshing.current = true;
    try {
      setData(await fetchList());
      setError(null);
      setConnected(true);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      setConnected(false);
    } finally {
      refreshing.current = false;
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    let timer: number | undefined;

    const loop = async () => {
      if (!document.hidden) await refresh();
      if (!cancelled) timer = window.setTimeout(loop, 1000);
    };
    void loop();

    return () => {
      cancelled = true;
      if (timer) window.clearTimeout(timer);
    };
  }, [refresh]);

  return { data, error, connected, refresh };
}
