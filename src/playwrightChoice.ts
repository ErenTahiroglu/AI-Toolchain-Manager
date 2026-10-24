// CATALOG-02 - the "Playwright seçenekleri" comparison: three SEPARATE catalog candidates that the publisher presents as three ways to use Playwright.
// Pure data and logic only (no I/O, no React). Nothing here installs, enables, saves or connects to anything: the comparison explains, and its one
// action hands an exact target + variant to the Project Tool Workspace, which stays the only surface that writes a preference.
//
// The wording is the application's own plain-language reading of the pinned publisher sources recorded in docs/catalog-02/SOURCE_EVIDENCE.md. It
// states what the publisher says, never that anything works, is installed, is verified or is supported on a given target.
import type { CatalogEntry } from "./types";

export type PlaywrightVariantId = "microsoft-playwright-cli" | "microsoft-playwright" | "microsoft-playwright-mcp";

export interface PlaywrightVariant {
  entryId: PlaywrightVariantId;
  /** Short name used in buttons. */
  short: string;
  title: string;
  /** Set for the one option the publisher's own documents position as the starting point for coding agents. */
  suggestedForCodingAgents: boolean;
  /** For whom / when this option fits better than the other two. */
  when: string;
  /** What to watch out for: privacy, state, cost, scope. */
  watch: string;
}

/** The order the user asked for: CLI + Skills, Test / Library, MCP. */
export const PLAYWRIGHT_VARIANTS: readonly PlaywrightVariant[] = [
  {
    entryId: "microsoft-playwright-cli",
    short: "CLI + Skills",
    title: "CLI + Skills — kodlama ajanı için kısa komutlar",
    suggestedForCodingAgents: true,
    when:
      "Claude Code gibi bir kodlama ajanının birçok depoda kısa tarayıcı işleri yapmasını istediğinizde: bir sayfayı açmak, bakmak, tıklamak, ekran görüntüsü almak. " +
      "Yayıncıya göre komutlar, ajanın bağlamına büyük araç şemaları ve sayfa ağaçları yüklemediği için MCP’den daha az bağlam harcar (bu uygulama ölçmedi).",
    watch:
      "Tarayıcı oturumları çerez ve oturum durumunu tutabilir; “kalıcı” seçenek profili diske yazar. Ortak bir kurulum, ortak ya da ayrı bir tarayıcı profili demek değildir: " +
      "bu, oturumların nasıl adlandırıldığına bağlıdır. Komutlar çerezleri okuyup değiştirebilir, sayfada kod çalıştırabilir, iz ve video kaydedebilir. " +
      "Beceri (skills) kurulumu veya herhangi bir komut bu uygulamada yetkilendirilmedi.",
  },
  {
    entryId: "microsoft-playwright",
    short: "Test / Library",
    title: "Test / Library — projenin kendi uçtan uca testleri",
    suggestedForCodingAgents: false,
    when:
      "Bir projenin içinde kendi tarayıcı testlerinizi yazıp sürdürmek istediğinizde. Test bağımlılıkları o projeye sabitlenir; makine genelindeki bir CLI bunun yerini tutmaz.",
    watch:
      "Test çalıştırmak tarayıcı açar, sitelere gider, ekran görüntüsü, iz ve oturum durumu dosyaları yazabilir ve tarayıcı çalışma zamanı indirebilir. " +
      "Test betikleri makinenizde kod olarak çalışır; güvenmediğiniz betikleri çalıştırmayın. Bu kayıt bir MCP sunucusu veya ajan aracı değildir.",
  },
  {
    entryId: "microsoft-playwright-mcp",
    short: "MCP",
    title: "MCP — ajana zengin, durumlu tarayıcı denetimi",
    suggestedForCodingAgents: false,
    when:
      "Ajanın sayfayı yapısal olarak görüp aynı tarayıcıda uzun süre durumlu, keşfe dayalı işler yapmasını istediğinizde: keşif gezintisi, kendini onaran testler, uzun otonom akışlar. " +
      "Yayıncıya göre bağlam ve jeton maliyeti CLI’dan yüksektir; kodlama ajanları için yayıncı önce CLI + Skills’i öneriyor.",
    watch:
      "Ajanın MCP yapılandırması gerekir; bu uygulama hiçbir ajan ayarına yazmaz. Sayfa özetleri ajanın bağlamına ve dolayısıyla model sağlayıcısına gider. " +
      "Varsayılan tarayıcı profili diskte kalıcıdır (açık oturumlar kalır); uzantı kipi mevcut tarayıcınızın açık oturumlarını kullanabilir. " +
      "Yayıncının Claude Code ve Antigravity yapılandırma notları belgedir, mimariye özel bir işlev testi değildir.",
  },
] as const;

export const FAMILY_IDS: readonly string[] = PLAYWRIGHT_VARIANTS.map((v) => v.entryId);

export const PLAYWRIGHT_INTRO =
  "Playwright, tarayıcı otomasyonu için yayıncının üç ayrı biçimde sunduğu bir araçtır ve katalogda üç ayrı aday kayıt olarak durur. " +
  "Hiçbiri kurulu değil, hiçbiri çalıştırılmadı, hiçbirine bağlanılmadı; bu bölüm hiçbir şey kurmaz, kaydetmez veya etkinleştirmez. " +
  "Birden fazlasını isteyebilirsiniz: aralarında teknik bir dışlama yoktur.";

export const SUGGESTED_NOTE =
  "Yayıncı belgelerinin kodlama ajanı tarayıcı işleri için başlangıç noktası olarak gösterdiği seçenek. Bu, “her depoda çalışır”, “kurulu” ya da “uyumluluğu doğrulandı” demek değildir ve kendiliğinden etkinleştirilmez.";

export const FUTURE_RUNTIME_NOTE =
  "Bu bir mimari öneridir; kurulu bir özellik değildir. İleride tek bir Playwright CLI çalışma zamanı birden çok proje arasında paylaşılabilir; " +
  "her proje kendi tercihini ayrı tutar ve tarayıcı oturumları ile profilleri ayrı kalır. Bu, projeler arasında çerez veya oturum paylaşımına izin vermez ve hiçbir dosya yazmaz. " +
  "Böyle bir paylaşım ancak ayrı bir aşamada, açık onay ve gerçek kurulum kapıları geçildikten sonra düşünülebilir.";

export const NEED_TARGET_MESSAGE =
  "Önce proje, işletim sistemi ve ajan yüzeyini seçin. Hiçbir şey açılmadı, hiçbir kayıt yapılmadı.";

export type OptionState =
  /** Not in the catalog at all. No other option is offered in its place. */
  | { kind: "missing" }
  | { kind: "present"; entry: CatalogEntry; removed: boolean; disabled: boolean; userOwned: boolean };

export function optionStateFor(entries: readonly CatalogEntry[], entryId: string): OptionState {
  const entry = entries.find((e) => e.entryId === entryId);
  if (!entry) return { kind: "missing" };
  return { kind: "present", entry, removed: entry.removedAt !== null, disabled: !entry.enabled, userOwned: entry.origin === "user" };
}

/** Plain facts about the catalog record itself (not about the target). Empty when it is an ordinary, enabled seed record. */
export function recordNotes(state: OptionState): string[] {
  if (state.kind === "missing") return ["Bu kayıt katalogda bulunamadı; yerine başka bir seçenek seçilmedi."];
  const notes: string[] = [];
  if (state.userOwned) {
    notes.push("Bu kayıt sizin eklediğiniz bir aday: uygulama bu depo için yayıncı künyesini uygulamadı, bu yüzden aşağıdaki açıklama kayda ait kanıt sayılmaz ve uyumluluk bilinmiyor.");
  }
  if (state.removed) {
    notes.push("Katalogdan kaldırılmış: yeni karar kaydedilemez; eski bir karar varsa yalnızca görülebilir veya sıfırlanabilir. Geri eklemek için aşağıdaki kayda bakın.");
  } else if (state.disabled) {
    notes.push("Katalogda kapalı: “kullanmak istiyorum” seçilemez; önce aşağıdaki kayıtta katalogda etkinleştirin. “Şimdi değil” ve “Reddet” seçilebilir.");
  }
  return notes;
}

export interface ChosenTarget {
  alias: string;
  platform: string;
  surface: string;
}

/** The target is usable only when ALL three parts are chosen; nothing is defaulted or guessed. */
export const isCompleteTarget = (t: { alias: string; platform: string; surface: string }): t is ChosenTarget => t.alias !== "" && t.platform !== "" && t.surface !== "";
