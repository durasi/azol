<p align="center">
  <a href="https://github.com/durasi/azol#readme"><img src="assets/lang/en-off.svg" alt="English" height="28"></a>
  <a href="README.tr.md"><img src="assets/lang/tr-on.svg" alt="Türkçe" height="28"></a>
</p>

<p align="center">
  <img src="assets/logo.svg#gh-light-mode-only" alt="azoL" height="104">
  <img src="assets/logo-dark.svg#gh-dark-mode-only" alt="azoL" height="104">
</p>

<p align="center"><em>The rust remover.</em><br><sub>Pas sökücü.</sub></p>

<p align="center">
  <strong>Garbage collector'ı ve lifetime annotation'ları olmayan, bellek güvenli bir sistem dili;<br>
  derleyicisi 44.000 satırı 25 milisaniyede çalıştırılabilir dosyaya çevirir.</strong>
</p>

<p align="center">
  <a href="#sayılar">Sayılar</a> ·
  <a href="#azol-rust-c-go-ve-javascript">Karşılaştırma</a> ·
  <a href="#hızlı-başlangıç">Hızlı başlangıç</a> ·
  <a href="docs/language.md">Dil başvurusu</a> ·
  <a href="AGENTS.md">Yapay zekâ ajanları için</a> ·
  <a href="docs/for-agents.md">Kısa bilgilendirme</a>
</p>

---

azoL, Rust ile yazacağınız türden programlar içindir: garbage collector yok, dangling pointer yok, double free yok, memory leak yok. Buraya başvurular yerine değerlerle varır, bu yüzden açıklama yazılacak bir şey kalmaz; derleyicisi de kaynağınızı okuyup çalışmaya hazır dosyayı tek başına yazar: LLVM yok, assembler yok, linker yok, `codesign` yok, bağımlılık yok.

Genç ve küçük bir dildir. Üzerine bir şey planlamadan önce [nelerin olmadığını](#azolda-olmayanlar) okuyun.

Bu sayfa İngilizce [README.md](README.md) dosyasının eksiksiz çevirisidir. `docs/` altındaki belgeler, hata iletileri ve kod İngilizcedir.

## Bir tadımlık

```
struct Player {
    name: str
    scores: List[i64]
}

fn total(p: Player) -> i64 {
    var sum = 0
    for score in p.scores {
        sum = sum + score
    }
    return sum
}

fn main() {
    var ada = Player { name: "Ada", scores: [12, 30] }
    ada.scores.push(25)

    var grace = ada
    grace.name = "Grace"
    grace.scores.push(40)

    for player in [ada, grace] {
        print(player.name + ": " + str(total(player)) + " points")
    }
}
```

```text
Ada: 67 points
Grace: 107 points
```

`grace` kendi listesi olan bir kopyadır; ona ekleme yapmak `ada`'ya dokunmaz. Yol boyunca oluşan metinler ve listeler, onlara sahip olan kalmadığı anda serbest bırakılır. Bu programın hiçbir yerinde bunun nasıl yapılacağı yazmaz.

## Sayılar

Tek tablo, tek makine, [`bench/compare.py`](bench/compare.py) betiğinin tek çalıştırması; bu bölümü betiğin kendisi yazar (`python3 bench/compare.py --write-readme`). Her program, aynı algoritmanın her dilde yalın biçimde yazılmış hâlidir ([`bench/`](bench/)) ve bütün sürümler aynı sonucu yazdırmak zorundadır. Bu sayfadaki ölçüm sayıları (tablo ve oranlar) araçların yazdığı biçimdedir ve İngilizce sayfadakiyle aynıdır: ondalık ayırıcı nokta, binlik ayırıcı virgüldür.

<!-- bench:start -->
Apple M5, Darwin 27.2.0 (arm64) üzerinde ölçüldü. 5 çalıştırmanın en iyisi.
Araçlar: azol 0.1.0; rustc 1.99.1 (58cf8f9c3 2026-10-13); Apple clang version 21.0.0 (clang-2100.4.1.1); go version go1.27.1 darwin/arm64; node v22.17.0.

Her yerde düşük olan daha iyidir. Her satırda en iyi değer kalın yazılmıştır; tabloda başka hiçbir şey kalın değildir. *ölçülecek* yazan hücreler bu makinede henüz çalıştırılmadı; bir satırın tamamı ölçülene kadar o satırda kalın yoktur.

|  | <img src="assets/compare/azol.svg" alt="azoL" height="28"><br><sub>varsayılan</sub> | <img src="assets/compare/azol.svg" alt="azoL" height="28"><br><sub><code>-O</code></sub> | <img src="assets/third-party/rust-logo-blk.svg#gh-light-mode-only" alt="Rust" height="28"><img src="assets/compare/rust.svg#gh-dark-mode-only" alt="Rust" height="28"><br><sub>Rust&nbsp;<code>-O</code></sub> | <img src="assets/compare/c.svg" alt="C" height="28"><br><sub><code>-O2</code></sub> | <img src="assets/compare/go.svg" alt="Go" height="28"><br><sub>go&nbsp;build</sub> | <img src="assets/third-party/js.png" alt="JavaScript" height="28"><br><sub>Node.js</sub> |
|:---|---:|---:|---:|---:|---:|---:|
| <sub>DERLEME SÜRESİ</sub> | | | | | | |
| Aynı 44,004 satırlık program | **0.025&nbsp;s** | 0.38&nbsp;s | 9.46&nbsp;s | 131.60&nbsp;s | 0.81&nbsp;s | derleme&nbsp;yok |
| <sub>ÇALIŞTIRILABİLİR DOSYA BOYUTU</sub> | | | | | | |
| `fib40` | **33&nbsp;KB** | **33&nbsp;KB** | 460&nbsp;KB | **33&nbsp;KB** | 2373&nbsp;KB | Node.js&nbsp;gerekir |
| <sub>ÇALIŞMA SÜRESİ</sub> | | | | | | |
| `fib40` <sub>özyinelemeli çağrılar</sub> | 0.25&nbsp;s | **0.15&nbsp;s** | **0.15&nbsp;s** | **0.15&nbsp;s** | 0.22&nbsp;s | 0.46&nbsp;s |
| `loops` <sub>iç içe döngüler</sub> | 0.016&nbsp;s | **0.014&nbsp;s** | 0.018&nbsp;s | 0.018&nbsp;s | 0.018&nbsp;s | 0.041&nbsp;s |
| `sort` <sub>quicksort, 2 milyon sayı</sub> | 0.13&nbsp;s | 0.11&nbsp;s | 0.11&nbsp;s | **0.10&nbsp;s** | 0.11&nbsp;s | 0.27&nbsp;s |
| `sieve` <sub>20 milyona kadar asal sayılar</sub> | 0.080&nbsp;s | 0.071&nbsp;s | 0.045&nbsp;s | 0.042&nbsp;s | **0.037&nbsp;s** | 0.29&nbsp;s |
| `particles` <sub>yerinde güncellenen struct'lar</sub> | 0.097&nbsp;s | 0.074&nbsp;s | **0.039&nbsp;s** | 0.050&nbsp;s | 0.052&nbsp;s | 0.17&nbsp;s |
| `tree` <sub>ağaç kurma ve dolaşma</sub> | 0.091&nbsp;s | 0.082&nbsp;s | **0.053&nbsp;s** | 0.054&nbsp;s | 0.064&nbsp;s | 0.36&nbsp;s |
| `append` <sub>uzun metinler oluşturma</sub> | 0.023&nbsp;s | 0.023&nbsp;s | 0.020&nbsp;s | **0.018&nbsp;s** | 0.026&nbsp;s | 0.11&nbsp;s |
| `words` <sub>metni bölme ve birleştirme</sub> | 0.035&nbsp;s | 0.030&nbsp;s | 0.026&nbsp;s | 0.026&nbsp;s | **0.019&nbsp;s** | 0.10&nbsp;s |
| <sub>AI AJANLARI İÇİN</sub> | | | | | | |
| Derlemeden denetleme <sub>aynı 44,004 satırlık program</sub> | ölçülecek | varsayılanla&nbsp;aynı | ölçülecek | ölçülecek | ölçülecek | ölçülecek |
| Kod satırı <sub>8 program</sub> | 231 | varsayılanla&nbsp;aynı | 258 | 401 | 253 | **220** |
| Karakter <sub>8 program</sub> | 3,759 | varsayılanla&nbsp;aynı | 4,717 | 8,380 | 4,238 | **3,616** |
| Token, yaklaşık <sub>8 program</sub> | **1,376** | varsayılanla&nbsp;aynı | 1,778 | 2,966 | 1,429 | 1,397 |
| Yanlış türde argüman <sub>denetleyicinin bildirdiği</sub> | ölçülecek | varsayılanla&nbsp;aynı | ölçülecek | ölçülecek | ölçülecek | ölçülecek |

Üretilen programın uzunluğu: 44,004 satır (azoL), 45,006 satır (Rust), 49,035 satır (C), 44,008 satır (Go), 38,002 satır (JavaScript).

Derleme: azoL'un varsayılan derlemesi `rustc -O` (9.46 s) karşısında 380 kat hızlı, optimizasyonsuz `rustc` derlemesi (1.48 s) karşısında 60 kat hızlı, yalnızca `rustc` tür denetimi (`--emit=metadata`) (0.78 s) karşısında 31 kat hızlı, `cc -O2` (131.60 s) karşısında 5300 kat hızlı, `go build` (0.81 s) karşısında 33 kat hızlı.

Node.js için derlenecek bir şey yoktur; JavaScript sürümünün sözdizimini denetlemek (`node --check`) 0.027 s sürer.

Çalışma: azoL'un varsayılan derlemesinin önde olduğu program sayısı: Rust `-O` karşısında 8 programdan 1, C `-O2` karşısında 8 programdan 1, Go karşısında 8 programdan 2, Node.js karşısında 8 programdan 8.

Çalışma: `azol build -O` derlemesinin önde olduğu program sayısı: Rust `-O` karşısında 8 programdan 1 (2 programda eşit), C `-O2` karşısında 8 programdan 1 (1 programda eşit), Go karşısında 8 programdan 3 (1 programda eşit), Node.js karşısında 8 programdan 8.

Denetleme: komutlar şunlardır: `azol check`, `rustc --emit=metadata`, `cc -fsyntax-only`, `go vet`, `node --check`.

Kaynak: 8 kıyaslama programının toplamı; yorumlar, boş satırlar ve girintiler sayılmaz. Token'lar her dil için tek bir kuralla sayılır (bir ad, bir sayı ya da bir metin tek token'dır; her parantez, virgül ya da noktalı virgül ve her operatör karakteri dizisi de öyle); bu, bir modelin okuyup yazdığına yaklaşık bir ölçüdür, herhangi bir modelin tokenizer'ı değildir.

Hata: her dilde aynı ([`bench/agents/`](bench/agents/)): sayı beklenen yere `"40"` metni verilir. *JSON*, aracın istendiğinde iletiyi ve yeri veri olarak verdiği anlamına gelir; *span*, yerin başı ve sonu; *fix*, bir aracın uygulayabileceği bir düzenleme; *metin, konum*, içinde satır ve sütun geçen, okunacak bir satır.
<!-- bench:end -->

**Bunları adil okumak için.**

- azoL'un iki sütunu var. İlki varsayılan derlemesidir: tek ve hızlı bir geçiş. İkincisi `azol build -O`, yani optimizer'dır: yeni, deneysel ve derlemesi daha yavaş. İkisi de her indeksi ve her bölmeyi denetler.
- Rust ve Go da indeksleri denetler; C hiçbir şeyi denetlemez. Go ve JavaScript çöp toplar; azoL, Rust ve C belleği belirli noktalarda serbest bırakır.
- Derleme süresi programı üretilmiştir ve kendini tekrar eder ([`bench/gen.py`](bench/gen.py)), üstelik tek dosyadır. Bu, diğerlerinden daha az iş yapan azoL'un lehinedir (trait yok, küçük bir kütüphane var) ve projeleri yan yana derlenen çok sayıda dosyaya bölünen C'nin aleyhinedir. Go'nun derleme önbelleği standart kütüphanesi için korunur, program için devre dışı bırakılır.
- Çalışma süreleri bütün bir sürecin süresidir, yani başlangıcı da içerir; bu, Node.js için onlarca milisaniyedir.
- AI ajanları için olan satırlar, bir betiğin ölçebildiğini ölçer: bir denetimin ne kadar sürdüğünü, ne kadar kaynak yazmak gerektiğini ve bir denetleyicinin tek bir hatayı hangi biçimde bildirdiğini. Denetleme komutları aynı işi yapmaz: `azol check`, `rustc --emit=metadata`, `cc -fsyntax-only` ve `go vet` türleri denetler, `go vet` ayrıca başka çözümlemeler de çalıştırır, `node --check` ise yalnızca sözdizimini okur. Daha kısa bir kaynak tek başına daha iyi bir kaynak değildir; sayılar bir modelin ne kadar üretmesi gerektiğini söyler, doğru yazmanın ne kadar kolay olduğunu değil.
- Bir ajanın bir programı ilk derlemede doğru yazıp yazmadığı ayrıca ve yalnızca iki dil için ölçüldü: elinde belgelerden başka yardım olmayan, sıfırdan başlayan ajanlar 34 azoL programından 33'ünü, 32 Rust programından 32'sini ilk denemede doğru derledi. Bu, bir modelin azoL'u belgelerinden, üzerinde eğitildiği bir dil kadar güvenilir biçimde yazabildiğini gösterir; iki dil, küçük programlar ve tek bir model söz konusu olduğu için tabloda yer almaz.
- Bunlar sekiz küçük programdır, bir kıyaslama paketi değildir. azoL'un kendine koyduğu ölçüt daha büyüktür: `rustc -O` karşısında 28 program ([`bench/suite/`](bench/suite/), yol haritasına bakın).

## azoL, Rust, C, Go ve JavaScript

Her satırda, o satırda önde olan diller kalın yazılmıştır; bu yalnızca açıkça söylenebildiği yerde yapılmıştır. Kalın olmayan satırlar trade-off'tur.

| | <img src="assets/compare/azol.svg" alt="azoL" height="30"> | <img src="assets/third-party/rust-logo-blk.svg#gh-light-mode-only" alt="Rust" height="30"><img src="assets/compare/rust.svg#gh-dark-mode-only" alt="Rust" height="30"><br><sub>Rust</sub> | <img src="assets/compare/c.svg" alt="C" height="30"> | <img src="assets/compare/go.svg" alt="Go" height="30"> | <img src="assets/third-party/js.png" alt="JavaScript" height="30"><br><sub>JavaScript</sub> |
|:---|:---|:---|:---|:---|:---|
| <sub>GÜVENLİK</sub> | | | | | |
| Garbage collector olmadan bellek güvenliği | **Var, derleme sırasında denetlenir** | **Var, derleme sırasında denetlenir; `unsafe` devre dışı bırakır** | Yok | Güvenli, garbage collector ile | Güvenli, garbage collector ile |
| Lifetime annotation'lar | Hiçbir zaman | Derleyicinin çıkaramadığı yerlerde | Yok | Yok | Yok |
| Kodunuzdaki başvurular | Yok. Yalnızca değerler; bir fonksiyonun argümanını değiştirebilmesi için çağrı `mut` der | `&T`, `&mut T`, akıllı işaretçiler | İşaretçiler | İşaretçiler | Her nesne paylaşılan bir başvurudur |
| Data race'ler | Henüz thread yok | **Derleme sırasında engellenir** | Olabilir | Olabilir; testler için bir yarış algılayıcı var | Varsayılan olarak paylaşılan bellek yok |
| Null | **Yok. `Option[T]`** | **Güvenli kodda yok. `Option<T>`** | `NULL` | `nil` | `null` ve `undefined` |
| Hatalar | `Result[T, E]`, `?`, eksiksiz `match` | `Result<T, E>`, `?`, eksiksiz `match`, panikler | Dönüş kodları | Hata değerleri, `panic` | İstisnalar |
| <sub>DİL</sub> | | | | | |
| Türler | Statik, fonksiyon içinde çıkarımlı | Statik, fonksiyon içinde çıkarımlı | Statik | Statik, fonksiyon içinde çıkarımlı | Dinamik |
| Generic'ler | Fonksiyonlar, struct'lar ve enum'lar. Henüz sınır (bound) yok | **Trait'ler ve sınırlarla** | Yok; makrolar | **Kısıtlarla** | Gerekmez |
| Sayılar | Yalnızca `i64`. Floating point yok | **Her boyutta tamsayı, floating point** | **Her boyutta tamsayı, floating point** | **Her boyutta tamsayı, floating point** | 64 bit floating point, `BigInt` |
| Closure'lar | Yok | **Var** | Yok; fonksiyon işaretçileri | **Var** | **Var** |
| Thread'ler | Yok | **Var** | Var, kütüphane aracılığıyla | **Var: goroutine'ler ve kanallar** | Worker'lar ve bir olay döngüsü |
| Paketler | Yok. Birkaç dosya, tek ad alanı | **Cargo, crates.io** | Standart bir tane yok | **Go modülleri** | **npm** |
| <sub>DERLEME VE DAĞITIM</sub> | | | | | |
| Kaynaktan çalışmaya | Önceden derlenir | Önceden derlenir | Önceden derlenir | Önceden derlenir | Çalışırken derlenir |
| Ne dağıtırsınız | Tek çalıştırılabilir dosya | Tek çalıştırılabilir dosya | Tek çalıştırılabilir dosya | Tek çalıştırılabilir dosya | Kaynak; kullanıcıda bir çalışma zamanı gerekir |
| Başka bir sistem için derleme | **Yerleşik: `--target`** | Kurulacak bir hedef ve onun için bir linker | Bir cross compiler | **Yerleşik: `GOOS`, `GOARCH`** | Geçerli değil |
| Hedefler | Apple Silicon macOS, x86-64 Linux, ARM64 Linux | Onlarca | **Neredeyse her işlemci** | Onlarca | Bir motorun çalıştığı her yer |
| <sub>ARAÇLAR</sub> | | | | | |
| Araçlar ve yapay zekâ ajanları için hatalar | `check --json`: kod, tam aralık, çiğnenen kural ve onarım mekanikse yapılacak düzenleme | `--error-format=json`, önerilen düzenlemeler ne kadar güvenli olduklarına göre işaretlenir | Metin, düzeltme ipuçlarıyla | Metin | Derleyici yok; hatalar kod çalışırken ortaya çıkar |
| Formatter | **`azol fmt`, tek düzen** | **`rustfmt`, araç zinciriyle gelir** | Ayrı araçlar | **`gofmt`, tek düzen** | Ayrı araçlar |
| <sub>PROJE</sub> | | | | | |
| Yazıldığı dil | azoL: derleyici kendini derler | Rust | Derleyiciler C ve C++ ile yazılır | Go | Motorlar C++ ile yazılır |
| Olgunluk | Deneysel. Ona bel bağlamayın | **2015'ten beri kararlı** | **1989'dan beri bir standart** | **2012'den beri kararlı** | **1995'ten beri kullanımda** |

Rust, C, Go ve JavaScript'in her biri, bugün azoL'un yapabildiğinden çok daha fazlasını yapabilir. azoL'un sunduğu şudur: derleme sırasında denetlenen, garbage collector'ı ve lifetime annotation'ları olmayan bellek güvenliği, milisaniyelerle ölçülen bir derleme ve bütün başvurusu tek belge olan bir dil.

<sub>Rust logosu Rust Foundation'a, JS logosu topluluğuna aittir; yalnızca sütunları etiketlerler. C ve Go adlarıyla etiketlenmiştir. azoL bunların hiçbiriyle bağlantılı değildir ve hiçbiri tarafından onaylanmamıştır. Kaynaklar ve koşullar: [assets/third-party](assets/third-party/README.md).</sub>

## Hızlı başlangıç

Bir kabuktan başka hiçbir şey gerekmez: Rust yok, C derleyicisi yok, linker yok, paket yöneticisi yok. Apple Silicon Mac'te ve x86-64 Linux'ta çalışır; ARM64 Linux aynı biçimde derlenir ama yalnızca bir emülatörde çalıştırılmıştır.

```sh
git clone https://github.com/durasi/azol
cd azol
sh build.sh
./azol build examples/players.azl
./examples/players
```

`build.sh`, [`bootstrap/`](bootstrap/) içinde tutulan, makinenize uygun küçük derleyiciyi alır; ona [`compiler/`](compiler/) içindeki azoL kaynaklarını derletir, çıkan sonuca aynı kaynakları iki kez daha derletir ve son iki çıktının aynı dosya olduğunu denetler. Bu bir saniyeden çok daha kısa sürer ve geriye `./azol` kalır.

azoL ile yazılmış gerçek bir komut satırı aracı:

```sh
./azol build examples/wc.azl
./examples/wc README.md lib/prelude.azl
```

Çalıştırılabilecek diğer örnekler: `examples/generics.azl`, `examples/bank.azl`, `examples/tree.azl`, `examples/lists.azl`, `examples/structs.azl`, `examples/fib.azl`. Testleri çalıştırmak için: `sh test.sh` (Python 3 gerekir). Ölçümleri yinelemek için: `python3 bench/compare.py` (kuruluysa `rustc` ve `node` kullanılır).

## Beş dakikada dil

Dilin tamamı [docs/language.md](docs/language.md) belgesindedir; oradaki her örnek derleyiciyle sınanmıştır. Burası kısa sürümdür.

**Null yok, istisna yok.** Bulunmayabilecek bir değer `Option`'dır, başarısız olabilecek bir şey `Result` döndürür, `?` başarısızlığı yukarı iletir ve `match` sizi her durumu ele almaya zorlar.

```
struct Account {
    owner: str
    balance: i64
}

enum Failure {
    NoSuchAccount(str)
    NotEnough(i64)
}

fn find(accounts: List[Account], owner: str) -> Result[i64, Failure] {
    for i, account in accounts {
        if account.owner == owner {
            return Ok(i)
        }
    }
    return Err(NoSuchAccount(owner))
}

fn withdraw(accounts: mut List[Account], owner: str, amount: i64) -> Result[i64, Failure] {
    let i = find(accounts, owner)?
    let balance = accounts[i].balance
    if balance < amount {
        return Err(NotEnough(amount - balance))
    }
    accounts[i].balance = balance - amount
    return Ok(balance - amount)
}
```

`withdraw(mut accounts, "Ada", 70)` ilk seferde `Ok(30)`, ikincisinde `Err(NotEnough(40))` döndürür. Bir `match` içinde bir kolu atlarsanız program derlenmez:

```text
error[E0501]: this `match` does not handle `None`; add an arm for it or a final `_ =>` arm
```

**Törensiz generic'ler.** Her türle çalışan kod bir kez yazılır. Derleyici her çağrıda türleri bulur ve her biri için bir sürüm üretir.

```
struct Stack[T] {
    items: List[T]
}

fn pop[T](stack: mut Stack[T]) -> Option[T] {
    if stack.items.len() == 0 {
        return None
    }
    return Some(stack.items.pop())
}
```

**azoL ile yazılmış bir standart kütüphane.** Metin (`split`, `trim`, `replace`, `find`, `to_i64`, ...), listeler (`sorted`, `reversed`, `contains`, `extend`, ...), sayılar (`abs`, `pow`, `clamp`, `to_hex`, ...) ve metin anahtarlı bir map, [`lib/prelude.azl`](lib/prelude.azl) dosyasındadır. Programa yalnızca kullandığı kadarı derlenir.

```
fn main() {
    var seen: Map[i64] = new_map()
    for word in "the cat and the hat and the bat".split(" ") {
        match seen.get(word) {
            Some(count) => seen.set(word, count + 1)
            None => seen.set(word, 1)
        }
    }
    for word in ["the", "and", "dog"] {
        match seen.get(word) {
            Some(count) => print(word + ": " + str(count))
            None => print(word + ": never")
        }
    }
}
```

```text
the: 3
and: 2
dog: never
```

**Ayrıca var:** struct içinde struct, varyantları değer taşıyan enum'lar, her şeyin `List[T]`'si, `const`, bit işleçleri, onaltılık, ikilik ve karakter sabitleri, indeksli `for`, birkaç dosyaya yayılmış programlar, `Result` döndüren `read_file` / `write_file`, `args()` ve `exit()`.

### azoL'da olmayanlar

Floating point sayılar, `i64` dışındaki tamsayı türleri, closure'lar, trait'ler, kendi türleriniz üzerinde metotlar, thread'ler, modüller ya da bir paket yöneticisi. Tamsayı taşması başa sarar. [Başvuru belgesi bunların hepsini](docs/language.md#19-what-azol-does-not-have), her birinin yerine ne yazılacağıyla birlikte listeler.

## Güvenlik nasıl çalışır

Değerler value semantics'e sahiptir: bir struct'ı, bir metni ya da bir listeyi atamak size bağımsız bir kopya verir ve bir fonksiyon bir argümanı ancak çağıran `mut` yazdığında değiştirebilir. Zihinsel modelin tamamı budur.

Altta, büyük değerler hız için başvuruyla geçirilir ve derleyici bunu asla fark edemeyeceğinizi garanti eder. Bir başvuru tam olarak özel bir kopya gibi davranır, çünkü başka bir şey ona hâlâ başvururken hiçbir şey değiştirilemez. Bunu dört kural sağlar; her biri tek seferde tek bir fonksiyonun içinde denetlenir, denetimi hızlı tutan da budur:

- Tek bir çağrı, kullanımlardan biri `mut` iken aynı veriyi iki kez alamaz.
- Bir döngü üzerinden geçerken liste değiştirilemez.
- Bir `match` içine bakarken değer değiştirilemez.
- Bir işleç onu hâlâ okurken metin değiştirilemez.

Bir kural çiğnendiğinde bir bulmaca değil, yalın bir cümle alırsınız:

```text
error[E0408]: cannot change `xs` while a loop is going through it
 --> loop.azl:4:9
  |
4 |         xs.push(x)
  |         ^
```

Bellek ownership ile yönetilir. Her arabelleğin tam olarak bir sahibi vardır: bir değişken ya da bir geçici değer. Geçici değer saklandığında arabelleğini devreder; bir değişken yeniden saklandığında çoğaltılır, ama bu onun son kullanımıysa o da arabelleğini devreder; her arabellek de sahibi kapsam dışına çıktığında serbest bırakılır. Kopyalar programın anlamında vardır, makinede olmak zorunda değildir: `text = text + more`, `text`'e yerinde ekleme yapar. Bu her derinlikte çalışır: her biri bir düğüm listesine sahip düğümlerden oluşan bir ağaç, elle hiçbir şey yazılmadan doğru kopyalanır ve doğru serbest bırakılır.

Aralık dışı indeksler ve dilimler, boş bir listede `pop`, sıfıra bölme ve hiç bitmeyen özyineleme, programı `azol: stack overflow` gibi bir iletiyle durdurur. Okumamaları ya da yazmamaları gereken belleği asla okumaz ve yazmazlar.

## Yapay zekâ ajanları için tasarlandı

Hiçbir model azoL'u görmedi; bu yüzden bir ajanın gerek duyduğu her şey depoda ve derleyicinin çıktısındadır.

- [docs/for-agents.md](docs/for-agents.md), azoL'u ilk denemede doğru yazması gereken bir model için kısa bir bilgilendirmedir: Rust, Go, Swift ve Python'dan farkları, deyimler ve hataların nasıl okunacağı.
- `azol check a.azl b.azl`, çalıştırılabilir dosya yazmadan bütün bir programı denetler.
- Tek çalıştırma her bağımsız hatayı bildirir: her fonksiyonun ilk hatası ya da her dosyanın ilk sözdizimi hatası. Yalnızca daha önceki bir hatanın sonucu olabilecek bir hata sonraki çalıştırmaya bırakılır; böylece bildirilen hiçbir şey gürültü değildir.
- Her hata türünün kararlı bir kodu vardır ve `azol explain E0408`, arkasındaki kuralı bir yanlış ve bir doğru örnekle açıklar. Bütün kodlar [docs/errors.md](docs/errors.md) belgesinde listelenmiştir.
- `--json` (`check` ve `build` için), standart çıktıya hata başına bir JSON nesnesi ve son satıra bir özet yazar: sorunlu metnin tam başlangıcı ve bitişi, kaynak satırı, çiğnenen kural ve onarım mekanikse onu yapan düzenleme.
- `azol guide` o kısa bilgilendirmeyi derleyicinin kendisinden yazdırır, `azol guide <topic>` ise başvurunun bir bölümünü; böylece doğru sürüm her zaman el altındadır.
- Başka bir dilin biçimi (`let mut`, `Vec<T>`, `&x`, `println!`, `len(xs)`, `elif`, `null`, bir `;`) adıyla yanıtlanır: azoL'daki biçimi ve çoğu zaman düzeltmesi birlikte verilir. `azol fix` bu düzeltmelerin hepsini uygular ve hiçbiri kalmayana kadar yeniden denetler: alışkanlıkla yazılmış ilk taslak, derlenmeye tek komut uzaklıktadır.
- `azol fmt` her dosyaya tek kanonik düzeni verir; böylece bir ajan boşluklarla hiç uğraşmaz ve farklar küçük kalır.

Şu program için

```
fn main() {
    let total = 0
    total = total + 1
    print(total)
}
```

`azol check --json total.azl` şunu yazar (burada satırlara bölünmüştür; satırlar ve sütunlar 1'den başlar, bitiş dışlayıcıdır):

```text
{"severity":"error","code":"E0401",
 "message":"cannot change `total`: declare it with `var` to make it changeable",
 "file":"total.azl","line":3,"column":5,"end_line":3,"end_column":10,
 "source":"    total = total + 1",
 "rule":"A variable declared with `let` never changes; only one declared with `var` can be assigned, grown or passed as `mut`.",
 "help":"Declare the variable with `var` instead of `let`.",
 "fix":{"file":"total.azl","line":2,"column":5,"end_line":2,"end_column":8,"replacement":"var"}}
{"summary":{"errors":1,"files":1}}
```

Bir `fix`, yalnızca tam olarak o düzenlemeyi uygulamak hatayı gideriyorsa verilir; testler her türü uygular ve sonucu denetler. Bugün bu şunları kapsar: `var` olması gereken `let`, bir argümanda eksik ya da yanlış yerdeki `mut`, eksik `match` kolları, değerleri yazılmamış bir varyant deseni ve değer taşımayan bir varyanttan sonra gelen `()`. Diğer hatalar kuralla birlikte gelir, düzenleme içermez.

## Araçlar

| Komut | Ne yapar |
|---|---|
| `azol build <files...> [-o out]` | Çalıştırılabilir dosyaya derler. Birkaç dosya tek program oluşturur |
| `azol build --target <target> ...` | Başka bir sistem için derler |
| `azol build -O ...` | Optimizer'dan geçirerek derler: daha hızlı program, daha yavaş derleme. Deneysel |
| `azol check <files...>` | Her bağımsız hatayı bildirir ve hiçbir şey yazmaz. Araçlar için `--json` |
| `azol explain [code]` | Bir hata kodunu açıklar ya da hepsini listeler |
| `azol fix <files...>` | Denetleyicinin önerdiği her düzeltmeyi uygular, yeniden denetler, yineler. `--check` yalnızca onları listeler, `--json` her birini bildirir |
| `azol guide [topic]` | azoL'a yeni başlayan biri için kısa bilgilendirmeyi ya da başvurunun bir bölümünü yazdırır. `--list` konuları adlandırır |
| `azol fmt <files...>` | Dosyaları kanonik düzende yeniden yazar. `--check` yalnızca onları listeler, `-` standart girdiyi biçimlendirir |
| `azol parse <file>`, `azol lex <file>` | Sözdizimi ağacını ya da token'ları yazdırır |

VS Code için sözdizimi renklendirmesi [`editors/vscode`](editors/) içindedir.

## Hedefler

| Hedef | Çıktı | Sınandığı yer |
|---|---|---|
| `aarch64-macos` | Apple Silicon için imzalı bir Mach-O çalıştırılabilir dosyası | Bir Mac'te ve denetleyen bir yığın belleği (heap) olan emüle edilmiş bir işlemcide |
| `x86_64-linux` | libc'siz, statik bir ELF çalıştırılabilir dosyası | Yerel olarak ve valgrind altında |
| `aarch64-linux` | libc'siz, statik bir ELF çalıştırılabilir dosyası | Şimdilik yalnızca emüle edilmiş bir işlemcide, hiç gerçek donanımda değil |

Varsayılan hedef, derleyicinin çalıştığı makinedir ve her hedef bunların herhangi birinden derlenebilir.

## Nasıl yapıldı

Beş aşama; her biri bir öncekinin çıktısı üzerinde yalın bir geçiştir:

1. **Lexer.** Baytlardan token'lara.
2. **Parser.** Token'lardan, düğüm başına bellek ayırmayan düz bir sözdizimi ağacına. Hiç geri dönmez.
3. **Denetleyici.** Adlar, türler, ownership ve dışlayıcılık kuralları; tek seferde tek fonksiyon.
4. **Kod üretici.** Sözdizimi ağacından tek geçişte doğrudan makine koduna: ARM64 ya da x86-64.
5. **Linker.** macOS için Mach-O dosyasını düzenler, sistem kütüphanesi içe aktarımlarını bağlar ve macOS'un gerektirdiği kod imzasını hesaplar. Linux için, sistemden çekirdek dışında hiçbir şey istemeyen statik bir ELF dosyası yazar.

Derleyicinin hiçbir bağımlılığı yoktur, SHA-256 dahil. Yazdırma, dosyalar, liste büyütme ve metin birleştirme için küçük bir çalışma zamanı, makine kodu olarak içine gömülüdür: macOS'ta elle yazılmış assembly; Linux'ta, C kütüphanesi olmadan yazılmış, kendi bellek ayırıcısı olan ve çekirdekle doğrudan konuşan C.

**Nasıl sınanıyor.** `sh test.sh` her test programını derleyip çalıştırır ve yazdırdıklarını karşılaştırır, birkaç bin hatalı programı her birinin alması gereken tam rapora göre denetler, üç hedefin hepsi için her çalıştırılabilir dosyanın baytlarını sabitler ve derleyiciye kendini derletip aynı testleri yeniden geçirtir. Bunun ötesinde, her test programı denetleyen bir yığın belleği olan emüle edilmiş bir ARM64 işlemcide çalıştırılır; bu bellek sızdırılan, iki kez serbest bırakılan, serbest bırakıldıktan sonra kullanılan ya da tek bir bayt taşırılan her arabelleği bildirir. x86-64 çıktısı Linux'ta yerel olarak ve denetleyen bir ayırıcıya bağlanarak valgrind altında çalıştırılır. Bir [fuzzer](tests/fuzz/) rastgele programlar yazar, her birinin ne yazdırması gerektiğini kendi yorumlayıcısıyla bulur ve bunu derlenmiş programın iki işlemcide yazdırdıklarıyla karşılaştırır.

## Self-hosting

Derleyici azoL ile yazılmıştır: [`compiler/`](compiler/) içinde lexer, parser, denetleyici, ARM64 ve x86-64 kod üreticileri, üç linker, tanılama iletileri, formatter ve komut satırı için yaklaşık 20.000 satır; buna ek olarak standart kütüphanenin azoL ile yazılmış bölümü [`lib/prelude.azl`](lib/prelude.azl). Kendini derler.

- **Nereden başlar.** Bir derleyici kendini sıfırdan derleyemez. [`bootstrap/`](bootstrap/), her hedef için hazır bir derleyici tutar; her biri yaklaşık 0,7 MB'tır ve bu aynı kaynaklardan derlenmiştir. [`bootstrap/README.md`](bootstrap/README.md) bunların özet değerlerini ve yeniden nasıl üretileceklerini verir.
- **Aynı derleyici olduğunun kanıtı.** Tohum derleyiciyle derlendiğinde kaynaklar bir derleyici üretir; o derleyici aynı kaynaklardan bir tane daha üretir; ikisi bayt bayt aynı dosyadır ve tohuma eşittir. `build.sh` bunu her derlemede denetler.
- **Bunun görüldüğü yerler.** x86-64 Linux'ta yerel olarak. macOS'ta bir Apple Silicon Mac'te, kaynakların daha eski bir durumuyla (o zaman 611.936 bayt). ARM64 Linux için yalnızca emüle edilmiş bir işlemcide, hiç gerçek donanımda değil.
- **Nereden geldi.** İlk derleyici Rust ile yazılmıştı. azoL ile yazılanın her parçası, ancak çıktısı Rust ile yazılanınkine bayt bayt eşit olduğunda kabul edildi; bu, deponun programları ve bilerek bozulmuş binlerce program üzerinde sınandı: token'lar, sözdizimi ağaçları, denetleyicinin tabloları, hata raporları ve üç hedefin hepsi için bütün çalıştırılabilir dosyalar. Bunu yapan betikler, [`compiler/check_*.py`](compiler/), artık değiştirilmiş bir derleyiciyi bir öncekiyle karşılaştırır.
- **Bugünkü maliyeti.** azoL ile yazılmış derleyici, aynı derleme için Rust ile yazılmış olanın yaklaşık iki katı komut çalıştırır (132.000 satırlık kıyaslama programında, x86-64 Linux'ta sayıldığında, x86-64 Linux için 2,0 kat, macOS için 1,9 kat). Yukarıdaki derleme süresi sayıları azoL ile yazılmış derleyiciyle ölçüldü.

## Yol haritası

Yapılmamış olanlar, planlanan sırayla. Biten işler burada listelenmez; yukarıdaki bölümlerde ve geçmiştedir.

**Hız: `azol build -O`, optimize edilmiş Rust kadar hızlı** ([tasarım](docs/design/optimizer.md), [bulgular](docs/design/optimizer-findings.md))

1. `azol build -O` vardır ve deneyseldir. Yapılacak olan: derleyicinin kendisinin onunla derlenmesi (yeni tohumlar), böylece varsayılan derleme hızını geri kazanır.
2. `rustc -O`'dan hâlâ uzak olan programlar: struct listeleri, kütüphane sıralaması, map'ler, metin oluşturma.
3. Kalan geçişler: döngü açma, kopya eleme, metin oluşturma, vektörleştirme.
4. Listelerde sıkıştırılmış struct'lar ve küçük enum'lar (`bool` listeleri sıkıştırılmıştır), bölünen metnin parçaları için view'lar, iç çağrı kuralı.
5. `-O` derleme hızı.
6. İddiayı tanımlayan ölçüt: [`bench/suite/`](bench/suite/) içindeki 28 programın `rustc -O` karşısında geometrik ortalaması 1.00 ya da daha iyi, hiçbiri 1.10'dan kötü değil, Apple M5'te ve x86-64'te. Bugün: Apple M5'te süre olarak 1.48 (varsayılan derleme: 2.12), 28 programın 9'u 1.10 sınırının içinde.

**Dil**

7. Floating point sayılar: `f64` ([tasarım](docs/design/floats.md)).
8. Metotlar ve trait'ler: kendi türleriniz üzerinde metotlar, her tür için `==`, sıralama ve özetleme (hash), sınırlı generic'ler, `Map[K, V]` ve `Set[T]` ([tasarım](docs/design/traits.md)).
9. Fonksiyon değerleri ve closure'lar ile bunları alan kütüphane fonksiyonları ([tasarım](docs/design/closures.md)).
10. Data race olmadan thread'ler ve kanallar ([tasarım](docs/design/threads.md)).
11. Paketler: modüller, `import`, merkezî sunucu olmadan bağımlılıklar, `test` blokları ([tasarım](docs/design/packages.md)).

**Yapay zekâ ajanları ve eski sistemlerin yerini almak için**

12. Ajanlar için daha fazlası: `azol example`, kendi `AGENTS.md` dosyasıyla `azol new`, bir fonksiyonun bütün hatalarının tek seferde bildirilmesi, JSON için bir şema ([tasarım](docs/design/agents.md)).
13. C çağırma, daha fazla tamsayı türü, standart girdi, süreçler, dizinler, tarihler, JSON, CSV, kaynak satırlarıyla yığın izleri ([tasarım](docs/design/legacy.md)).

**İlk sürümden önce**

14. Gerçek donanımda ARM64 Linux.
15. Yukarıdaki sayıların boş bir makinede, `-O` ile yeniden ölçülmesi.

## Ad

Rust adını pas mantarlarından alır. Azoller, onlara karşı kullanılan mantar ilaçlarıdır. Logodaki büyük L, bir pas katmanını kaldıran bir kazıyıcıdır; tek başına L ([`assets/mark.svg`](assets/mark.svg)) simgedir.

## Lisans

Tercihinize göre [MIT lisansı](LICENSE-MIT) ya da [Apache Lisansı, Sürüm 2.0](LICENSE-APACHE) ile lisanslanmıştır.
