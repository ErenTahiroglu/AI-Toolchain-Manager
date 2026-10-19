# jq kurulum yetkisi — TEKLİF (VERİLMEDİ, yürürlükte DEĞİL)

> Bu metin bir **tekliftir**. Siz açıkça "onaylıyorum" demedikçe hiçbir şey yapılmaz. Henüz yapılmayan üç ön koşul vardır (bağımsız tarif incelemesi, etkinleştirme değişikliğinin Red Team onayı, bu yetki): bkz. [READINESS-DECISION](READINESS-DECISION.md). O ön koşullar tamamlanmadan bu teklifin hiçbir maddesi uygulanamaz.

## İstenen yetki (yalnızca bu)

**jq 1.8.2 (Windows x64, `jq-windows-amd64.exe`, 1.035.264 bayt, SHA-256 `a6fc67fe…c627`) dosyasının, bu bilgisayarda, uygulamanın kendi klasörüne BİR KEZ kurulması. Dosya ÇALIŞTIRILMAYACAK.**

Bu yetki şunları **kapsar**:

1. **Yalnızca okuma:** Request A'dan saklanan doğrulanmış dosya ve kanıt oturumu okunur (yeniden özetlenir); hiçbiri değiştirilmez, silinmez, taşınmaz.
2. Dosyanın bir **kopyası**, bu deneme için ayrılmış, boş bir veri klasöründeki `pilot-input` klasörüne konur (özetle yeniden doğrulanır).
3. Uygulamanın, jq kurulumunu içeren özel (varsayılan olarak kapalı) derlemesi **yerelde** derlenir ve açılır.
4. **Onay düğmesine yalnızca siz basarsınız.** Önizlemede yazan her şeyi okuyup "okudum ve anladım" kutusunu işaretledikten sonra "Onaylıyorum ve kur"a siz basarsınız; ben basmam.
5. Kurulum öncesi ve sonrası dosya sistemi envanteri alınır ve karşılaştırılır; Smart App Control / Defender tepkisi yalnızca **kaydedilir**.
6. İsterseniz, ayrı bir önizleme ve **ikinci bir onayınızla** kaldırma denenir (geri alınabilirliği göstermek için).

Bu yetki şunları **kapsamaz** (hiçbiri yapılmaz):

* jq'yu **çalıştırmak** (`--version` dahil), PATH'e eklemek, kısayol/zamanlanmış görev/kayıt defteri girdisi oluşturmak, ajan ayarlarına dokunmak.
* **İndirme** (ağ kullanılmaz; yeni hiçbir adrese bağlanılmaz).
* Yönetici yetkisi, Smart App Control / Defender / güvenlik ayarlarında herhangi bir değişiklik.
* jq'yu dağıtmak, barındırmak, uygulamayla birlikte paketlemek (lisans kararı yalnızca bu bilgisayardaki yerel kullanım içindir).
* Başka bir araç, başka bir sürüm, güncelleme, macOS, başka bir bilgisayar.
* Windows Hello anahtarı oluşturmak veya kullanmak.

## Bilmeniz gerekenler (kısa)

* Kurulan dosya `artifact.bin` adıyla uygulamanın klasöründe durur; çalıştırılmaz. **Bu, jq'nun çalıştığının kanıtı değildir** ve Smart App Control imzasız jq'yu ilk çalıştırmada büyük olasılıkla engeller (bu pilotta denenmeyecek).
* Uygulama içi onay **kimlik doğrulaması değildir**: aynı Windows hesabındaki başka bir program bu pencereyi kullanabilir.
* Kanıt: Request A'nın kriptografik kanıtı "koşullu kabul"dür; ilk çevrimiçi çalıştırma onaylı listenin dışında bir adrese bağlanmıştı (kayıtlı, tekrarlanmadı).
* Hukuki not: bu bir mühendislik değerlendirmesidir, hukuki tavsiye değildir; yerel kullanım için "yeniden dağıtım sayılmaz" varsayımına dayanır.
* Durma koşulları (herhangi biri pilotu bitirir, hiçbir şey silinmez veya "düzeltilmez"): özet uyuşmazlığı, beklenmeyen önizleme, uygulama klasörü dışında herhangi bir fark, güvenlik yazılımı engeli/karantinası, "Tamamlandı"dan başka bir sonuç.

## Onay biçimi (siz doldurursunuz)

> "jq 1.8.2 (Windows x64) dosyasının yukarıdaki sınırlar içinde, bu bilgisayarda bir kez, **çalıştırılmadan** kurulmasına izin veriyorum." — tarih, imza/ad

Bu cümle olmadan hiçbir adım atılmaz.
