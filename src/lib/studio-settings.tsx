import {
  createContext,
  useContext,
  useState,
  type ReactNode,
} from "react";

export type MdmMode = "background" | "window";

export interface StudioSettings {
  downloadDir: string;
  mode: MdmMode;
}

interface StudioSettingsValue {
  settings: StudioSettings;
  save: (next: StudioSettings) => void;
}

const STORAGE_KEY = "fucha.settings";
const LEGACY_KEY = "mdm-studio.settings";
const DEFAULTS: StudioSettings = { downloadDir: "", mode: "background" };
const Context = createContext<StudioSettingsValue | null>(null);

function load(): StudioSettings {
  try {
    const raw =
      localStorage.getItem(STORAGE_KEY) ?? localStorage.getItem(LEGACY_KEY);
    if (!raw) return DEFAULTS;
    const parsed = JSON.parse(raw) as Partial<StudioSettings>;
    return {
      downloadDir:
        typeof parsed.downloadDir === "string" ? parsed.downloadDir : "",
      mode: parsed.mode === "window" ? "window" : "background",
    };
  } catch {
    return DEFAULTS;
  }
}

export function StudioSettingsProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<StudioSettings>(load);

  const save = (next: StudioSettings) => {
    setSettings(next);
    localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
    localStorage.removeItem(LEGACY_KEY);
  };

  return (
    <Context.Provider value={{ settings, save }}>{children}</Context.Provider>
  );
}

export function useStudioSettings() {
  const value = useContext(Context);
  if (!value) {
    throw new Error("useStudioSettings must be used inside StudioSettingsProvider");
  }
  return value;
}
