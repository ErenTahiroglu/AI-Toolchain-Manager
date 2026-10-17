import { useState } from "react";
import { api, toAppError } from "../api";
import { ErrorNotice, StatusBadge, useLoad } from "../components/common";
import { applyTheme, isThemeChoice } from "../theme";
import type { AppError, StartupState } from "../types";

const THEMES = [
  { value: "system", label: "Sistem ayarını kullan" },
  { value: "light", label: "Açık" },
  { value: "dark", label: "Koyu" },
] as const;

export function SettingsPage({ onSettingsChanged }: { onSettingsChanged?: () => void }) {
  const settings = useLoad(api.getSettings);
  const startup = useLoad(api.getStartupState);
  const info = useLoad(api.getAppInfo);
  const [error, setError] = useState<AppError | null>(null);
  const [saved, setSaved] = useState("");

  const change = async (key: string, value: string) => {
    setError(null);
    try {
      const next = await api.setSetting(key, value);
      settings.setData(next);
      if (key === "theme" && isThemeChoice(value)) applyTheme(value);
      setSaved("Kaydedildi.");
      onSettingsChanged?.();
    } catch (e) {
      setSaved("");
      setError(toAppError(e));
    }
  };

  const changeStartup = async (enabled: boolean) => {
    setError(null);
    try {
      startup.setData(await api.setStartupEnabled(enabled));
      setSaved(enabled ? "Başlangıçta çalıştırma açıldı." : "Başlangıçta çalıştırma kapatıldı.");
    } catch (e) {
      setSaved("");
      setError(toAppError(e));
      startup.reload(); // show the real OS state after a failure
    }
  };

  const s = settings.data;
  const st: StartupState | null = startup.data;
  const mismatch = st && st.osEnabled !== null && st.osEnabled !== st.preference;

  return (
    <>
      <div aria-live="polite" className="sr-live">{saved}</div>
      {error && <ErrorNotice error={error} />}
      {settings.error && <ErrorNotice error={settings.error} onRetry={settings.reload} />}

      <section aria-labelledby="h-theme">
        <h2 id="h-theme">Görünüm</h2>
        <fieldset disabled={!s}>
          <legend>Tema</legend>
          {THEMES.map((t) => (
            <label key={t.value} className="choice">
              <input type="radio" name="theme" value={t.value} checked={s?.theme === t.value} onChange={() => change("theme", t.value)} />
              {t.label}
            </label>
          ))}
        </fieldset>
      </section>

      <section aria-labelledby="h-window">
        <h2 id="h-window">Pencere ve başlangıç</h2>
        <label className="choice">
          <input
            type="checkbox" disabled={!s} checked={s?.close_to_tray === "true"}
            onChange={(e) => change("close_to_tray", e.target.checked ? "true" : "false")}
          />
          Pencereyi kapatınca uygulamayı sistem tepsisinde çalışır bırak
        </label>
        <p className="muted small">Tepsi simgesi oluşturulamazsa pencere kapatılınca uygulama normal şekilde kapanır.</p>

        <label className="choice">
          <input
            type="checkbox" disabled={!st} checked={st?.preference === true}
            onChange={(e) => changeStartup(e.target.checked)}
          />
          Bilgisayar açıldığında uygulamayı otomatik başlat
        </label>
        <p className="muted small">
          Varsayılan: kapalı. Yalnızca siz açarsanız işletim sistemine bu uygulamaya ait bir başlangıç kaydı eklenir; istediğiniz
          an kapatabilirsiniz.
          {st && (
            <>
              {" "}İşletim sistemi şu an: <strong>{st.osEnabled === null ? "okunamadı" : st.osEnabled ? "açık" : "kapalı"}</strong>.
            </>
          )}
        </p>
        {mismatch && (
          <p role="alert"><StatusBadge status="failed" /> Kayıtlı tercih ile işletim sistemi durumu uyuşmuyor. Anahtarı yeniden ayarlayabilirsiniz.</p>
        )}
      </section>

      <section aria-labelledby="h-privacy">
        <h2 id="h-privacy">Gizlilik</h2>
        <ul>
          <li>Telemetri yok. Bu sürüm hiçbir ağ isteği yapmaz.</li>
          <li>Kayıtlar yalnızca bu bilgisayarda tutulur; gizli değerler ve kişisel yollar silinir.</li>
          <li>Hiçbir araç kurulmaz, hiçbir ajan ayar dosyası değiştirilmez.</li>
        </ul>
      </section>

      <section aria-labelledby="h-about">
        <h2 id="h-about">Hakkında</h2>
        {info.error && <ErrorNotice error={info.error} onRetry={info.reload} />}
        {info.data && (
          <dl className="facts">
            <dt>Sürüm</dt><dd>{info.data.version} (Faz {info.data.phase})</dd>
            <dt>Sistem</dt><dd>{info.data.platform} / {info.data.arch}</dd>
            <dt>Veritabanı şema sürümü</dt><dd>{info.data.dbVersion ?? "kullanılamıyor"}</dd>
            <dt>Yerel veri klasörü</dt><dd>{info.data.dataDir}</dd>
          </dl>
        )}
      </section>
    </>
  );
}
