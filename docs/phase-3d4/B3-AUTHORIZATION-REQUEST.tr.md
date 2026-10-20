# B3 — jq kurulum yetkisi: TASLAK TEKLİF (yetki VERİLMEDİ, yürürlükte DEĞİL)

> Bu metin kod incelemesinden **ayrı** bir tekliftir ve **TASLAK**tır. Siz açıkça onay cümlesini yazmadıkça hiçbir şey yapılmaz; bu fazda (B2) gerçek jq dosyasına dokunulmadı, hiçbir şey kurulmadı, çalıştırılmadı. Aşağıdaki ön koşulların hiçbiri **henüz** tamamlanmış değildir; tamamlanmadan bu teklifin hiçbir maddesi uygulanamaz. Bu teklif B2 kodunu onaylamaz; B2'nin Red Team incelemesi ayrı bir iştir.

## Ön koşullar (hepsi gerekir; sırayla)

1. B2'nin **tam SHA'sı** için bağımsız Red Team incelemesi kabul edildi (Windows x64 ve macOS ARM64 CI'si o SHA'da yeşil; CI bağımsız kullanıcı cihazı testi sayılmaz).
2. **B3 değişikliği** ayrı, küçük ve incelenebilir bir commit olarak yapıldı: `jq_pilot::B3_UNLOCK` sabiti `None`'dan tam tarif özetine (`9c7a020d65d233e86577464a06c32b8f08fd4fd8403110e68fd551b674f7cca1`) çevrildi; başka hiçbir şey değişmedi (fark yalnızca bu satır ve ilgili testtir). Bu commit de incelendi.
3. Sizin **bir kerelik, yalnızca kurulum** yetkiniz (aşağıdaki cümle, kendi sözlerinizle).

## İstenen yetki (yalnızca bu)

**jq 1.8.2 (Windows x64, `jq-windows-amd64.exe`, 1.035.264 bayt, SHA-256 `a6fc67fe…c627`, kaynak `jqlang/jq` @ `34f7186b…4108`) dosyasının, bu bilgisayarda, uygulamanın kendi klasörüne BİR KEZ kurulması. Dosya ÇALIŞTIRILMAYACAK (`--version` dahil).**

Adımlar (hiçbiri ajan tarafından onay düğmesine basılarak yapılmaz):

1. **Boş, ayrılmış veri klasörü** (`ATM_DATA_DIR`); normal uygulama verileriniz kullanılmaz.
2. **Envanter 0** (`tools/fs_inventory.mjs`, yalnızca okur).
3. Saklanan, doğrulanmış dosya **yalnızca okunarak** yeniden özetlenir (1.035.264 bayt ve `a6fc67fe…c627` olmalı; değilse dururuz). Kopyası **sizin elinizle** `<ATM_DATA_DIR>\jq-pilot-input\jq-windows-amd64.exe` konumuna konur (uygulamanın okuduğu tek sabit yer; hiçbir yol arayüzden verilmez).
4. Uygulamanın `jq-pilot` derlemesi (varsayılan olarak kapalı; Windows'a özel) yerelde derlenir ve açılır. Önizleme, jq için şunları göstermelidir: tam SHA-256, **yerel saklanan dosya — indirme yok**, lisans kapsamı (*yalnızca bu bilgisayar, yeniden dağıtım DEĞİL*), bildirimler ve çözülmemiş Q1–Q3, her dosya sistemi etkisi, yapılmayacaklar, geri alma beklentisi, dosyanın **`artifact.bin`** adıyla saklanacağı ve komut olmayacağı/PATH'e eklenmeyeceği/çalıştırılmayacağı, ve onayın **Windows kimlik doğrulaması olmadığı**.
5. **"Onaylıyorum ve kur" düğmesine yalnızca siz basarsınız.**
6. **Envanter 1** ve fark: yalnızca `atm-managed\` altındaki girdiler (journal, receipts, state, versions) ve uygulamanın SQLite dosyaları yeni/değişmiş olabilir. Başka bir fark → pilot durur, hiçbir şey silinmez/"düzeltilmez".
7. Smart App Control / Defender tepkisi yalnızca **kaydedilir** (ayar değiştirilmez). Kurulan dosya yazılırken bir engel/karantina olursa uygulama bunu *dikkat gerekiyor* olarak gösterir; onarılmaz.
8. İsterseniz, **ayrı bir önizleme ve ikinci bir onayınızla** kaldırma denenir.

Bu yetki şunları **kapsamaz**: jq'yu çalıştırmak, PATH/kısayol/zamanlanmış görev/kayıt defteri/ajan ayarı, indirme veya ağ, yükseltilmiş yetki, güvenlik ayarı değişikliği, dağıtım/paketleme/barındırma, başka araç/sürüm/mimari/yöntem, Windows Hello, ikinci bilgisayar, genel yayın (**Issue #14 açık kalır**).

## Bilmeniz gerekenler

* Uygulama içi onay **kimlik doğrulaması değildir**; aynı Windows hesabındaki başka bir program bu pencereyi kullanabilir.
* Kurulan dosya jq komutu olmaz ve çalıştığının kanıtı değildir; imzasız jq'nun ilk çalıştırmada Smart App Control tarafından engellenmesi beklenir (denenmeyecek).
* Bağımsız inceleme **dar kapsamlıdır**: Rust ve ön yüz testlerini ve gerçek Windows penceresini inceleyici kendisi çalıştırmadı; bunlar geliştirici testleri ve CI ile desteklenir. Request A'nın ağ teması FAIL olarak kalır; kriptografik kanıt koşullu kabuldür. Lisans kabulü yeniden dağıtım izni değildir ve hukuki tavsiye değildir.
* Durma koşulları: özet/boyut uyuşmazlığı, beklenmeyen önizleme, yönetici klasörü dışında herhangi bir fark, güvenlik yazılımı engeli, "Tamamlandı"dan başka bir sonuç, sizin herhangi bir isteğiniz.

## Onay biçimi (siz doldurursunuz — ben doldurmam)

> "jq 1.8.2 (Windows x64) dosyasının yukarıdaki sınırlar içinde, bu bilgisayarda bir kez, **çalıştırılmadan** kurulmasına izin veriyorum." — tarih, ad

yetki VERİLMEDİ: bu cümle yazılmadan ve yukarıdaki ön koşullar tamamlanmadan hiçbir adım atılmaz.
