// CONFIG-02C (Issue #40) - the PURE half of "Dosyadan oku": fixed Turkish wording and the strict acceptance of the one typed answer the backend may give.
// No IPC, no timer, no storage, no clipboard and no network here (those live in components/FileSnapshot.tsx and api.ts). Everything a user sees about the file is
// chosen from the closed tables below; nothing the response contains is ever shown as text unless it is one of the listed constants, a hex digest, a count or a size.
// The wording follows docs/config-02c/REAL_READ_AUTHORIZATION_REQUEST.md section 4 and must never promise more than the implementation does.

export const READ_CONSENT = "config02c-read-once-v1";
export const SNAPSHOT_LABEL = "dosya anlık görüntüsü (katı JSON sözleşmesi)";
export const INVENTORY_LABEL = "önceki envanter gözlemi (farklı kapsam/yöntem)";
export const SNAPSHOT_TTL_MS = 5 * 60 * 1000;
export const MAX_BYTES = 1_048_576;

export const STATUSES = [
  "root_not_found", "absent", "present_summarized", "present_metadata_only", "malformed", "denied", "unsafe_path", "unsupported_fs", "too_large", "changed_during_read", "io_error",
] as const;
export type SnapshotStatus = (typeof STATUSES)[number];

const REASONS: Record<SnapshotStatus, readonly string[]> = {
  root_not_found: [],
  absent: [],
  present_summarized: [],
  present_metadata_only: ["duplicate_key", "byte_order_mark", "root_not_object", "servers_not_object", "too_deep", "too_many_servers"],
  malformed: ["invalid_utf8", "invalid_json", "lone_surrogate", "empty_file"],
  denied: ["access_denied", "locked"],
  unsafe_path: ["link", "not_a_directory", "not_regular_file", "hard_linked", "case_alias", "invalid_root_text", "root_too_broad"],
  unsupported_fs: [],
  too_large: [],
  changed_during_read: [],
  io_error: [],
};

export const NOT_CLAIMS = ["not_loaded", "not_trusted", "not_installed", "not_enabled", "not_reachable", "not_verified"] as const;
const NAME_STATES = ["absent", "present", "present_other_letter_case"] as const;
const ENDPOINT_STATES = ["absent", "present_under_this_name", "present_under_other_name", "not_evaluable"] as const;
const WITH_BYTES: readonly SnapshotStatus[] = ["present_summarized", "present_metadata_only", "malformed"];

// ---- the gate text (shown before the button; verbatim from the reviewed request) ------------------------------------------------------

export const SECTION_TITLE = "Dosyadan oku (isteğe bağlı) — yalnızca .mcp.json, bir kez, salt okunur";
export const FOLDER_LABEL = "Proje klasörünün tam yolu (uygulama bunu kaydetmez; okuma bitince bu alandan temizlenir)";
export const FOLDER_HELP = "Yazdığınız yol, siz yazarken uygulama penceresinin belleğinde bulunur ve okuma için uygulamanın içine iletilir; bellekten güvenli silinmesi garanti edilmez.";
export const FOLDER_PLACEHOLDER = "C:\\Projeler\\sitem veya /Users/ad/sitem";
export const READ_BUTTON = "Bu klasördeki .mcp.json dosyasını bir kez oku";
export const DECLINE_BUTTON = "Hayır, metni yapıştırarak devam edeceğim";
export const CLEAR_BUTTON = "Temizle";
export const DECLINED_TEXT = "Dosya okunmadı. Yapıştırma önizlemesi her zamanki gibi kullanılabilir.";
export const NOT_REQUESTED_TEXT = "Henüz okunmadı. Hiçbir dosyaya bakılmadı.";
export const READING_TEXT = "Okunuyor…";
export const UNEXPECTED_TEXT = "Uygulamadan beklenmeyen bir yanıt geldi; hiçbir şey gösterilmedi.";
export const HOST_NOTE = "Okuma, bu bilgisayardaki klasörü okur; satırın işletim sistemi seçiminden bağımsızdır.";

export const GATE_LEAD = "Bu uygulama, yazdığınız klasördeki yalnızca .mcp.json dosyasına bir kez bakacak.";
export const GATE_POINTS: readonly { lead: string; text: string }[] = [
  {
    lead: "Okunacak dosya:",
    text: ".mcp.json — klasörün hemen altında. Başka dosyanın içeriği ve alt klasörler okunmaz. Klasöre ulaşmak için üst klasörlerin yalnızca türü ve bağlantı olup olmadığı denetlenir; içerikleri okunmaz. Klasördeki dosya adları, yalnızca .mcp.json adının harf büyüklüğü farkıyla ikinci bir kopyası olup olmadığını anlamak için bellekte taranır; gösterilmez ve saklanmaz. Kapsam: proje (Claude Code, terminal).",
  },
  { lead: "Sınır:", text: "en çok 1 MiB (1.048.576 bayt). Daha büyük dosya okunmaz." },
  { lead: "Yerel:", text: "bu okuma işlemi dosyayı yalnızca bu bilgisayarda işler ve dosya içeriğini göndermek için bilerek ağ bağlantısı başlatmaz; hiçbir program çalıştırılmaz, hiçbir dosyaya yazılmaz, hiçbir şey kurulmaz. Uygulamanın içindeki WebView çalışma ortamının ayrı ağ trafiği bu denetimin dışındadır ve dışlanmış değildir: geçmişte bu ortamdan uygulama dışına HTTPS trafiği gözlendi (açık sorun #25). Bunun bu dosyayla ilgili olduğuna dair bir kanıt yoktur, ancak olmadığı da kanıtlanmamıştır. Bu metin, bellekten kesin silinme veya o çalışma ortamının ağ erişimine karşı koruma vaadi değildir." },
  {
    lead: "Ekrana yalnızca şunlar gelir:",
    text: "dosyanın var olup olmadığı, boyutu, SHA-256 özeti, onaylı Exa bildirimiyle ilgili durum ve toplam sunucu sayısı. Dosyanın metni, anahtarlar, parolalar, URL’ler, ortam değişkenleri, başlıklar ve diğer sunucuların adları ekrana, günlüğe veya veritabanına gelmez. Exa durumu, kaynağı belgelenmiş bir sözdizimi karşılaştırmasıdır; çalışan bir Exa sunucusu olduğunu göstermez. SHA-256 özeti yalnızca okunan baytları tanımlar; kaynağını veya güvenilirliğini göstermez.",
  },
  { lead: "Saklanmaz:", text: "sonuç yalnızca bu sayfada, bellekte durur; “Temizle”ye bastığınızda, sayfadan ayrıldığınızda veya 5 dakika sonra silinir." },
  {
    lead: "Ne anlama gelmez:",
    text: "bu okuma, bir aracın yüklendiğini, güvenilir olduğunu, kurulu veya etkin olduğunu, erişilebildiğini ya da çalıştığını göstermez. Dosya, okuma anındaki bir anlık görüntüdür.",
  },
  {
    lead: "Önceki taramadan ayrıdır:",
    text: `Bilgisayar Envanteri’ndeki sonuçlar “${INVENTORY_LABEL}”, bu sonuç “${SNAPSHOT_LABEL}” olarak adlandırılır; biri diğerini doğrulamaz veya çürütmez. Uygulamanın katı JSON kuralı yerel bir politikadır; Claude Code’un dosyayı nasıl işleyeceğini göstermez.`,
  },
  { lead: "", text: "Dosya hâlâ kendi düzenleyicinizde görünür; bu uygulama onu değiştirmez." },
];

export const FOOTER =
  "Okuma anındaki anlık görüntüdür; dosya şimdi değişmiş olabilir. SHA-256 özeti yalnızca okunan baytları tanımlar; kaynağını, güvenilirliğini veya dosyanın şimdiki durumunu göstermez. Bu sonuç yüklendiğini, güvenildiğini, kurulduğunu, etkin olduğunu, erişilebildiğini veya doğrulandığını göstermez.";

// ---- result wording: one fixed sentence per status, one per reason ------------------------------------------------------------------------

const HEADLINE: Record<SnapshotStatus, string> = {
  root_not_found: "Bu klasör bulunamadı; dosyaya bakılmadı. Bu, dosyanın olmadığı anlamına gelmez.",
  absent:
    "Bu klasörde okuma anında tam olarak .mcp.json adlı bir dosya görülmedi. Bu, MCP’nin başka bir yerde (kullanıcı veya yerel kapsam) yapılandırılmadığını ya da bu klasörün projenizin kökü olduğunu göstermez.",
  present_summarized: ".mcp.json bulundu ve okundu: {n} sunucu tanımı. Hiçbir sunucu çalıştırılmadı; dosyanın içeriği saklanmadı.",
  present_metadata_only:
    ".mcp.json bulundu ancak içeriği güvenle özetlenemedi; yalnızca boyut ve SHA-256 özeti gösteriliyor. Bu, Claude Code’un dosyayı geçersiz bulacağı anlamına gelmez.",
  malformed:
    ".mcp.json geçerli bir JSON metni olarak okunamadı; yalnızca boyut ve SHA-256 özeti gösteriliyor. Claude Code’un dosyayı nasıl yorumlayacağı hakkında bir şey söylenmez.",
  denied: "Bu dosyaya veya klasöre erişim izni yok ya da dosya başka bir program tarafından kilitli. Dosyanın var olup olmadığı bilinmiyor; “yapılandırılmamış” anlamına gelmez.",
  unsafe_path:
    "Güvenlik için okunmadı: yol ya da dosya bir sembolik bağlantı/junction, sabit bağlantı, normal olmayan bir nesne ya da yalnızca harf büyüklüğüyle ayrılan ikinci bir ad içeriyor.",
  unsupported_fs: "Güvenlik için okunmadı: klasör, yerel olduğu doğrulanan bir dosya sisteminde değil (ağ paylaşımı, uzak sürücü veya bilinmeyen dosya sistemi).",
  too_large: "Dosya 1 MiB sınırından büyük; okunmadı.",
  changed_during_read: "Dosya okunurken değişti; hiçbir sonuç gösterilmedi. Başka bir program (düzenleyici, git, ajan) dosyaya yazıyor olabilir. İsterseniz yeniden deneyin.",
  io_error: "Dosya okunamadı (beklenmeyen hata). Hiçbir sonuç gösterilmedi.",
};

const REASON_LINE: Record<string, string> = {
  duplicate_key: "Neden: aynı anahtar bir nesnede birden fazla kez geçiyor.",
  byte_order_mark: "Neden: dosya bir bayt sırası işaretiyle (BOM) başlıyor.",
  root_not_object: "Neden: en üst düzey değer bir JSON nesnesi değil.",
  servers_not_object: "Neden: “mcpServers” bir nesne değil.",
  too_deep: "Neden: iç içe yapı bu uygulamanın sınırından (64 düzey) derin.",
  too_many_servers: "Neden: sunucu sayısı bu uygulamanın sınırından (256) fazla.",
  invalid_utf8: "Neden: dosya geçerli UTF-8 değil.",
  invalid_json: "Neden: JSON söz dizimi geçersiz.",
  lone_surrogate: "Neden: eşlenmemiş bir vekil (surrogate) karakter var.",
  empty_file: "Neden: dosya boş.",
  access_denied: "Neden: erişim reddedildi.",
  locked: "Neden: dosya başka bir program tarafından kilitli.",
  link: "Neden: bir sembolik bağlantı veya junction bulundu.",
  not_a_directory: "Neden: yazdığınız yol bir klasör değil.",
  not_regular_file: "Neden: .mcp.json normal bir dosya değil.",
  hard_linked: "Neden: .mcp.json birden fazla sabit bağlantıya sahip.",
  case_alias: "Neden: klasörde .mcp.json adının harf büyüklüğü farklı bir kopyası var.",
  invalid_root_text: "Neden: klasör yolu geçersiz (tam yol olmalı; boş, çok uzun olamaz; “.” ve “..” içeremez).",
  root_too_broad: "Neden: bir sürücünün veya dosya sisteminin kökü seçilemez.",
};

const NAME_LINE: Record<(typeof NAME_STATES)[number], string> = {
  present: "“exa” adlı bir sunucu tanımı var (yalnızca sözdizimi karşılaştırması; çalıştığı doğrulanmadı).",
  present_other_letter_case: "Harf büyüklüğü farklı bir “exa” adı var (yalnızca sözdizimi karşılaştırması).",
  absent: "“exa” adlı bir sunucu tanımı görülmedi.",
};

const ENDPOINT_LINE: Record<(typeof ENDPOINT_STATES)[number], string> = {
  present_under_this_name: "Onaylı uç nokta adresi dosyada bu adla görüldü (yalnızca karşılaştırma; erişilebilirlik denenmedi).",
  present_under_other_name: "Onaylı uç nokta adresi dosyada başka bir adla görüldü (yalnızca karşılaştırma; erişilebilirlik denenmedi).",
  absent: "Onaylı uç nokta adresi görülmedi.",
  not_evaluable: "Uç nokta karşılaştırılamadı.",
};

export const BRIDGE_EQUAL = "Yapıştırdığınız metin bu dosyayla bayt bayt aynı.";
export const BRIDGE_DIFFERENT = "Özetler farklı; yapıştırdığınız metnin bu dosyayla aynı olduğu kanıtlanamadı (satır sonu veya kodlama farkı olabilir).";

const ERROR_TEXT: Record<string, string> = {
  PROJECT_NOT_FOUND: "Bu proje kaydı bulunamadı; dosyaya bakılmadı.",
  READ_NOT_AUTHORIZED: "Onay iletilemedi; hiçbir şey okunmadı.",
};
export const ERROR_FALLBACK = "Okuma başlatılamadı; hiçbir şey okunmadı.";
/** Only the code is looked at; no message text from the backend is ever displayed. */
export const errorText = (code: string): string => ERROR_TEXT[code] ?? ERROR_FALLBACK;

// ---- strict acceptance of the backend answer ------------------------------------------------------------------------------------------

export interface SnapshotView {
  status: SnapshotStatus;
  headline: string;
  reasonLine: string | null;
  sizeBytes: number | null;
  /** Full digest (64 hex digits) or null. The screen shows only the first 16. */
  sha256: string | null;
  serverCount: number | null;
  declarationLines: string[];
}

const KEYS = ["schema", "status", "reason", "alias", "file", "scope", "surface", "maxBytes", "sizeBytes", "sha256", "serverCount", "reviewedDeclarations", "notClaims"];
const isObject = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);
const isCount = (v: unknown, max: number): v is number => typeof v === "number" && Number.isInteger(v) && v >= 0 && v <= max;

/**
 * Turns the raw backend value into a view, or `null` when it is not exactly the closed answer of the contract (unknown key, unknown value, a field that does not belong
 * to the status, a different alias...). A `null` means nothing from the value is shown.
 */
export function describeResult(raw: unknown, expectedAlias: string): SnapshotView | null {
  if (!isObject(raw)) return null;
  const keys = Object.keys(raw);
  if (keys.length !== KEYS.length || !KEYS.every((k) => keys.includes(k))) return null;
  const { status, reason, alias, sizeBytes, sha256, serverCount, reviewedDeclarations, notClaims } = raw;
  if (raw.schema !== "config02c.read.v1" || raw.file !== ".mcp.json" || raw.scope !== "project" || raw.surface !== "claude-code-terminal" || raw.maxBytes !== MAX_BYTES) return null;
  if (alias !== expectedAlias) return null;
  if (typeof status !== "string" || !(STATUSES as readonly string[]).includes(status)) return null;
  const s = status as SnapshotStatus;
  if (reason !== null && (typeof reason !== "string" || !REASONS[s].includes(reason))) return null;
  if (REASONS[s].length > 0 && reason === null) return null; // every status that has reasons always names one
  if (!Array.isArray(notClaims) || notClaims.length !== NOT_CLAIMS.length || !NOT_CLAIMS.every((c, i) => notClaims[i] === c)) return null;
  const bytesAllowed = WITH_BYTES.includes(s);
  if (bytesAllowed) {
    if (!isCount(sizeBytes, MAX_BYTES) || typeof sha256 !== "string" || !/^[0-9a-f]{64}$/.test(sha256)) return null;
  } else if (sizeBytes !== null || sha256 !== null) return null;
  if (s === "present_summarized") {
    if (!isCount(serverCount, 256) || !Array.isArray(reviewedDeclarations) || reviewedDeclarations.length !== 1) return null;
  } else if (serverCount !== null || reviewedDeclarations !== null) return null;

  const declarationLines: string[] = [];
  if (s === "present_summarized" && Array.isArray(reviewedDeclarations)) {
    const d = reviewedDeclarations[0];
    if (!isObject(d) || Object.keys(d).length !== 3 || d.id !== "exa-mcp-server") return null;
    const { nameState, endpointState } = d;
    if (typeof nameState !== "string" || !(NAME_STATES as readonly string[]).includes(nameState)) return null;
    if (typeof endpointState !== "string" || !(ENDPOINT_STATES as readonly string[]).includes(endpointState)) return null;
    declarationLines.push(NAME_LINE[nameState as (typeof NAME_STATES)[number]], ENDPOINT_LINE[endpointState as (typeof ENDPOINT_STATES)[number]]);
  }
  return {
    status: s,
    headline: HEADLINE[s].replace("{n}", s === "present_summarized" ? String(serverCount) : ""),
    reasonLine: typeof reason === "string" ? REASON_LINE[reason] ?? null : null,
    sizeBytes: bytesAllowed ? (sizeBytes as number) : null,
    sha256: bytesAllowed ? (sha256 as string) : null,
    serverCount: s === "present_summarized" ? (serverCount as number) : null,
    declarationLines,
  };
}

/** Equal digests prove identical bytes; different digests prove nothing else. `null`: nothing was pasted, nothing is said. */
export function pasteBridge(pastedDigest: string | null, fileDigest: string | null): string | null {
  if (pastedDigest === null || fileDigest === null) return null;
  return pastedDigest === fileDigest ? BRIDGE_EQUAL : BRIDGE_DIFFERENT;
}

/** Everything the panel can ever say, for the forbidden-wording test. */
export function allFixedText(): string[] {
  return [
    SECTION_TITLE, FOLDER_LABEL, FOLDER_HELP, READ_BUTTON, DECLINE_BUTTON, CLEAR_BUTTON, DECLINED_TEXT, NOT_REQUESTED_TEXT, READING_TEXT, UNEXPECTED_TEXT, HOST_NOTE, GATE_LEAD,
    ...GATE_POINTS.map((p) => `${p.lead} ${p.text}`), FOOTER, ...Object.values(HEADLINE), ...Object.values(REASON_LINE), ...Object.values(NAME_LINE), ...Object.values(ENDPOINT_LINE),
    BRIDGE_EQUAL, BRIDGE_DIFFERENT, ...Object.values(ERROR_TEXT), ERROR_FALLBACK,
  ];
}
