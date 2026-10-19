# Talep A (düzeltilmiş) — iki sabitlenmiş dosyayı indir, çalıştırmadan doğrula

**Bu yalnızca bir TEKLİFTİR. İndirme izni verilmemiştir.** Ayrıntılı ve tam komutlu İngilizce sürüm: [REQUEST-A.md](REQUEST-A.md). Burada yalnızca indirme ve doğrulama (izin 2 ve 3) istenir. **Kurulum (4) ve çalıştırma (5) istenmiyor.**

## 1. Dosyalar ve sabitlenmiş girdiler
| Ne | Tam değer |
| --- | --- |
| Dosya 1 | `jq-windows-amd64.exe`, **1.035.264 bayt**, SHA-256 `a6fc67fedaf9128a3309a1e2ebb8b986aeccf70122ee46d2cb4849e423f0c627`, URL `https://github.com/jqlang/jq/releases/download/jq-1.8.2/jq-windows-amd64.exe` |
| Dosya 2 | `jq-macos-arm64`, **841.504 bayt**, SHA-256 `2d75340ba57a4b4b4c8708a21c2dc8e958a48aaa8bba13b27f77f6e4c0eca07e`, URL `https://github.com/jqlang/jq/releases/download/jq-1.8.2/jq-macos-arm64` |
| Resmî sürüm | <https://github.com/jqlang/jq/releases/tag/jq-1.8.2> (etiket → commit `34f7186b86743a083a589741b6cea95293524108`) |
| İmza dosyaları | `master` dalından **değil**, değişmez commit `f0dabad36a5fdb37393829e6bd4c465d70c40a3a` (“Update signatures of 1.8.2 (#3566)”; ebeveyni sürüm commit’i; PR’ı bakımcı `itchyny` onaylayıp birleştirdi) üzerinden: `jq-windows-amd64.exe.asc` (833 B), `jq-macos-arm64.asc` (833 B), `sha256sum.txt` (2302 B) ve her birinin SHA-256’sı sabit |
| Açık anahtar | aynı commit’teki `sig/jq-release-new.key` (3167 B, SHA-256 `710cc5f0…283b`); parmak izi **tam olarak** `93079A9511EFC0B7D6CDBFB5B0DA60FB454BAF18` olmalı (RSA 4096, 2025-07-31’de oluşturuldu, süresiz) |
| Kimlik politikası | depo `jqlang/jq` (id 5101141, sahip id 104800540); sertifika kimliği tam `https://github.com/jqlang/jq/.github/workflows/ci.yml@refs/tags/jq-1.8.2`; ref `refs/tags/jq-1.8.2`; kaynak ve imzalayıcı özeti `34f7186b…4108`; OIDC yayıncısı `https://token.actions.githubusercontent.com`; GitHub barındırmalı koşucu |

**Önemli dürüstlük notu:** GPG anahtarı ve imzaları **bağımsız bir yayıncı doğrulaması sağlamaz**. Aynı depodan geliyorlar, anahtar 2025-07-31’de üretildi ve eski sürümler aynı gün yeniden imzalandı, web sitesi parmak izi yayımlamıyor. Yalnızca tutarlılık/ek savunma kontrolüdür.

## 2. Ağ izin listesi (önceki “yalnızca github.com” ifadesi yanlıştı)
- `github.com`: iki sürüm dosyasının adresi (yönlendirme verir).
- `objects.githubusercontent.com` ve `release-assets.githubusercontent.com`: GitHub’ın kontrolündeki dosya depolama; yönlendirme hedefi. **Başka herhangi bir host görülürse indirme durur ve size bildirilir.**
- `raw.githubusercontent.com`: yalnızca commit numarasıyla sabitlenmiş metin dosyaları.
- `api.github.com`: GitHub CLI’nin attestation sorgusu (CLI’de kayıtlı GitHub oturumu yalnızca burada kullanılır).
- `tuf-repo-cdn.sigstore.dev` ve `tuf-repo.github.com`: güvenilir Sigstore kök anahtarının yenilenmesi (kurulu CLI’nin içine gömülü; CLI bu istekleri kaydetmediği için “gözlenen” değil “beklenen” olarak raporlanır).
- Başka hiçbir hedef yok. İndirme aracı **hiçbir kimlik bilgisi (token, çerez, Authorization)** göndermez, en çok 3 yönlendirme izler, her adımda `https` ve izin listesi ister, her adımı kaydeder; **gerçekte bağlanılan son hostlar raporlanır.** Not: Faz 3D-0’da güvenilir kökü almak için bir kez `gh attestation trusted-root` çalıştırıldı; bu iki TUF hostuna, önceden listelenmeden bağlandı. Yalnızca herkese açık metadata idi; artık listelendi.

## 3. Geçici depolama ve dosya sistemi etkisi
Asistanın scratch klasörü altında yeni bir oturum klasörü (depo dışında, yönetilen kök dışında, PATH’te değil). **Her indirilen dosya kendi yeni boş klasöründe**, çalıştırma izni verilmeden. Toplam: iki çalıştırılabilir (~1,9 MB) + beş küçük metin dosyası (~7 KB) + ~40 KB doğrulama çıktısı + izole bir GPG anahtarlığı klasörü. Sistem ayarı, kayıt defteri, PATH, başlangıç öğesi, ajan yapılandırması, Smart App Control, Defender, Gatekeeper değişmez. Defender dosyaları yazılırken tarayabilir (işletim sisteminin kendi davranışı); bir şey karantinaya alırsa kaydedilir.

## 4. Doğrulama
1. Boyut ve SHA-256 (sabit değerlerle ve yayıncının `sha256sum.txt` listesiyle).
2. **Bakımı yapılan resmî doğrulayıcı** `gh attestation verify` (sıkı politika: `--cert-identity`, `--cert-oidc-issuer`, `--source-ref`, `--source-digest`, `--signer-digest`, `--deny-self-hosted-runners`, `--predicate-type`), bir kez çevrimiçi, bir kez saklı paketle çevrimdışı tekrar; **taze** alınmış güven köküyle (depodaki anlık kopya güven çıpası sayılmaz).
3. Elle yazılmış Sigstore doğrulayıcı **yalnızca destekleyici kanıt**tır, indirilen baytları da karşılaştırır. **İkisi uyuşmazsa oturum başarısız sayılır.**
4. GPG: izole anahtarlık, anahtar sunucusu yok, ağ yok; her dosya için tam olarak bir geçerli imza ve sabit parmak izi.
5. Yalnızca statik bilgi: Windows dosyasının Authenticode durumu, PE ve Mach-O başlık alanları. Hiçbir şey çalıştırılmaz (`--version` bile).
6. Tüm sonuçlar `tools/review_jq_verification.py` ile oybirliği denetiminden geçer; her şey uyuşmadıkça başarısız.

Yayımlama **atomik ve üzerine yazmaz**: doğrulanmış dosya önce hedef klasörde benzersiz adlı bir geçici dosyada bekler, sonra sert bağlantıyla (hard link) son adına yayımlanır; o ada ait herhangi bir şey (dosya, klasör, sembolik bağ) varsa — az önce oluşmuş olsa bile — işletim sistemi reddeder ve hiçbir şey ezilmez. Klasörün dosya sistemi bunu desteklemiyorsa ağa bağlanmadan önce işlem reddedilir. Yalnızca aracın kendi oluşturduğu dosyalar silinir; bilinmeyen içerik korunur.

## 4b. Birinci uygulamadan sonra düzeltmeler (Issue #19)
- **Ağ:** GitHub CLI'nin çevrimiçi doğrulaması onaylı listede olmayan `tmaproduction.blob.core.windows.net` adresine (GitHub'ın Azure üzerindeki kanıt paketi depolaması) bağlandı. Bu bağlantı **yetkisizdi, geriye dönük olarak da onaylanmadı** ve host listeye eklenmedi. Ayrıntı: [REQUEST-A-NETWORK-INCIDENT.md](REQUEST-A-NETWORK-INCIDENT.md).
- **Varsayılan artık çevrimdışı:** saklı paketle yeniden doğrulama hiçbir hosta bağlanmaz. Yalnızca güven kökü yenilemesi (iki TUF hostu) ağ kullanır ve `tools/egress_guard.py` (varsayılan olarak her şeyi reddeden yerel koruma) arkasında çalışır: izin listesinde olmayan her deneme bağlantı kurulmadan reddedilir ve olay sayılır (çıkış kodu 97). Çevrimiçi kontrol ancak **ayrı ve açık bir yetkiyle** (her host adıyla) yapılabilir. Koruma bir güvenlik duvarı değildir; proxy ayarını yok sayan bir program durdurulamaz — bu yüzden çevrimdışı yol tercih edilir.
- **Paket biçimi:** `tools/extract_sigstore_bundles.py` saklı API yanıtından her paketi bayt bayt aynen çıkarır (yeniden yazmaz).
- **GPG:** Windows'ta Git'in GnuPG'si için `GNUPGHOME` POSIX yazımıyla (`/c/Users/...`) verilir; `gpg --version` çıktısındaki `Home:` satırı doğrulanır. Açık anahtar içe aktarırken görülen `gpg-agent` başlatma hatası (çıkış 2) yalnızca anahtarın gerçekten içe aktarıldığı (IMPORT_OK) kanıtlanırsa kabul edilir. Araç kullanım hataları ile kriptografik hatalar ayrılır; beklenmeyen her hata başarısız sayılır.
- **Raporlama:** altı ayrı bilgi: kriptografik doğrulama, tam bayt eşleşmesi, resmî doğrulayıcı, ağ politikasına uyum, kurulum yetkisi (hep hayır), çalıştırma yetkisi (hep hayır). Yapılan oturum için ağ politikasına uyum **kalıcı olarak hayır** diye kayıtlıdır.

## 5. Kanıt saklama
Oturum klasörünün **tamamı Red Team incelemesine kadar saklanır, otomatik silinmez** (aksini siz istemedikçe): iki dosya, sabit metinler, indirme günlüğü (hostlar), taze güven kökü, tüm doğrulama çıktıları, anahtarlık listesi, oybirliği sonucu. Dosyaların SHA-256 listesi depoya yazılır (ikililer değil). Mevcut yedeklere dokunulmaz.

## 6. Yapılmayacaklar
Çalıştırma, kurulum, PATH/ayar değişikliği, herhangi bir referans ya da tarife özet eklenmesi, başka dosya/sürüm/platform, macOS `codesign`/Gatekeeper kontrolü (gerçek Mac gerekir), uygulamanın kendisinin ağ kullanımı.

**Onayınız şu cümleyle verilmiş sayılır:** “Talep A’yı yukarıdaki haliyle onaylıyorum.” Değişiklik isterseniz belirtin; başka bir şey yapılmaz.
