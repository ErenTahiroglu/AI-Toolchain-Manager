# Diğer ajanlar ve MCP/Skills kullanım kılavuzu (uygulamadaki metnin belge hâli)

Bu kılavuz, uygulamanın **yönetmediği her ajan istemcisi** için tek, ortak ve **elle** izlenen bir iş akışıdır. Uygulamadaki karşılığı Projeler sayfasındaki “Diğer ajanlar ve MCP/Skills kullanım kılavuzu — elle, salt okunur” bölümüdür (`src/components/AgentGuide.tsx`; metin `src/agentGuide.ts` ve `src/agentHosts.ts` içindedir). Kılavuz **dosya okumaz veya yazmaz, ağa bağlanmaz, hesap veya anahtar istemez, hiçbir şey kurmaz ve tercihinizi değiştirmez.** Her marka veya sağlayıcı için ayrı bir bağdaştırıcı yoktur; Gemini CLI, Grok/xAI, Groq ve DeepSeek yalnızca açıklayıcı örnektir.

> Tüm istemciler için tek bir ortak `settings.json` veya tek bir birlikte çalışabilir yapılandırma biçimi **yoktur.** Aynı MCP sunucusu için Claude Code `.mcp.json` (JSON), Antigravity `.agents/mcp_config.json` (JSON) ve Codex `.codex/config.toml` (TOML) farklı anahtarlar ve kurallar kullanır.

## Altı adım

1. **Adı sınıflandırın.** Adı verilen şey ne? (a) MCP araçlarını ve/veya skill'leri bulup kullanabilen bir ajan istemcisi (CLI, IDE eklentisi, masaüstü uygulaması), (b) yalnızca yönerge dosyası okuyan bir istemci, (c) bir istemcinin çağırdığı model/API sağlayıcısı, yoksa (d) belirsiz mi? Marka adı bunu belirlemez: DeepSeek, Grok, Groq ve Gemini API sağlayıcıdır; Gemini CLI bir istemcidir.
2. **Resmî desteği doğrulayın.** Yayıncının kendi belgesinden bulun: hangi MCP aktarımı (stdio, Streamable HTTP …), yapılandırma şeması (JSON mu, TOML mu, başka mı), dosyanın tam konumu ve hangi kapsamda (proje, kullanıcı, sistem) okunduğu. Dosya yolunu bir sağlayıcının adından veya başka bir istemciden tahmin etmeyin.
3. **Farklı şeyleri birbirinden ayırın.** MCP sunucuları, skill'ler (`SKILL.md`), ajan eklentileri, proje yönerge dosyaları (`AGENTS.md`, `CLAUDE.md`), barındırılan API modelleri ve çalışma ortamları farklı şeylerdir. Hiçbiri tek bir ortak biçimi paylaşmaz. Bir MCP sunucusu birden çok istemcide aynı olabilir; ama her istemcinin kendi dosyası, anahtar adları ve onay kuralları vardır.
4. **Kapsamı ve riski belirleyin.** İşletim sistemi (Windows x64 / macOS Apple Silicon), CLI mı IDE mi, kullanıcı mı proje mi kapsamı, gereken ayrıcalıklar, API anahtarının nerede tutulduğu, hangi alt süreçlerin başlatıldığı ve hangi ağ bağlantılarının kurulduğu. Paket çalıştırıcıları (`npx`, `uvx`) her başlatmada kayıt defterinden kod indirir ve çalıştırır.
5. **Kanıt eksikse `MANUAL_VERIFY`.** Resmî kanıt bulamadıysanız çıktı `MANUAL_VERIFY` olmalıdır: ne yapılandırma farkı, ne “uyumlu”, ne “kurulu”. Bilinmeyen hiçbir zaman desteklenen anlamına gelmez. Uygulama bilinmeyen bir istemci için dosya üretmez ve tercih olarak “kullanmak istiyorum” kaydetmez.
6. **Örnekler yalnızca açıklayıcıdır.** Aşağıdaki örnekler nasıl düşüneceğinizi gösterir; gelecekte adı geçmeyen istemciler de aynı elle iş akışını kullanır.

## Farklı şeyler, farklı yerler

| Şey | Nedir | Paylaşılır mı? |
|---|---|---|
| MCP sunucusu | Bir istemciye araç ve kaynak sunan program veya uzak adres (stdio ya da HTTP). | Sunucunun kendisi paylaşılabilir; her istemcinin tanım dosyası ve onay kuralları ayrıdır. |
| Skill (`SKILL.md`) | Bir istemcinin yükleyebildiği yeniden kullanılabilir yönergeler. | Biçim paylaşılabilir; nereye konacağı ve yüklenip yüklenmediği istemciye bağlıdır. |
| Ajan eklentisi / pazaryeri | Bir istemcinin kendi paketleme ve yönetim mekanizması. | İstemciye özgüdür; başka bir istemciye taşınmaz. |
| Proje yönerge dosyası | `AGENTS.md`, `CLAUDE.md` gibi metin yönergeleri. | Yapılandırma değildir; bir MCP sunucusu tanımlamaz. |
| Model / API sağlayıcısı | Bir istemcinin çağırdığı barındırılan model (API anahtarı ve adresle). | İstemci değildir; proje MCP dosyası yoktur. Sağlayıcı ayarı istemcinin KENDİ yapılandırmasındadır. |
| Çalışma ortamı (runtime) | Bir sunucuyu çalıştıran Node, Python, Docker gibi ortam. | Ayrı bir kurulum ve güven kararıdır. |

## Örnekler (yalnızca açıklayıcı)

| Ad | Sınıf | Not |
|---|---|---|
| Claude Code | Ajan istemcisi — uygulamada seçilebilir hedef | Proje dosyası JSON (`.mcp.json`). |
| Antigravity IDE | Ajan istemcisi — uygulamada seçilebilir hedef | Proje dosyası JSON (`.agents/mcp_config.json`). Uygulama, CLI ve “2” arayüzleri birbirinin yerine geçmez. |
| Codex CLI | Ajan istemcisi — uygulamada seçilebilir hedef | Proje dosyası **TOML** (`.codex/config.toml`, yalnızca **güvenilen** projede). TOML önizlemesi uygulanmadı; yalnızca elle yönerge var. |
| Gemini CLI | Ajan istemcisi — yalnızca örnek | JSON; kullanıcı `~/.gemini/settings.json`, proje `.gemini/settings.json`. Uygulama yönetmez. Yayıncı sayfası bazı kullanıcı gruplarında Antigravity CLI ile değiştirildiğini belirtir. |
| Gemini API | Model / API sağlayıcısı | “Gemini CLI” ile aynı şey değildir. |
| DeepSeek | Model / API sağlayıcısı | Yayıncı sayfası onu Codex'te bir model sağlayıcısı olarak tanımlamayı anlatır; sağlayıcı anahtarları proje dosyasında yok sayılır. API anahtarı bu uygulamaya girilmez; sayfadaki “tek tıkla kurulum” betiği (boruyla kabuğa) hiçbir zaman kullanılmaz. |
| Grok (xAI) | Model / API sağlayıcısı | Groq ile aynı değildir. Yerel MCP istemcisi veya proje dosyası belgelenmemiş. |
| Groq (GroqCloud) | Model / API sağlayıcısı | Grok (xAI) ile aynı değildir. Kendi API'sinde “uzak MCP” aracı belgeler: bu sağlayıcı tarafı bir özelliktir, yerel istemci yapılandırması değildir. |
| Başka herhangi bir ad | Belirsiz | `MANUAL_VERIFY` — yukarıdaki 2. adımı yayıncının belgesinden elle yapın. |

## Uygulamanın gösterdiği “ad sınıflandırıcı”

Bir ad yazıp “Sınıflandır” düğmesine basarsınız. Sonuç yalnızca yukarıdaki tabloya **tam eşleşmedir** (büyük/küçük harf, aksan ve boşluk duyarsız; **bulanık eşleşme yok**): `Grok` ile `Groq` farklıdır, `gemini` tek başına belirsizdir, `groc`/`grokk`/`codex` (eksik ad) belirsizdir. Eşleşme yoksa `MANUAL_VERIFY` gösterilir; hiçbir dosya yolu, biçim, “uyumlu” veya “kurulu” sözü üretilmez. Hiçbir şey kaydedilmez, kopyalanmaz ve gönderilmez.
