# AI Toolchain Manager

Yapay zekâ kodlama ajanları (Claude Code, Antigravity) için geliştirici araçlarını **ileride** kurmayı, yapılandırmayı, güncellemeyi ve yönetmeyi hedefleyen bir masaüstü uygulaması (Tauri 2 + Rust + React/TypeScript + SQLite).

> **Durum: Phase 3D-0 — geliştirme aşaması. Uygulama yayın için hazır DEĞİLDİR.** Bugün hiçbir araç kurmaz, indirmez, çalıştırmaz veya güncellemez; ajan ayarlarını değiştirmez; ağa kendiliğinden bağlanmaz. Yalnızca **salt okunur** işler yapar; kurulum yaşam döngüsünün (kur/güncelle/kaldır/geri al/çökme kurtarma) dürüst biçimde yapılabildiği, **yalnızca sentetik test dizinlerinde** ve **hiçbir düğmeye/komuta bağlı olmadan** kanıtlandı. Phase 3B/3C yalnızca araştırma, tasarım, yalıtılmış test kipli bağdaştırıcı ve çalıştırmasız doğrulama içerir (gerçek kurulum yoktur; jq dahil hiçbir üçüncü taraf program indirilmedi): dosya sahipliği sertleştirmesi (Issue #14), güvenilir kaynak kanıtı karşılaştırması, pilot aday seçimi, arşiv/açma şartnamesi ve gerçek onay tasarımı. İmzalı bir dağıtım paketi, otomatik güncelleyici veya kurucu henüz yoktur.

Bu proje bir kullanıcı (programcı olmayan bir kişi) tarafından, bağımsız bir "Red Team" incelemesi eşliğinde adım adım geliştirilir. Her aşama ayrı raporlanır; hiçbir aşama bir önceki aşamanın yetkisini genişletmez.

## Bugün neler yapabilirsiniz?

| Özellik | Ne yapar | Ne yapmaz |
| --- | --- | --- |
| Araç Kütüphanesi | 13 hazır kayıtlı araç/eklenti için künye bilgisi; GitHub bağlantısı ekleyerek aday kayıt oluşturma | Bağlantıdaki hiçbir şeyi indirmez veya çalıştırmaz; yayıncının README'sindeki komutlar asla uygulanmaz |
| Projeler ve tercihler | Örnek projeler ve takma adlı proje kayıtları; proje başına "istiyorum / ertele / reddet" tercihleri | Tercih "istek"tir; kurulu olduğu anlamına gelmez |
| Öneriler | Sabit kurallarla çalışan, yapay zekâ çağırmayan öneri önizlemesi | Hiçbir şey kurmaz veya etkinleştirmez |
| **Bilgisayar Envanteri** | İzin verdiğiniz bölümlerde (PATH, Claude Code kullanıcı dosyaları, tek bir proje klasörü) araçların **varlığını** salt okunur olarak gözlemler; isteğe bağlı SHA-256 parmak izi | Program çalıştırmaz, sürüm sormaz, ağa bağlanmaz; "bulundu ≠ yapılandırıldı ≠ çalışıyor" ayrımını korur; hiçbir gerçek aracı "doğrulanmış" saymaz |
| Yedek önizlemesi | Eski veritabanı yedeklerinde kişisel veri izi olup olmadığını (kapsamı ve sınırlarıyla) gösterir | Hiçbir yedeği silmez, taşımaz veya değiştirmez |
| **Kurulum Planı** | Seçtiğiniz araç/platform/ajan için "kurulsaydı neler olurdu" taslağını gösterir: adımlar, ön koşullar (yalnızca GÜNCEL ve izinli tarama kanıtıyla), ağ/telemetri, değişecek yapılandırma, çakışmalar, **tarif ve onay durumu**, geri almanın yok/tasarlanmış/benzetilmiş olduğu, planın neden uygulanamayacağı | **Hiçbir şey kurmaz.** Plan bir taslaktır, bir izin veya onay değildir; "Kur/Onayla" düğmesi yoktur. Gerçek araçlar için incelenmiş tarif yoktur |
| Etkinlik ve denetim | Zincirleme (hash) denetim kaydı, sağlık kontrolleri, tanılama | Gizli değerleri saklamaz |
| Ayarlar | Tema, sistem tepsisi, kullanıcı kontrollü otomatik başlatma | — |

## Henüz yapamadıkları

- Araç kurma, kaldırma, güncelleme, sürüm sorgulama, geri alma — **hiçbiri yok**.
- Gerçek bir araç için **incelenmiş tarif yok** (yerleşik tarif listesi bilerek boştur) ve kullanıcı onayı için bir arayüz/kalıcı depo yok; bu yüzden her gerçek plan "uygulanamaz" durumundadır. Tarif kayıt defteri, onay sözleşmesi ve işlem motoru yalnızca sentetik sahte araçlarla, yalıtılmış geçici dizinlerde sınanır ve hiçbir komuta/düğmeye bağlı değildir.
- Bağımsız olarak doğrulanmış bir referans özeti (imzalı kaynak) yok; yerleşik yetkili liste **bilerek boştur**. Bu nedenle hiçbir gerçek araç "kimliği doğrulanmış" olamaz.
- Ajan yapılandırma dosyalarını (ör. `settings.json`, `CLAUDE.md`) düzenleme yok.
- Cihazlar arası eşitleme, otomatik güncelleyici, imzalı kurulum paketi yok.
- Aktif sürüm yoklaması ve aktif güvenlik testi (Strix vb.) yok; yalnızca bir [ilke önerisi](docs/ACTIVE-PROBE-POLICY-PROPOSAL.md) var.

## Desteklenen ve doğrulanmamış platformlar

"Hedef" = yazılımın amaçladığı platform; "Test edilen" = bu depoda gerçekten kanıtı olan ortam. İkisi aynı şey değildir.

| Platform | Hedef | Gerçekte test edilen | Test edilmeyen |
| --- | --- | --- | --- |
| Windows x64 | Evet (Windows 10/11 + WebView2) | Geliştirici makinesinde Windows 11 x64: birim testleri, hata ayıklama derlemesi, WebView2 ile elle arayüz denemesi. GitHub Actions `windows-latest`: derleme + tüm testler | **Release (yayın) derlemesinin yerel olarak çalıştırılması** (Windows Akıllı Uygulama Denetimi yerel imzasız derlemeleri engelleyebilir; yalnızca CI'da doğrulanıyor); başka Windows sürümleri; ARM Windows (desteklenmez) |
| macOS Apple Silicon (arm64) | Evet | Yalnızca GitHub Actions `macos-14` (arm64): derleme, testler, başsız `--self-check`, ikilinin `Mach-O arm64` olduğu | **Gerçek bir Mac'te arayüz (GUI) doğrulaması hiç yapılmadı.** CI başarısı, masaüstü deneyiminin doğrulandığı anlamına gelmez |
| macOS Intel, Linux | Hayır | — | — |

Asgari işletim sistemi sürümleri bu projede ayrıca **sınanmamıştır**; Tauri/WebView2 belgelerindeki gereksinimlere bağlıdır. Evrensel uyumluluk iddia edilmez.

## Güvenlik sınırları (özet)

- Üçüncü taraf program çalıştırılmaz; keşfedilen çalıştırılabilir dosyalar açılmaz (yalnızca isteğe bağlı olarak baytları SHA-256 için okunur). Paket yöneticisi, kabuk veya ajan başlatılmaz.
- Ağ isteği, telemetri ve LLM çağrısı yok.
- Gizli değerler (anahtarlar, parolalar, jetonlar) günlüklere, arayüze, veritabanına veya Git'e yazılmaz; makineye özgü gözlemler yerelde kalır.
- WebView yalnızca tek tek listelenmiş tipli komutları çağırabilir (kabuk, dosya sistemi, HTTP izni yok).
- Dosya okumaları bağlantı (symlink/junction) izlemeyen katmandan geçer; ağ birimleri reddedilir.
- Bilinen sınırlar: [Phase 2C belgesi](docs/PHASE-2C.md) (özet doğrulama, WAL, TOCTOU sınırları) ve [Phase 2D belgesi](docs/PHASE-2D.md).

## Yerel geliştirme

Gerekenler: Rust (1.90+), Node.js (CI'da 24), Python 3.12 (yalnızca sözleşme testleri), Windows'ta WebView2, macOS'ta Xcode komut satırı araçları.

```bash
npm ci                              # ön yüz bağımlılıkları
npm run typecheck && npm test       # tip denetimi + arayüz testleri (vitest)
npm run tauri dev                   # geliştirme modunda uygulama
cd src-tauri
cargo fmt --check && cargo clippy --all-targets -- -D warnings
cargo test                          # Rust birim ve entegrasyon testleri
cd ..
python -m pip install -r requirements-test.txt
python -m unittest discover -s tests -v    # JSON Schema ve sözleşme testleri
```

Başsız kendi kendini denetleme (arayüz açmaz; sözleşme testlerinin girdisini üretir):

```bash
src-tauri/target/debug/ai-toolchain-manager --self-check out.json
ATM_SELFCHECK_JSON=out.json python -m unittest discover -s tests -v
```

`ATM_DATA_DIR` ortam değişkeni, veritabanı ve yedekler için yalıtılmış bir klasör seçer (testler ve CI bunu kullanır). Verilen değer yoksa işletim sisteminin kullanıcı başına uygulama verisi klasörü kullanılır.

## Test ve CI kanıtı

- Her `development` itme işlemi GitHub Actions'ta Windows x64 ve macOS arm64 üzerinde şunları çalıştırır: Rust biçim/lint (`-D warnings`) ve testler, ön yüz tip denetimi/testi/derlemesi, Python sözleşme testleri, derlenen ikilinin başsız `--self-check` çalıştırması. Kesin SHA ve sonuçlar için [Actions sayfasına](https://github.com/ErenTahiroglu/AI-Toolchain-Manager/actions) bakın; yeşil bir CI yalnızca o koşucunun gerçekten yaptığını kanıtlar.
- Phase 3C yerel sonuçları (Windows 11 x64): Rust 530 test, ön yüz 72 test, Python sözleşme 100 test, hata ayıklama derlemesi ve WebView2 üzerinde elle arayüz denemesi. Sayılar aşama raporlarında güncellenir; bu satır bir sürüm vaadi değildir.
- Kanıt sınırı: macOS CI'ı gerçek Mac masaüstü testi değildir; Windows release derlemesinin yerel doğrulaması yoktur.

## Aşama raporları ve belgeler

- Phase 0 (mimari, şemalar, risk kaydı): [docs/PHASE-0-OVERVIEW.md](docs/PHASE-0-OVERVIEW.md) (eski README, olduğu gibi korundu), [docs/PHASE-0-HANDOFF.md](docs/PHASE-0-HANDOFF.md), [docs/TOOL-COMPATIBILITY.md](docs/TOOL-COMPATIBILITY.md), [docs/RED-TEAM.md](docs/RED-TEAM.md), [docs/RESEARCH-SOURCES.md](docs/RESEARCH-SOURCES.md), [reviews/](reviews/)
- [Phase 1A](docs/PHASE-1A.md) — masaüstü temeli, denetim zinciri, sağlık kontrolleri
- [Phase 1B](docs/PHASE-1B.md) — dinamik katalog, proje tercihleri, öneri önizlemesi
- [Phase 2A](docs/PHASE-2A.md), [Phase 2B](docs/PHASE-2B.md) — salt okunur envanter, geçiş ve gizlilik kapısı, kanıt merdiveni
- [Phase 2C](docs/PHASE-2C.md) — güvenli yedek incelemesi, pasif artefakt kimliği, güven sözleşmesi
- [Phase 2D](docs/PHASE-2D.md) — kurulum planı önizlemesi, taşınabilirlik, evrensel Mach-O doğrulaması
- [Phase 3D-0](docs/PHASE-3D0.md) — ilk gerçek pilot (jq 1.8.2) hazırlık kapısı: çevrimdışı doğrulanmış yayıncı attestation kanıtı, Issue #14 kalan pencereler, gerçek onay tasarımı, gerçek cihaz test planı (indirme/kurulum yok)
- [Phase 3D-0 kapanışı](docs/PHASE-3D0-CLOSURE.md) — Red Team bulguları RT-3D0-01/02: sabitlenmiş imza kaynağı (commit `f0dabad…`), açık ağ izin listesi, resmî GitHub CLI doğrulayıcısı yetkili/elle yazılan doğrulayıcı destekleyici, düzeltilmiş indirme talebi ([Türkçe](docs/phase-3d0/REQUEST-A.tr.md)); hâlâ indirme/kurulum yok
- [Phase 3C kapanışı](docs/PHASE-3C-CLOSURE.md) — Issue #16: macOS'ta atomik "üzerine yazma yok" taşıma (`RENAME_EXCL`), yalnızca bilinen dosyanın değiştirilmesi, kurulu dosya kayması denetimi
- [Phase 3C](docs/PHASE-3C.md) — Issue #14/#15: tutamaç-göreli, bağlantı izlemeyen dosya sistemi katmanı (Windows + macOS), SQLite'ta kayıtlı yönetici kökü, yalnızca test kipinde tek-dosyalı bağdaştırıcı, sahte anahtar/onay arayüzleri, jq inceleme paketi (gerçek kurulum yok)
- [Phase 3B](docs/PHASE-3B.md) — dosya sahipliği (Issue #14), kaynak kanıtı kararı, pilot karşılaştırması (jq / ripgrep / Gitleaks), arşiv şartnamesi, onay ve denetim tasarımı, taşınabilirlik ve elle test listeleri (gerçek kurulum yok)
- [Phase 3A](docs/PHASE-3A.md) — incelenmiş tarif kayıt defteri, ayrı onay sözleşmesi, sentetik işlem motoru (kur/güncelle/kaldır/geri al/kurtarma); [Gitleaks pilot uygunluk değerlendirmesi](docs/GITLEAKS-PILOT-ASSESSMENT.md) (yalnızca araştırma: uygun değil)
- [Aktif yoklama ilke önerisi](docs/ACTIVE-PROBE-POLICY-PROPOSAL.md) (uygulanmadı), [Windows Akıllı Uygulama Denetimi notu](docs/WINDOWS-SMART-APP-CONTROL.md), [dal yönetimi](docs/BRANCH-GOVERNANCE.md), [ADR'ler](docs/adr/)

## Dallar

Yalnızca iki kalıcı dal vardır: `main` (kararlı; değiştirilmez) ve `development` (tüm çalışma). Aşama dalları açılmaz; ayrıntı için [dal yönetimi](docs/BRANCH-GOVERNANCE.md).
