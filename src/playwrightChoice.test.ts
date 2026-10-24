// CATALOG-02 - the pure part of the Playwright comparison: the three variants, their wording, and how a catalog record's state is described. No React, no IPC.
import { describe, expect, it } from "vitest";
import { FAMILY_IDS, FUTURE_RUNTIME_NOTE, PLAYWRIGHT_INTRO, PLAYWRIGHT_VARIANTS, SUGGESTED_NOTE, isCompleteTarget, optionStateFor, recordNotes } from "./playwrightChoice";
import { makeEntry } from "./test/backend";

const entry = (id: string, over = {}) => makeEntry(id, { displayName: id, ...over });

describe("the three options", () => {
  it("are CLI + Skills, Test / Library and MCP, in that order, each a distinct catalog id", () => {
    expect(PLAYWRIGHT_VARIANTS.map((v) => v.short)).toEqual(["CLI + Skills", "Test / Library", "MCP"]);
    expect(PLAYWRIGHT_VARIANTS.map((v) => v.entryId)).toEqual(["microsoft-playwright-cli", "microsoft-playwright", "microsoft-playwright-mcp"]);
    expect(new Set(FAMILY_IDS).size).toBe(3);
  });

  it("mark exactly one as the publisher's starting point for coding agents, and say that is not a claim of installation or compatibility", () => {
    expect(PLAYWRIGHT_VARIANTS.filter((v) => v.suggestedForCodingAgents).map((v) => v.entryId)).toEqual(["microsoft-playwright-cli"]);
    for (const needle of ["başlangıç noktası", "Yayıncı belgelerinin", "“her depoda çalışır”", "“kurulu”", "“uyumluluğu doğrulandı”", "kendiliğinden etkinleştirilmez"]) {
      expect(SUGGESTED_NOTE).toContain(needle);
    }
  });

  it("each says when to choose it over the others and what to watch, in plain Turkish prose (no URL, command or markdown)", () => {
    const [cli, lib, mcp] = PLAYWRIGHT_VARIANTS;
    expect(cli.when).toMatch(/kodlama ajanının birçok depoda/);
    expect(cli.when).toMatch(/MCP’den daha az bağlam/);
    expect(cli.when).toMatch(/ölçmedi/); // the efficiency claim is the publisher's, not measured here
    expect(lib.when).toMatch(/projenin içinde kendi tarayıcı testlerinizi/);
    expect(lib.when).toMatch(/makine genelindeki bir CLI bunun yerini tutmaz/);
    expect(mcp.when).toMatch(/durumlu, keşfe dayalı/);
    expect(mcp.when).toMatch(/CLI’dan yüksektir/);
    expect(cli.watch).toMatch(/Ortak bir kurulum, ortak ya da ayrı bir tarayıcı profili demek değildir/);
    expect(lib.watch).toMatch(/tarayıcı çalışma zamanı indirebilir/);
    expect(mcp.watch).toMatch(/hiçbir ajan ayarına yazmaz/);
    expect(mcp.watch).toMatch(/belgedir, mimariye özel bir işlev testi değildir/);
    for (const v of PLAYWRIGHT_VARIANTS) {
      for (const text of [v.when, v.watch, v.title]) expect(text).not.toMatch(/https?:|npm |npx|```|playwright-cli |✓|✕/);
    }
  });

  it("the intro and the future-runtime note keep the choice with the user and promise nothing", () => {
    expect(PLAYWRIGHT_INTRO).toMatch(/Hiçbiri kurulu değil, hiçbiri çalıştırılmadı, hiçbirine bağlanılmadı/);
    expect(PLAYWRIGHT_INTRO).toMatch(/Birden fazlasını isteyebilirsiniz: aralarında teknik bir dışlama yoktur/);
    for (const needle of ["mimari öneridir", "kurulu bir özellik değildir", "kendi tercihini ayrı tutar", "profilleri ayrı kalır", "çerez veya oturum paylaşımına izin vermez", "hiçbir dosya yazmaz"]) {
      expect(FUTURE_RUNTIME_NOTE).toContain(needle);
    }
  });
});

describe("how a catalog record is described", () => {
  const ids = ["microsoft-playwright", "microsoft-playwright-cli", "microsoft-playwright-mcp"];
  it("a missing record is missing: no other option is offered in its place", () => {
    const state = optionStateFor([entry("microsoft-playwright")], "microsoft-playwright-mcp");
    expect(state).toEqual({ kind: "missing" });
    expect(recordNotes(state)).toEqual(["Bu kayıt katalogda bulunamadı; yerine başka bir seçenek seçilmedi."]);
  });

  it("an ordinary enabled seed record needs no note", () => {
    const state = optionStateFor(ids.map((i) => entry(i)), "microsoft-playwright-cli");
    expect(state).toMatchObject({ kind: "present", removed: false, disabled: false, userOwned: false });
    expect(recordNotes(state)).toEqual([]);
  });

  it("removed, disabled and user-owned records each say so, and say what that means", () => {
    const removed = recordNotes(optionStateFor([entry("microsoft-playwright-mcp", { removedAt: "2026-10-20T00:00:00Z", enabled: false })], "microsoft-playwright-mcp"));
    expect(removed.join(" ")).toMatch(/Katalogdan kaldırılmış: yeni karar kaydedilemez/);
    expect(removed.join(" ")).not.toMatch(/Katalogda kapalı/); // removed wins over disabled
    const disabled = recordNotes(optionStateFor([entry("microsoft-playwright", { enabled: false })], "microsoft-playwright"));
    expect(disabled.join(" ")).toMatch(/Katalogda kapalı: “kullanmak istiyorum” seçilemez/);
    const user = recordNotes(optionStateFor([entry("microsoft-playwright-cli", { origin: "user" })], "microsoft-playwright-cli"));
    expect(user.join(" ")).toMatch(/sizin eklediğiniz bir aday/);
    expect(user.join(" ")).toMatch(/uyumluluk bilinmiyor/);
  });

  it("a target is complete only when project, operating system and agent surface are ALL chosen", () => {
    expect(isCompleteTarget({ alias: "a", platform: "windows-x64", surface: "local-cli" })).toBe(true);
    for (const t of [{ alias: "", platform: "windows-x64", surface: "local-cli" }, { alias: "a", platform: "", surface: "local-cli" }, { alias: "a", platform: "windows-x64", surface: "" }, { alias: "", platform: "", surface: "" }]) {
      expect(isCompleteTarget(t)).toBe(false);
    }
  });
});
