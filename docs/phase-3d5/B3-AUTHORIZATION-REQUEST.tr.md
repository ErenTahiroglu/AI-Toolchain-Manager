# B3 — jq için TEK SEFERLİK kurulum yetkisi: TASLAK TEKLİF (yetki VERİLMEDİ, yürürlükte DEĞİL)

> Bu metin kod incelemesinden **ayrı** bir tekliftir ve **TASLAK**tır; [Phase 3D-4 teklifinin](../phase-3d4/B3-AUTHORIZATION-REQUEST.tr.md) yerine geçer. Siz aşağıdaki cümleyi kendi sözlerinizle yazmadıkça hiçbir şey yapılmaz. Bu fazda gerçek jq dosyasına dokunulmadı; hiçbir şey kurulmadı, çalıştırılmadı. Bu teklif B2/B3 kodunu onaylamaz; incelemeleri ayrı bir iştir.

## Neyi onaylıyorsunuz (ve neyi onaylamıyorsunuz)

**Tek bir** kurulum: jq 1.8.2 (Windows x64, `jq-windows-amd64.exe`, 1.035.264 bayt, SHA-256 `a6fc67fe…c627`, kaynak `jqlang/jq` @ `34f7186b…4108`, tarif özeti `9c7a020d…cca1`), **yalnızca sizin seçeceğiniz tek bir izole veri klasörüne**, uygulamanın kendi klasörüne, **BİR KEZ**. Dosya **çalıştırılmadan** (`--version` dahil) saklanır.

**"Tek seferlik" ne demek (Issue #24 düzeltmesi):** Uygulama bunu kalıcı olarak uygular — derleme bayrağına veya tek kullanımlık önizlemeye güvenmez. İlk **tamamlanan ve geri okunan** kurulum izni *kalıcı olarak tüketir*. Uygulamayı kapatıp açmak, dosyayı kaldırmak, yeni bir önizleme açmak ya da yeni bir onay vermek **ikinci bir kurulum** yetkisi vermez. Kaldırma ayrı bir önizleme ve **ikinci, ayrı bir onayınızla** yapılabilir; fakat yeniden kurulum için yeni, açık bir yetki ve yeni bir değişiklik gerekir. Sonucu belirsiz kalan bir kurulumda (ör. çökme) uygulama izni **sıfırlamaz** ve *dikkat gerekiyor* der; çözüm incelenmiş bir insan kararıdır.

Bu yetki şunları **kapsamaz**: jq'yu çalıştırmak, PATH/kısayol/zamanlanmış görev/kayıt defteri/ajan ayarı, indirme veya ağ, yükseltilmiş yetki, güvenlik (SAC/Defender) ayarı değişikliği, dağıtım/paketleme/barındırma, başka araç/sürüm/mimari/yöntem, Windows Hello, ikinci bir kurulum, ikinci bir veri klasörü, genel yayın (**Issue #14 açık kalır**).

## Ön koşullar (hepsi gerekir; sırayla; henüz hiçbiri tamamlanmadı)

1. Bu aşamanın (B3 ön-yetki) **tam SHA'sı** için bağımsız Red Team incelemesi (Windows x64 ve macOS ARM64 CI yeşil; CI bağımsız kullanıcı cihazı testi değildir).
2. **B3 etkinleştirme adayı** (`B3_UNLOCK` → tam tarif özeti) da incelendi; tek seferlik izin hâlâ **etkin değil**.
3. **Sizin yetkiniz** (aşağıdaki cümle) ve **tek izole klasörünüzün adı**. Ben `python tools/pilot_data_dir_digest.py "<klasör>"` ile özetini üretip **ayrı, küçük bir yetki commit'ine** (`B3_ARMED_DATA_DIR_SHA256`) yazarım; bu commit de incelenir. O commit gelmeden uygulama gerçek jq'yu önizlemez bile.

Uygulanacak adımlar [PILOT-CHECKLIST.tr.md](PILOT-CHECKLIST.tr.md) dosyasındadır (hazırlıktır, izin değildir): izole boş klasör → envanter 0 → saklanan dosyanın yalnızca-okuma yeniden özeti → sizin elinizle `<klasör>\jq-pilot-input\` içine koyulan kopya → tam önizleme → **onay düğmesine yalnızca siz basarsınız** → envanter 1 ve SHA-256 karşılaştırması → SAC/Defender gözlemi (ayar değişmez) → dürüst sonuç; beklenmeyen sonuçta otomatik temizlik veya yeniden deneme yok.

## Bilmeniz gerekenler

* Uygulama içi onay **kimlik doğrulaması değildir**; aynı Windows hesabındaki başka bir program bu pencereyi kullanabilir.
* Kurulan dosya `artifact.bin` adıyla durur; jq komutu olmaz; çalıştığının kanıtı değildir. Smart App Control imzasız jq'yu ilk çalıştırmada büyük olasılıkla engeller (denenmeyecek).
* Bağımsız inceleme dar kapsamlıdır: Rust/ön yüz testlerini ve gerçek Windows penceresini inceleyici kendisi çalıştırmadı; bunlar geliştirici testleri ve CI ile desteklenir. Request A'nın ağ teması FAIL olarak kalır; kriptografik kanıt koşullu kabuldür. Lisans kabulü yeniden dağıtım izni değildir ve hukuki tavsiye değildir.
* Durma koşulları: özet/boyut uyuşmazlığı, önizlemede listelenmeyen bir şey, veri klasörü dışında herhangi bir fark, güvenlik yazılımı engeli, *tamamlandı*dan başka sonuç, sizin herhangi bir isteğiniz.

## Onay biçimi (siz doldurursunuz — ben doldurmam)

> "jq 1.8.2 (Windows x64) dosyasının, **\<seçtiğim tek izole klasör\>** içinde, yukarıdaki sınırlar içinde, **bir kez**, **çalıştırılmadan** kurulmasına izin veriyorum. Bu izin tek seferliktir; kaldırma yeniden kurulum izni vermez." — tarih, ad

yetki VERİLMEDİ: bu cümle yazılmadan ve yukarıdaki ön koşullar tamamlanmadan hiçbir adım atılmaz.
