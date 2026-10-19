# Önerilen kullanıcı yetkileri — ilk gerçek jq kurulumuna giden iki dar adım (TEKLİF)

**Bu belge yalnızca bir TEKLİFTİR. Hiçbir yetki verilmemiştir, hiçbiri kullanılmamıştır.** Aşağıdaki iki talep birbirinden ayrıdır; her biri ayrı ve açık onayınızı ister. İkisi de Red Team incelemesinden ve aşağıdaki "Ön koşullar" tamamlandıktan sonra sorulacaktır. **Çalıştırma (jq'yu başlatmak, `--version` dahil) hiçbirinde yoktur.**

## Talep B — Windows Hello deneme kaydı (yalnızca bu bilgisayar, geçici deneme anahtarı)

**Amaç:** Windows Hello'nun bu uygulamadan gerçekten çalışıp çalışmadığını görmek: anahtar oluşturma, tek imza, doğrulama, silme. jq ile ilgisi yoktur.

**Ne olur (sırayla):**
1. Siz özel cümleyi yazarak onay güvenliğini açarsınız (`ENABLE-OS-APPROVAL <bilgisayar kimliği>`); açıkça yapılan, kayda geçen bir eylemdir.
2. Windows sizden **PIN/biyometri** ister ve **bir deneme anahtarı** (`atm-approval-<bilgisayar>`) oluşturulur. Özel anahtar bilgisayardan çıkmaz; uygulama yalnızca açık anahtarı ve onun özetini saklar.
3. Zararsız, uydurma bir onay metni için Windows sizden **bir kez daha** PIN/biyometri ister; imza uygulama tarafından doğrulanır (doğrulanamazsa hiçbir şey kaydedilmez).
4. Anahtarın donanım kanıtı (TPM raporu) varsa **kaydedilir ama doğrulanmış sayılmaz**.
5. Hemen ardından `DISABLE-OS-APPROVAL <bilgisayar kimliği>` ile anahtar **silinir** ve kayıt temizlenir (bu da ayrı yazdığınız bir cümledir).

**Etkiler:** iki Windows Hello istemi (+ silme için kimlik isteği çıkarsa bir tane daha); uygulama veri klasöründe küçük bir dosya (`approval-ledger.sqlite`, onay defteri ve denetim günlüğü); Windows'un kendi kimlik bilgisi deposunda **geçici** bir anahtar. Ağ bağlantısı yok. PIN/biyometri, mevcut Hello kayıtlarınız, parolalar, Smart App Control, Defender, PATH, ajan ayarları **değişmez**. Mevcut gerçek kimlik bilgilerinize dokunulmaz.

**Hata olursa:** işlem durur, deneme anahtarı silinir, sonuç rapor edilir. Anahtar silinemezse bunu size açıkça söylerim ve silme adımını tekrar denemek için sizden yeniden onay isterim.

**İstenmeyenler:** gerçek onay kullanımı, üretim onayı, jq'ya herhangi bir dokunuş, ikinci bir anahtar, başka bilgisayar.

**Onay cümleniz:** "Talep B'yi yukarıdaki haliyle onaylıyorum."

## Talep C — jq 1.8.2 Windows x64'ün İLK gerçek kurulumu (çalıştırmadan)

**Yalnızca şu dosya:** `jq-windows-amd64.exe`, 1.035.264 bayt, SHA-256 `a6fc67fedaf9128a3309a1e2ebb8b986aeccf70122ee46d2cb4849e423f0c627`. **Yeni indirme yoktur:** Request A oturumunda saklanan, doğrulanmış kopya kullanılır; kurulumdan önce uygulama dosyanın boyutunu ve SHA-256'sını yeniden kontrol eder, uyuşmazsa hiçbir şey yapmaz.

**Ne olur:**
1. Uygulama kurulum planını ve **tam etkileri** size gösterir (hangi dosya, hangi klasör, hangi özet).
2. Siz Windows Hello ile (PIN/biyometri) **tek kullanımlık, süreli** bir onay verirsiniz; onay yalnızca bu bilgisayara, bu tarife, bu plana, bu dosya özetine ve bu işleme bağlıdır.
3. Dosya, uygulamanın **kendi yönetilen klasörüne** kopyalanır: `…/atm-managed/root-<kimlik>/versions/jq/1.8.2/artifact.bin` (uzantısı bilerek `.exe` değildir), yanında küçük işaretçi/makbuz/günlük dosyaları. Kopyalama önce geçici klasörde yapılır, doğrulanır, sonra tek adımda etkinleştirilir; bir hata olursa her şey geri alınır.
4. Onay defterine ve denetim günlüğüne kayıt düşülür. Onay tüketilir (ikinci kez kullanılamaz).

**Etkiler:** yaklaşık 1 MB + birkaç KB dosya, yalnızca yönetilen klasörde. Defender dosyayı yazılırken tarayabilir (işletim sisteminin davranışı; karantina olursa kaydedilir). **Hiçbir şey çalıştırılmaz** — Smart App Control'ün imzasız `.exe`'yi çalıştırma tepkisi bu adımda sınanmaz.

**Geri alma:** (a) uygulamanın kendi kaldırma işlemi (ayrı bir Windows Hello onayı ister; jq'nun kullanıcı verisi klasörüne dokunulmaz); (b) Talep B'deki gibi anahtarın silinmesi; (c) Request A oturum klasörü hiç değişmez.

**Saklanan kanıt:** makbuz, onay defteri satırı, denetim zinciri doğrulaması, yönetilen klasörün dosya listesi ve özetleri — Red Team incelemesine kadar silinmez.

**İstenmeyenler / yasaklar:** jq'yu çalıştırmak (`--version` dahil), PATH'e eklemek, ajan ayarlarını değiştirmek, paket yöneticisi, güncelleme, başka araç/sürüm/platform, macOS, ağ bağlantısı, üretim referans listesine otomatik giriş.

**Onay cümleniz:** "Talep C'yi yukarıdaki haliyle onaylıyorum."

## Ön koşullar — bunlar tamamlanmadan Talep C sorulmayacak

1. Red Team, Phase 3D-1'i (bu belge dahil) kabul etmiş olmalı.
2. **Lisans:** `LICENSE_CONDITIONAL` şartları (C1–C3) yerine getirilmiş, açık sorular (Q1–Q3) yazılı olarak çözülmüş veya kabul edilmiş.
3. **Referans ve tarif:** tek bir doğrulanmış referans girdisi ve gözden geçirilmiş tarif, ayrı bir kod değişikliği olarak Red Team tarafından kabul edilmiş. (Bugün ikisi de **yok**.)
4. **Onay yolu:** Windows Hello için seçenek A/B/C kararı verilmiş (donanım kanıtı doğrulaması) ve Talep B sonuçları kabul edilmiş.
5. **Cihaz kanıtı (Issue #14):** bağımsız ikinci bir Windows bilgisayarda sınama (H1–H2) sonuçları ve dosya değiştirme penceresi kararı (H3). Sanal makine tek başına yeterli sayılmaz (ayrıntı: `ISSUE-14-WINDOWS-PILOT-HARDWARE-GATE.md`).
6. **Kod:** bugün gerçek kurulum yolu **kapalıdır** (`Adapter::production` her şeyi reddeder, arayüzde kurulum düğmesi yoktur). Yalnızca bu tek tarifi, bu tek özeti ve tek kez çalışan bir yolu açan, kendi testleri olan ve Red Team'in incelediği ayrı bir kod aşaması gerekir. O aşama da bu iki talepten **bağımsız** olarak sizin onayınıza tabidir.
7. Bu taleplerin onayı, Red Team'in ayrı yazılı onayı olmadan **geçerli sayılmaz**.
