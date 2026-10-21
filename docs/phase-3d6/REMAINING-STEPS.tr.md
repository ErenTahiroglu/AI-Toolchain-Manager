# İlk yetkili gerçek jq kurulumundan önce kalan adımlar — HAZIRLIK (izin DEĞİLDİR)

> Bu liste Phase 3D-6'nın **sonunda** durur. Bu fazda jq dosyası açılmadı, kopyalanmadı, özetlenmedi, kurulmadı ve çalıştırılmadı; gerçek önizleme açılmadı, gerçek onay düğmesine basılmadı. Aşağıdaki adımların hiçbiri, **son bağımsız Red Team incelemesi** tamamlanıp sonucu size ulaşmadan ve siz kurulumu *ayrıca, açıkça* başlatmadan atılmaz. Bu liste izin DEĞİLDİR; yetki cümlesi kullanıcının önceki mesajında verilmiştir ve bu fazda **kullanılmamıştır**.

Seçilen klasör, yolu depoda yazılmadan, yalnızca **kapsam tanımlayıcısıyla** anılır: `158c279e…e8a5` (yol metninin normalize edilmiş özeti; sır değildir, kimlik doğrulama değildir). Kapsam kuralı: büyük/küçük harf, `/` ile `\` ve sondaki ayraç önemsizdir; başka her yol (kısa 8.3 adı, bir bağlantı/junction üzerinden başka bir yazım, bir alt klasör, üst klasör, kopyalanmış bir veritabanı) **farklıdır** ve `pilot_wrong_data_directory` ile reddedilir.

## Sırasıyla

1. **Son bağımsız Red Team incelemesi** — bu fazın tam SHA'sı (arming commit dahil), Windows x64 + macOS ARM64 CI'si ve bu raporun kanıtları üzerinde. Sonuç olumsuzsa **hiçbir şey kurulmaz**.
2. **Gerçek kurulumu başlatan ayrı ve açık talimat** (kullanıcıdan). Bu fazın "dur" talimatı geçerli olduğu sürece ajan bu adıma geçmez.
3. **Klasörün yeniden okunması (yalnızca okuma):** klasör hâlâ var mı, **boş mu**, bağlantı/junction değil mi, depo çalışma ağacı değil mi. Bu faz sırasında okundu: var, boş, yeniden-ayrıştırma noktası yok, depoyla ilgisiz. Aradan zaman geçtiği için tekrar edilir; boş değilse **dururuz** (hiçbir şey silinmez, taşınmaz).
4. **Envanter 0:** `node tools/fs_inventory.mjs snapshot <klasör> inventory-0.json` (yalnızca okur).
5. **Saklanan Request A ikilisinin yalnızca-okuma yeniden özeti:** boyut tam 1.035.264 bayt, SHA-256 tam `a6fc67fe…c627`; Request A oturum listesi (54 dosya) kayıtlı özetleriyle karşılaştırılır. Herhangi bir fark → **dururuz**.
6. **Uygulamanın `jq-pilot` hata ayıklama derlemesi** yerelde derlenir (hiçbir sürüm derlemesi bunu kullanmaz). Smart App Control derlemeyi engellerse **BLOCKED / NOT RUN** olarak kaydedilir; ayar değiştirilmez, engeli aşmak için yeniden derlenmez.
7. **Elle hazırlama (sizin elinizle):** doğrulanmış dosyanın bir kopyası tam olarak `<klasör>\jq-pilot-input\jq-windows-amd64.exe` konumuna konur; kopya yeniden özetlenir. Bu, jq'nun uygulamanın okuyabileceği yere **ilk** konuşudur; bu fazda yapılmadı.
8. **Uygulamayı başlatma:** `ATM_DATA_DIR`, tam olarak seçilen klasör olur (kısa ad / bağlantı kullanılmaz). Uygulama jq satırını yalnızca her şey tutuyorsa *Kurulabilir* yapar; aksi halde nedenini yazar.
9. **Tam önizleme** (sizin okumanız için; hiçbir şeyi değiştirmez) → **onay düğmesine yalnızca siz basarsınız.** Ajan o düğmeye asla basmaz.
10. **Kurulum:** tek seferlik izin, onay denetim kaydından sonra ve işlemden önce kalıcı olarak ayrılır; yalnızca geri okuma doğruysa *kullanıldı* olur.
11. **Envanter 1 ve SHA-256 karşılaştırması;** SAC/Defender gözlemi (**ayar değişmeden**, yalnızca kayıt); dürüst sonuç. jq **çalıştırılmaz** (`--version` dahil), PATH'e eklenmez.
12. **Beklenmeyen sonuçta otomatik temizlik veya yeniden deneme yoktur;** *"tek seferlik izin belirsiz"* durumunda uygulama izni **sıfırlamaz** — çözüm incelenmiş bir insan kararıdır.

## Bu liste neyi kapsamaz

jq'yu çalıştırmak (`--version` dahil), PATH/kısayol/zamanlanmış görev/kayıt defteri/ajan ayarı, indirme veya ağ, yükseltilmiş yetki, SAC/Defender ayar değişikliği, Windows Hello, ikinci kurulum, ikinci klasör, başka araç/sürüm/mimari, dağıtım ve genel yayın (**Issue #14 açık kalır**).
