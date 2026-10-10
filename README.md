# AI Toolchain Manager | Phase 0

**Tarih:** 10 Ekim 2026  
**Durum:** Phase 0 R1 duzeltmeleri hazir; bagimsiz Red Team yeniden degerlendirmesi bekleniyor. Uygulama kodu ve arac kurulumu yok.  
**Dal:** `phase0/architecture` (yalnizca yerel; commit veya push yapilmadi).

## Yonetici Ozeti

### Neler dogrulandi

- Tauri 2; macOS ve Windows'ta sistem tepsisi, otomatik baslatma ve tek uygulama ornegi icin resmi API'lere sahip. Rust, React/TypeScript arayuzu ve SQLite ile onerilen taban teknik olarak makul. Bu, uygulamanin paketlenip imzalandigi veya hedef bilgisayarlarda dogrulandigi anlamina gelmez.
- Google Antigravity belgeleri IDE, CLI ve Antigravity 2.0'i ayri urun yuzeyleri olarak ele aliyor. MCP ve eklenti destegi ile Apple Silicon macOS ve Windows x64 resmi olarak belgelenmis. Bir arac icin Antigravity CLI destegi, Antigravity IDE destegi sayilmayacak.
- Dokuz katalog girdisi ayni turde degil: komut satiri ikilileri, Python araci, MCP sunucusu ve AI ajan talimat/eklenti paketleri var. Bunlar tek bir kurulum yontemine zorlanmayacak.
- Guvenli varsayilan: GitHub icerigini calistirmadan incele; bilinmeyen kuruculari otomatik calistirma; her proje etkinlestirmesini kullaniciya sor; gizli degerleri yerel kayitlarda dahi tutma.
- Bagimsiz Red Team'in R1 bulgularina gore katalog metadata'si kurulum yetkisi vermez; yetki ayri, tipli tarif ve kullanici onayi gerektirir. Dokuz arac icin makine-okunur baslangic katalogu `unreviewed` durumundadir.
- Proje secimleri ajan yuzeyine gore ayrilir; audit basarisi bagimsiz test ve kanit ozeti gerektirir; cihaz senkronizasyonu makine ozel verileri dislar.

### Belirsiz kalanlar

- Aracin belirli surumunun hedef OS/CPU'da ve hedef ajan yuzeyinde gercekten calistigi; upstream README iddialari tek basina islevsel kanit degil. Bagimsiz Red Team kabul testi tamamlanmadi.
- Imza, kurulum, temiz kaldirma ve geri alma davranisi her paket/surum icin ayni degil. Uygulama guncelleyicisinin imzalama anahtari ve dagitim/paketleme hatti henuz tasarlanmadi.
- ASD-STE100 metnini veya sozlugunu urunle yeniden dagitma izni dogrulanmadi. Tasarim bu metinleri urune gommez ve resmi uygunluk iddia etmez.
- Claude Code ve Antigravity etkin oturumlarini tum durumlarda guvenilir bicimde algilama henuz kanitlanmadi. Algilanamayan durumda guncelleme durdurulmali.

### Kritik Red Team bulgulari

1. GitHub'dan alinmis bir kurulum betigini okumadan calistirmak, kullanici yetkileriyle kod calistirma riskidir.
2. Ajan ayarlarini veya `CLAUDE.md` / `GEMINI.md` dosyalarini ezmek, talimat kaybi ve gizli politika degisikligi yaratabilir.
3. Gizli anahtar tarayicilarinin raporlari sirlarin kendisini icerebilir; rapor ve komut ciktilari varsayilan olarak senkronize edilmemeli veya loglanmamalidir.
4. Caveman CLI telemetrisi varsayilan olarak aciktir; proxy ve sikistirma da ayri veri akislaridir.
5. Arac guncellemesi acik bir ajan oturumunun kullandigi ikiliyi veya ayari degistirebilir. Oturumun kapali oldugu kanitlanamiyorsa guncelleme beklemelidir.

### Faz 1 karari

R1 duzeltmeleri yerel dalda ve otomatik kontrat testlerinde hazir; bagimsiz yeniden adjudikasyon bekleniyor. Bu paket Phase 1 izni degildir. R1 onaylanirsa bile yalnizca sahte araclarla, degisiklik yapmayan lifecycle/UI prototipi icin ayri kullanici yetkisi istenir. Daha sonraki kurulum, MCP/ajan ayari, senkronizasyon veya updater calismalari kendi kapilarina tabidir.

**Karar: PHASE 0 R1 REVIEW PENDING. Phase 1: NOT AUTHORIZED.**

## Teknik Paket

- [Teknik mimari, kontratlar, semalar ve kabul testleri](docs/PHASE-0-HANDOFF.md)
- [Dokuz arac icin uyumluluk ve kurulum matrisi](docs/TOOL-COMPATIBILITY.md)
- [Red Team bulgulari ve risk kaydi](docs/RED-TEAM.md)
- [Kaynaklar, surumler ve kanit sinirlari](docs/RESEARCH-SOURCES.md)
- [JSON Schema dosyalari](schemas/)
- [Dokuz araclik, unreviewed seed katalogu](data/catalog-seed.json)
- [Tipli install recipe kontrati](schemas/install-recipe.schema.json)
- [Senkronizasyon envelope kontrati](schemas/sync-envelope.schema.json)
- [Adversarial kontrat testleri ve sahte fixture'lar](tests/)
- [Bagimsiz Red Team adjudikasyonu](reviews/AI_Toolchain_Manager_Phase0_RedTeam_Adjudication.md)

## Kapsam Siniri

Bu dalda yalnizca Phase 0 mimari belgeleri, semalar, seed metadata ve kurcalamaya yonelik fixture testleri bulunur. Arac kurulumu, gelistirme ortami degisikligi, ozel GitHub deposu veya kimlik bilgisi olusturma, commit/push ya da Phase 1 uygulamasi yapilmamistir.