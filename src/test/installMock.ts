// ---- Phase 3D-2: the installation commands. A stand-in with the wire shapes of src-tauri/src/install_flow.rs. The REAL consent, freshness,
// replay, concurrency, rollback and recovery behaviour is covered by the Rust tests; this only lets the interface be tested.
import type { InstallAction, InstalledDetails, InstallPreview, InstallResult, ToolState } from "../types";

export interface InstallMock {
  states: Map<string, ToolState>;
  offers: Map<string, { entryId: string; action: InstallAction; digest: string }>;
  /** When set, the next confirmation returns this instead of the default answer. */
  nextConfirm: InstallResult | null;
  expiresInSeconds: number;
  counter: number;
}

export const incompleteState = (entryId: string): ToolState => ({
  entryId, status: "verification_incomplete", version: null, installedVersion: null, pendingAction: null, offerExpiresAt: null, synthetic: false,
  canInstall: false, canRemove: false, reasons: [{ code: "no_reviewed_recipe", text: "Bu araç için incelenmiş ve onaylanmış bir kurulum tarifi yok." }],
});

export const toolState = (entryId: string, status: ToolState["status"], over: Partial<ToolState> = {}): ToolState => ({
  entryId, status, reasons: [], version: "1.0.0", installedVersion: status === "installed_verified" ? "1.0.0" : null, pendingAction: null, offerExpiresAt: null,
  synthetic: true, canInstall: status === "available" || status === "removed", canRemove: status === "installed_verified", ...over,
});

export const createInstallMock = (): InstallMock => ({ states: new Map(), offers: new Map(), nextConfirm: null, expiresInSeconds: 120, counter: 0 });

export const SHA = "ab".repeat(32);

export const makePreview = (m: InstallMock, entryId: string, action: InstallAction): InstallPreview => {
  m.counter += 1;
  const offerId = String(m.counter).padStart(64, "0");
  const digest = String(m.counter + 1000).padStart(64, "f");
  m.offers.set(offerId, { entryId, action, digest });
  const removal = action === "uninstall";
  return {
    offerId, createdAt: new Date().toISOString(), expiresAt: new Date(Date.now() + m.expiresInSeconds * 1000).toISOString(), previewDigest: digest,
    content: {
      action, entryId, displayName: `Sentetik ${entryId}`, version: "1.0.0", platformTarget: "windows-x64", architecture: "x64",
      source: { repository: "synthetic/example", immutableCommit: "0123456789abcdef0123456789abcdef01234567", sourceAttested: true },
      artifact: { fileName: "artifact.bin", sizeBytes: 123, sha256: SHA, format: "pe", digestScope: "exact_file" },
      authentication: "authenticated_reference", licenseState: "accepted", licenseSpdx: "MIT",
      notices: { title: "SENTETİK bildirim paketi (deneme)", sha256: "cd".repeat(32), components: ["synthetic-hello - MIT (sentetik)", "synthetic-runtime - GPL WITH exception (sentetik)"], unresolved: ["Bu bir deneme paketidir."] },
      effects: removal
        ? [
            { place: "locks/synthetic-hello.lock", change: "create", remains: false, text: "Kilit; bitince silinir." },
            { place: "state/synthetic-hello.json", change: "remove", remains: false, text: "Etkin sürüm işareti kaldırılır." },
            { place: "versions/synthetic-hello/1.0.0/", change: "remove", remains: false, text: "Sürüm klasörü kaldırılır." },
            { place: "data/synthetic-hello/", change: "read", remains: true, text: "Aracın kendi kullanıcı verisi: dokunulmaz." },
          ]
        : [
            { place: "(yöneticinin kendi klasörü, uygulama veri klasörünün içinde)", change: "create", remains: true, text: "İlk kullanımda oluşturulur." },
            { place: "locks/synthetic-hello.lock", change: "create", remains: false, text: "Kilit; bitince silinir." },
            { place: "versions/synthetic-hello/1.0.0/artifact.bin", change: "create", remains: true, text: "Kurulan dosyanın kendisi." },
            { place: "state/synthetic-hello.json", change: "create", remains: true, text: "Etkin sürüm işareti." },
            { place: "receipts/<işlem-kimliği>.json", change: "create", remains: true, text: "Kurulum makbuzu." },
            { place: "data/synthetic-hello/", change: "read", remains: true, text: "Aracın kendi kullanıcı verisi: dokunulmaz." },
          ],
      notDone: ["Hiçbir program çalıştırılmaz; kurulan dosya da çalıştırılmaz.", "Ağa bağlanılmaz, hiçbir şey indirilmez.", "PATH (program arama yolu) değiştirilmez.", "Hiçbir ajanın ayar dosyası değiştirilmez."],
      networkUsed: false,
      rollback: { reliability: "simulated", undoes: ["pointer_restore", "staging_removal"], doesNotUndo: ["tool_user_data", "external_effects"], text: "Geri alma sözleşmesi yalnızca SENTETİK denemelerle sınandı." },
      synthetic: true, installedVersion: removal ? "1.0.0" : null, managerFolderExists: removal, planId: "1".repeat(64), recipeId: "r", recipeDigest: "2".repeat(64), effectsDigest: "3".repeat(64),
      consentStatement: "\"Onaylıyorum\" düğmesi bu işlemi uygulamaya yaptırır. Bu uygulama içi bir onaydır: kimlik kanıtı DEĞİLDİR ve aynı Windows hesabında çalışan başka bir programa karşı koruma sağlamaz. Onay tek kullanımlıktır.",
    },
  };
};

export const refused = (code: string, message: string): InstallResult => ({ outcome: "refused", action: null, entryId: null, version: null, code, message, receipt: null, rollback: null, state: null });

const receipt = (action: "install" | "uninstall") => ({ txnId: `txn-${"a".repeat(32)}`, action, version: "1.0.0", completedAt: "2027-01-01T12:00:00Z", artifactSha256: action === "install" ? SHA : null, kept: ["user_data"] });

export function installCommand(b: { entries: { entryId: string; removedAt: string | null }[]; install: InstallMock }, cmd: string, args?: Record<string, unknown>): { handled: boolean; value?: unknown } {
  const m = b.install;
  const stateOf = (id: string): ToolState => {
    const base = m.states.get(id) ?? incompleteState(id);
    const live = [...m.offers.values()].find((o) => o.entryId === id);
    return live && ["available", "removed", "installed_verified"].includes(base.status) ? { ...base, status: "ready_for_approval", pendingAction: live.action } : { ...base };
  };
  switch (cmd) {
    case "list_install_states": return { handled: true, value: b.entries.filter((e) => e.removedAt === null).map((e) => stateOf(e.entryId)) };
    case "prepare_install": {
      const r = args?.request as { entryId: string; action: InstallAction };
      const s = m.states.get(r.entryId) ?? incompleteState(r.entryId);
      if ((r.action === "install" && !s.canInstall) || (r.action === "uninstall" && !s.canRemove)) {
        throw { code: "INSTALL_NOT_AVAILABLE", message: "Bu araç için kurulum yapılamaz: doğrulama tamamlanmadı." };
      }
      // a newer preview for the same tool replaces the older one
      for (const [k, o] of m.offers) if (o.entryId === r.entryId) m.offers.delete(k);
      return { handled: true, value: makePreview(m, r.entryId, r.action) };
    }
    case "cancel_install": {
      const id = (args?.request as { offerId: string }).offerId;
      if (!m.offers.delete(id)) throw { code: "INSTALL_OFFER_UNKNOWN", message: "Bu onay isteği tanınmadı. Önizlemeyi yeniden açın." };
      return { handled: true, value: undefined };
    }
    case "confirm_install": {
      const r = args?.request as { offerId: string; previewDigest: string; acknowledged: boolean };
      const offer = m.offers.get(r.offerId);
      if (!offer) return { handled: true, value: refused("consent_unknown", "Bu onay isteği tanınmadı. Önizlemeyi yeniden açın.") };
      m.offers.delete(r.offerId);
      if (!r.acknowledged) return { handled: true, value: refused("consent_not_acknowledged", "Açık onay verilmedi. Hiçbir şey yapılmadı.") };
      if (r.previewDigest !== offer.digest) return { handled: true, value: refused("consent_preview_changed", "Onaylanan önizleme, uygulamanın elindeki önizlemeyle aynı değil. Hiçbir şey yapılmadı.") };
      const done = (state: ToolState, result: InstallResult) => { m.states.set(offer.entryId, state); return { handled: true, value: { ...result, state } }; };
      if (m.nextConfirm) {
        const forced = m.nextConfirm;
        m.nextConfirm = null;
        return { handled: true, value: forced };
      }
      if (offer.action === "install") {
        return done(toolState(offer.entryId, "installed_verified"), { outcome: "completed", action: "install", entryId: offer.entryId, version: "1.0.0", code: null, message: "Kurulum tamamlandı ve doğrulandı. Hiçbir program çalıştırılmadı.", receipt: receipt("install"), rollback: null, state: null });
      }
      return done(toolState(offer.entryId, "removed"), { outcome: "completed", action: "uninstall", entryId: offer.entryId, version: "1.0.0", code: null, message: "Kaldırma tamamlandı. Aracın kendi kullanıcı verisine dokunulmadı.", receipt: receipt("uninstall"), rollback: null, state: null });
    }
    case "get_installed_details": {
      const id = String(args?.entryId);
      const s = stateOf(id);
      const installed = s.status === "installed_verified";
      const d: InstalledDetails = {
        entryId: id, displayName: `Sentetik ${id}`, state: s, version: installed ? "1.0.0" : null,
        artifact: installed ? { fileName: "artifact.bin", sizeBytes: 123, sha256: SHA, format: "pe", digestScope: "exact_file" } : null,
        location: installed ? `versions/${id}/1.0.0/artifact.bin` : null, receipt: installed ? receipt("install") : null,
        checks: installed ? ["Etkin sürüm işareti eşleşiyor.", "Kurulum makbuzu geçerli.", "Dosya özeti kayıtlı özetle aynı.", "Sürüm klasöründe başka dosya yok.", "Dosya kayıtlı dosyanın ta kendisi."].map((text) => ({ text, passed: true })) : [],
        history: [receipt("install")], notices: { title: "SENTETİK bildirim paketi (deneme)", sha256: "cd".repeat(32), components: ["synthetic-hello - MIT (sentetik)"], unresolved: ["Bu bir deneme paketidir."], text: "SYNTHETIC NOTICE BUNDLE" },
        kept: installed ? ["user_data"] : [], never: ["Hiçbir program çalıştırılmaz; kurulan dosya da çalıştırılmaz.", "PATH (program arama yolu) değiştirilmez."], lastResult: null,
      };
      return { handled: true, value: d };
    }
    case "recover_installations": {
      for (const [id, s] of m.states) if (s.status === "failed_needs_attention") m.states.set(id, toolState(id, "available"));
      return { handled: true, value: { examined: true, rolledBack: 1, rolledForward: 0, needsAttention: 0, notes: [] } };
    }
    default: return { handled: false };
  }
}
