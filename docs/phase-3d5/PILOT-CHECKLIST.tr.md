# Gerçek jq pilotu için kontrol listesi — HAZIRLIK (izin DEĞİLDİR)

> Bu liste, **ileride** ayrıca yetkilendirilecek tek seferlik gerçek kurulum için bir hazırlıktır. Şu an bunlardan **hiçbiri yapılmaz**: jq dosyası açılmaz, kopyalanmaz, kurulmaz, çalıştırılmaz. Liste, siz açıkça yetki cümlesini yazmadan ve bağımsız Red Team incelemesi bitmeden uygulanamaz. Ajan, onay düğmesine **asla** basmaz.

Hedef: jq 1.8.2 · Windows x64 · `jq-windows-amd64.exe` · 1.035.264 bayt · SHA-256 `a6fc67fe…c627` · tarif özeti `9c7a020d…cca1`. Kurulum **tek seferliktir**: tamamlanan ilk kurulum izni kalıcı olarak tüketir; kaldırma izni geri açmaz.

## Önce (hiçbir şey kurulmadan)

1. **İzole, boş veri klasörü.** Yeni, boş bir klasör seçilir (ör. `C:\atm-jq-pilot`); normal uygulama verileriniz kullanılmaz. Uygulama bu klasörle `ATM_DATA_DIR` üzerinden başlatılır. Tek seferlik izin **yalnızca bu klasör için** etkinleştirilir: `python tools/pilot_data_dir_digest.py "C:\atm-jq-pilot"` çıktısı (klasör yolunun özeti; yol saklanmaz) yetki commit'ine yazılır. Yazım farkı (büyük/küçük harf, `/` ve `\`) önemsizdir; başka bir klasör ya da kopyalanmış bir veritabanı izni kullanamaz.
2. **Envanter 0:** `node tools/fs_inventory.mjs snapshot <ATM_DATA_DIR> inventory-0.json` (yalnızca okur).
3. **Saklanan Request A ikilisinin yeniden özetlenmesi (yalnızca okuma):** dosya boyutu tam 1.035.264 bayt ve SHA-256'sı tam `a6fc67fe…c627` olmalı; ayrıca Request A oturum listesi (54 dosya) kayıtlı özetleriyle karşılaştırılır. Herhangi bir fark → **dururuz**.
4. **Elle hazırlama konumu (açıkça bildirilir):** doğrulanmış dosyanın bir **kopyası, sizin elinizle**, tam olarak şuraya konur: `<ATM_DATA_DIR>\jq-pilot-input\jq-windows-amd64.exe`. Uygulama yalnızca bu tek sabit yeri okur (yol arayüzden verilmez; yalnızca okunur, değiştirilmez). Kopya yeniden özetlenir. Bu adım kurulum değildir; ama dosya bu andan itibaren uygulamanın okuyabileceği yerdedir.
5. Uygulamanın `jq-pilot` derlemesi (varsayılan olarak kapalı, yalnızca Windows) yerelde derlenir/açılır. jq satırı yalnızca her şey tutuyorsa *Kurulabilir* olur; aksi halde nedenini yazar (ör. *tek seferlik izin etkinleştirilmedi*, *başka veri klasörü*), ve hiçbir şey önizlenemez.

## Kurulum

6. **Tam, incelenmiş önizleme** (sizin okumanız için): kaynak depo ve sabit commit, sürüm, platform, dosya adı/boyutu, **tam SHA-256**, *yerel saklanan dosya — indirme yok*, lisans kapsamı (*yalnızca bu bilgisayar, yeniden dağıtım DEĞİL*), bildirimler ve çözülmemiş Q1–Q3, her dosya sistemi etkisi, yapılmayacaklar, geri alma beklentisi, dosyanın **`artifact.bin`** adıyla saklanıp **komut olmayacağı, PATH'e eklenmeyeceği, çalıştırılmayacağı**, ve **onayın Windows kimlik doğrulaması DEĞİLDİR** olduğu. Önizleme hiçbir şeyi değiştirmez.
7. **Onay düğmesine yalnızca siz basarsınız:** önizlemedeki her şeyi okuyup "okudum ve anladım"ı işaretledikten sonra *Onaylıyorum ve kur*. Tek kullanımlık onay, kısa süre sonra geçersiz olur.
8. Uygulamanın sırası: onay tüketilir → önizleme yeniden kurulur ve aynı özetle eşleşmeli → dosya yeniden okunup boyut/biçim/SHA-256/kimlik doğrulanmış referans yeniden denetlenir → zorunlu *onay* denetim kaydı yazılır → **tek seferlik izin ayrılır (kalıcı, atomik)** → işlem günlüğü, hazırlık kopyası, doğrulama, sürüm klasörü, işaret dosyası, makbuz → **durum geri okunur** → ancak o zaman izin *kullanıldı* olur → sonuç denetim kaydı.

## Sonra

9. **Envanter 1 ve SHA-256 karşılaştırması:** yalnızca `atm-managed\` altındaki girdiler (journal, receipts, state, versions) ve uygulamanın SQLite dosyaları yeni/değişmiş olabilir; `versions\jq\1.8.2\artifact.bin` dosyasının SHA-256'sı `a6fc67fe…c627` ve boyutu 1.035.264 olmalı; saklanan kaynak dosya ve girdi kopyası aynen durmalı. Başka herhangi bir fark → pilot durur, **hiçbir şey silinmez veya "düzeltilmez"**.
10. **Smart App Control / Defender gözlemi (ayar değiştirilmeden):** dosya yazılırken/yazıldıktan sonra bir olay, karantina, kaybolan veya değişen dosya olup olmadığı **yalnızca kaydedilir**. Uygulama bunu *dikkat gerekiyor* olarak gösterir; onarmaz. jq çalıştırılmaz (`--version` dahil).
11. **Dürüst sonuç:** *Tamamlandı* · *Reddedildi* (hiçbir şey değişmedi) · *Başarısız — geri alındı* (rapor) · *Yapıldı — ama dikkat gerekiyor* (gerçek durum gösterilir). Başarı yalnızca geri okuma doğruysa söylenir.
12. **Kaldırma (isteğe bağlı):** ayrı bir önizleme ve **ikinci, ayrı bir onayınızla**. Kaldırma, tek seferlik izni **geri açmaz**; yeniden kurulum için yeni, açık bir yetki ve yeni bir değişiklik gerekir.

## Beklenmeyen sonuç (durma koşulları)

Herhangi biri pilotu bitirir ve **otomatik temizlik veya otomatik yeniden deneme YAPILMAZ**: özet/boyut uyuşmazlığı · önizlemede listelenmeyen bir şey · veri klasörü dışında herhangi bir fark · güvenlik yazılımı engeli · *tamamlandı*dan başka sonuç · sizin herhangi bir isteğiniz. Özellikle *"tek seferlik izin belirsiz"* durumunda (kurulum başlatıldı ama sonuçlanmadı) uygulama **hiçbir şeyi sıfırlamaz**; çözüm kod değil, incelenmiş bir insan kararıdır (ör. Red Team'in onaylayacağı yeni bir izole klasör ve yeni yetki).

## Bu liste neyi kapsamaz

jq'yu çalıştırmak, PATH/kısayol/zamanlanmış görev/kayıt defteri/ajan ayarı, indirme veya ağ, yükseltilmiş yetki, güvenlik ayarı değişikliği, dağıtım/paketleme/barındırma, başka araç/sürüm/mimari, Windows Hello, başka bilgisayar, genel yayın (**Issue #14 açık kalır**).
