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
12. **Beklenmeyen sonuçta operatör tarafında otomatik temizlik, yeniden deneme veya izin sıfırlama yoktur** (uygulamanın *kendi işleminin* geri alması ve açılışta kurtarması ayrı şeylerdir ve vardır: aşağıdaki bölüme bakın). *"tek seferlik izin belirsiz"* durumunda uygulama izni **sıfırlamaz** — çözüm incelenmiş bir insan kararıdır.

## Uygulamanın yerleşik davranışı ve operatör kısıtları (NEXT-11 düzeltmesi)

> Bu listenin önceki hâli "otomatik temizlik yoktur" diyordu. Bu, kaynak koduyla **uyuşmuyordu** ve düzeltildi. Aşağıdakiler `adapter.rs` ve `install_flow.rs` okunarak yazıldı; **sıfır otomatik temizlik** vaadi ve **kalıntısız geri alınabilirlik** garantisi VERİLMEZ. Bu bölümü yazan NEXT-11 çalışması yerel ve işlenmemiştir (uncommitted); ilgili Rust derlemesi ve testleri **çalıştırılmadı** (NOT RUN).

### Uygulamanın kendi yaptıkları (otomatik; operatör kapatamaz)

- Kurulum işlemi **olağan bir hatayla** biterse uygulama, hemen ve aynı çağrıda, **işlemin kendi yazdığı** dosyaları geri almayı dener (`adapter.rs`: `finish` → `roll_back`). Bu bir **deneme**dir; her hatanın hemen ve tamamen geri alınacağı VAAT EDİLMEZ. Dört durum ayrı tutulur:
  1. **Olağan hata, geri alma tamam:** geri alma `restored` veya `not_needed` ile biterse geçici dosyalar temizlenir ve kilit bırakılır.
  2. **Olağan hata, geri alma eksik (`partial` / `failed`):** geri alınamayan dosyalar ve klasörler yerinde kalır, işlem `RollbackFailed` olur, kilit **kalır**, geçici dosyalar temizlenmez; durum `needs_attention` olarak bildirilir. Kalıntı dosyalar bu durumda beklenen sonuçtur.
  3. **`Interrupted` (kesinti) ve süreç sonlanması:** `TxnError::Interrupted` yolunda `finish` **hiçbir geri alma yapmaz** (`Err(Interrupted)` olduğu gibi döner); bu, bir çökme benzetimidir. Süreç gerçekten sonlandığında (görev sonlandırma, elektrik kesintisi, çökme) hiçbir kod çalışmaz: işlem günlüğü yarım, dosyalar ve kilit yerinde kalır. Hiçbir şey o anda geri alınmaz.
  4. **Sonraki açılış kurtarması:** yarım işlem ancak uygulama **bir sonraki kez açıldığında** ele alınır (aşağıya bakın); bu da başarısız olabilir.
- Geri alma ve temizlik yalnızca **işlem günlüğünde kayıtlı ve doğrulanmış** dosyaları siler, tutamaç (handle) üzerinden: kimlik (birim + dosya numarası) ve tek bağlantı; dosya `Final` aşamasındaysa ayrıca boyut ve SHA-256. Yalnızca `Created` aşamasındaki dosya için içerik özeti **yoktur** (henüz yazılmamıştır): kimlik + bağlantı sayısı denetlenir. Kimliği hiç kaydedilmemiş (`Intended`) dosya silinmez. Bu kalan risk bilinir ve değiştirilmemiştir.
- Boş klasörler, incelenip boşaltılan klasörün **kimliğiyle** silinir (NEXT-11 değişikliği; CI'da doğrulanmadı). Bu koruma yalnızca **incelemeden sonrası** içindir; aşağıdaki "inceleme öncesi" riske bakın.
- Başarılı bir işlemden sonra, işlemin geçici (staging ve snapshot) dosyaları aynı kurallarla temizlenir.
- Uygulama **her açılışta** (`install_flow.rs`: `startup` → `recover`) yarım kalmış **kendi** işlemlerini geri almayı ya da tamamlamayı **dener**; bu da başarısız olabilir. Geri alma başarısızsa durum `needs_attention` kalır ve sonraki her açılışta yeniden denenir. Günlüğü okunamayan ya da özet denetimini geçmeyen kayıt `CorruptJournal` olarak bildirilir ve dokunulmaz.
- Kayıtta olmayan, değişmiş, belirsiz veya yabancı **dosyalar**, kod bunları **tespit edebildiği** ölçüde silinmez; olduğu gibi bırakılır ve bildirilir. Bu "yabancı hiçbir şey asla silinmez" anlamına GELMEZ; aşağıdaki iki açık risk bilinir ve **giderilmemiştir**. Temizlik başarısız olursa **artık dosyalar kalabilir**. Geri alma kısmî olabilir.
- **Klasör değiştirme (inceleme öncesi) — AÇIK RİSK:** NEXT-11 ile boş klasör silme, **incelenip boşaltılan klasörün kimliğine** bağlandı; incelemeden **sonra** aynı adla başka bir klasör konursa silinmez. Ancak günlük yalnızca **dosya** kimliklerini kaydeder, klasör kimliklerini değil. Bu yüzden incelemeden **önce** aynı adla konmuş, farklı ama boş (ya da yalnızca kayıtlı dosyalarla eşleşen içerikli) bir klasör, işlemin kendi klasörü sayılır ve silinebilir. Bu risk giderilmemiştir; kapatmak için günlük biçiminde değişiklik gerekir (kapsam dışı).
- **Günlük değiştirme (H3) — AÇIK RİSK, `DESIGN_BLOCKED`:** günlüğün ilk kaydı var olanın üzerine yazmayan taşımayla oluşturulur; **sonraki her kayıt** mevcut günlüğü değiştirerek yazar. Windows'ta bu değiştirme için küçük, belgelenmiş bir pencere **kalır**: aynı hesabın başka bir süreci, kimlik denetimi ile yeniden adlandırma arasında günlüğü silip aynı adla bir dosya koyarsa o dosyanın üzerine bu işlemin günlüğü yazılır. Etki alanı, yönetilen kökün sahip-yalnız `journal` klasöründeki o tek addır; macOS'ta durum değişimle yakalanıp geri alınır. Pencere kapatılmamıştır (`Adapter::save` belgesine ve `NEXT-11-DESIGN.md` dosyasına bakın). İlk etkin-sürüm işaretçisi ise var olanın üzerine yazmayan taşımayla oluşturulur.

### Operatör kısıtları (siz ve ajan için)

- Onaysız kaldırma (uninstall) yok.
- Elle temizlik, elle silme veya izin sıfırlama yok.
- Yeniden deneme yok — ne otomatik ne elle başlatılan — yeni ve açık bir yetki olmadan.
- jq çalıştırılmaz (`--version` dahil); PATH, kısayol, zamanlanmış görev, kayıt defteri ve ajan yapılandırması değiştirilmez.
- Belirsiz bir başarısızlıktan sonra, inceleme (triage) bitene kadar ATM yeniden başlatılmaz: yeniden başlatma, yukarıdaki otomatik kurtarmayı yeniden çalıştırır.

## Bu liste neyi kapsamaz

jq'yu çalıştırmak (`--version` dahil), PATH/kısayol/zamanlanmış görev/kayıt defteri/ajan ayarı, indirme veya ağ, yükseltilmiş yetki, SAC/Defender ayar değişikliği, Windows Hello, ikinci kurulum, ikinci klasör, başka araç/sürüm/mimari, dağıtım ve genel yayın (**Issue #14 açık kalır**).
