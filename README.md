# Quicknote

Quicknote, notları seçtiğin klasörde ayrı Markdown (`.md`) dosyaları olarak saklayan küçük bir masaüstü uygulamasıdır. Tauri 2, Rust ve TypeScript kullanır. Not verileri için SQLite kullanmaz.

## Geliştirme ortamı

Gerekli araçlar: Node.js ve npm, Rust/Cargo ve Tauri'nin işletim sistemine özgü masaüstü bağımlılıkları. Linux'ta Tauri WebKitGTK 4.1 ve GTK geliştirme paketleri gerekir; tepsi simgesi için AppIndicator desteği de kullanılır.

Bağımlılıkları kurup geliştirme uygulamasını başlat:

```sh
npm install
npm run tauri -- dev
```

Frontend tür denetimi ve üretim çıktısı:

```sh
npm run build
```

Masaüstü paketini derle:

```sh
npm run build:desktop
```

Bu komut Tauri üretim protokolüyle `dist` dosyalarını release ikilisine gömer ve kurulum paketi oluşturmaz. Düz `cargo build --release`, `custom-protocol` özelliği etkin değilse `devUrl` (`http://127.0.0.1:1420`) adresini seçer; Vite sunucusu kapalıyken UI bağlantı hatası verir. Bu projede üretim ikilisini Tauri CLI ile oluştur. Ham release Cargo ikilisi üretim protokolü etkin değilse artık açılışı reddedip doğru komutu bildirir.

Rust depolama testleri:

```sh
cargo test --manifest-path src-tauri/Cargo.toml
```

Rust kaynaklarını masaüstü çalışma zamanı oluşturmadan kontrol et:

```sh
cargo check --manifest-path src-tauri/Cargo.toml
```

`npm run build`, `cargo check --manifest-path src-tauri/Cargo.toml` ve `cargo test --manifest-path src-tauri/Cargo.toml` bu çalışma sırasında geçti. `npm run build:desktop` Linux için release çalıştırılabilir dosyasını `src-tauri/target/release/quicknote` konumunda oluşturur. Bu komut kurulum paketi üretmez. Güncellenmiş ikiliyi denemeden önce eski örneği tepsi menüsünden **Çıkış** ile tamamen kapat; pencereyi kapatmak uygulamayı gizler ve süreç çalışmaya devam eder.

Vite kapalıyken `npm run build:desktop` ile üretilen ikili açıldı; WebView tanısı `QUICKNOTE_WEBVIEW_FINISHED tauri://localhost` ve ardından `QUICKNOTE_FRONTEND_READY` kaydetti. Bu, gömülü frontend'in yüklendiğini ve başlangıç IPC/ayar/dinleyici kurulumunun tamamlandığını doğrular. Görsel pencereyi, kısayol tuşlarını veya her ekranın etkileşimini doğrulamaz. Ayrıntılar ve tamamlanmamış elle kontrol listesi [QA.md](QA.md) dosyasındadır.

## Linux'a kurulum ve kaldırma

Kurulum betiği yalnızca geçerli Linux kullanıcısı için çalışır ve `sudo` gerektirmez. Önce ne yapacağını görmek için:

```sh
python3 scripts/install_linux.py install --dry-run
```

Varsayılan kurulum Tauri release uygulamasını `npm run build:desktop` ile derler ve kullanıcı uygulama menüsüne Quicknote'u ekler:

```sh
python3 scripts/install_linux.py install
```

Betik üretim ikilisini `~/.local/lib/quicknote/quicknote` altına, komut sarmalayıcısını `~/.local/bin/quicknote` altına, `app.quicknote.local.desktop` başlatıcısını ve hicolor simgelerini `${XDG_DATA_HOME:-~/.local/share}` altına kurar. Derleme daha önce yapıldıysa `--skip-build` kullanılabilir. Başka bir üretim ikilisi vermek için `--binary /path/to/quicknote` kullan; geliştirme modunda çalışan veya ham `cargo build --release` ile oluşturulan ikili reddedilir. Önceki kurucu sürümü başlatıcıdaki `TryExec` yolunu tırnaklı yazmış olabilir; Quicknote uygulama menüsünde görünmüyorsa güncel kurucuyla `python3 scripts/install_linux.py install --skip-build` komutunu yeniden çalıştır.

Kaldırmak için aynı kullanıcı ve aynı `XDG_DATA_HOME` değeriyle:

```sh
python3 scripts/install_linux.py uninstall
```

Kaldırıcı yalnızca kendi kurduğu, sonradan değiştirilmemiş başlatıcı, simge ve ikili dosyalarını siler; notları, ayarları ve kısayol tercihlerini korur. Aynı uygulama kimliğine ait mevcut portal başlatıcısı tanınır ve kaldırmada geri yüklenir. Betik başka bir `.desktop` girdisini varsayılan olarak değiştirmez. Böyle bir girdiyi inceledikten sonra kurulumda `--force` kullanırsan, betik onu yedekler ve kaldırmada geri koyar.

## Kullanım

İlk açılışta **Ayarlar** bölümünden not klasörünü seç. Uygulama yeni notları o klasöre yazar. Ayarlarda iki genel kısayolu ayrı ayrı atayabilirsin: boş hızlı not açma ve tüm notları son değişiklik sırasıyla gösterme. Kısayollar varsayılan olarak atanmaz; çakışan bir kısayol kaydedilemez. Tüm notlar görünümünde ok tuşları veya `J`/`K` ile seçimi taşı, `Enter` ile aç, `X` ile seçili notu onay sorusundan sonra sil. Silme, varsa kaydedilmiş taslağı da siler.

Notu arşivden açıp düzenleyebilir, başlık ve içerikte arayabilir, son değişiklik zamanına göre sıralayabilir ve dışarıda değişen dosyalar için çakışma uyarısı alabilirsin. Pencereyi kapatmak uygulamadan çıkmak yerine gizler; uygulamayı tepsi menüsünden tekrar açabilir veya **Çıkış** ile kapatabilirsin.

## Dosyalar ve saklama

- Her kaydedilmiş not seçilen klasörde UTF-8 Markdown dosyasıdır. Yeni dosya adları UTC zaman damgası ve çakışmayı önleyen sıra numarası içerir.
- Arşiv, seçilen klasördeki `.md` dosyalarını alt klasörler dahil tarar. Nokta ile başlayan öğeleri ve sembolik bağlantıları atlar.
- Liste son değiştirilme zamanına göre azalan sıradadır. Arama başlıkta ve kısa içerik önizlemesinde yapılır.
- Ayarlar, hızlı not taslağı ve kaydedilmemiş not düzenleme taslakları Tauri uygulama yapılandırma dizinindeki `notes-state.json` dosyasında tutulur. Bu taslaklar seçilen klasördeki Markdown dosyalarından ayrıdır.
- Dışarıdan yapılan not değişiklikleri düzenli aralıklarla taranır. Dosya içeriği beklenen sürümden farklıysa uygulama güncellemeyi veya silmeyi reddeder; önce notu yeniden aç.

## Platform notları

Linux Wayland oturumlarında genel kısayollar masaüstünün Global Shortcuts portalına bağlıdır. Portal uygulamayı tanıyabilsin diye ilk açılışta Tauri uygulama kimliğiyle eşleşen bir `.desktop` girdisi kullanılır; doğrudan çalıştırılan uygulama gerekirse bunu `$XDG_DATA_HOME/applications` (değişken ayarlı değilse `~/.local/share/applications`) altında oluşturur ve mevcut girdinin üstüne yazmaz. Portal desteklenmiyorsa veya bir kısayol kabul edilmezse uygulama tepsi menüsünden kullanılabilir. Linux tepsi simgesinin görünmesi masaüstü ortamının AppIndicator/tepsi desteğine bağlıdır. X11, macOS ve Windows kısayol, tepsi ve paket davranışları bu çalışma sırasında elle doğrulanmadı.

Güncellenmiş sürümü çalıştırmadan önce eski Quicknote sürecini tepsi menüsündeki **Çıkış** ile tamamen kapat. Pencereyi kapatmak uygulamayı gizler; aynı uygulama kimliğiyle yapılan sonraki başlatma eski çalışan örneğe yönlendirilir.

## QA

Çalıştırılabilir komutlar ve manuel masaüstü kontrol listesi için [QA.md](QA.md) dosyasına bak.
