# QA kontrol listesi

## Otomatik kontroller

Proje Rust testleri geçti (6/6):

- not oluşturma, güncelleme, silme ve taslakların yeniden açılışta kalması
- dışarıdan düzenlenen dosyada güncelleme/silme çakışmasının reddedilmesi
- görünür Markdown dosyalarının taranması; gizli dizin ve sembolik bağlantıların atlanması; dış değişikliklerin algılanması
- son değiştirilme sıralaması ve içerik araması
- uygulama kimliğiyle eşleşen `.desktop` dosyası oluşturma, `Exec` kaçışları ve mevcut dosyanın korunması

Rust test komutu:

```sh
cargo test --manifest-path src-tauri/Cargo.toml
```

`npm install` ve `npm run build` (TypeScript denetimi ve Vite üretim derlemesi), `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`, `cargo check --manifest-path src-tauri/Cargo.toml`, `cargo metadata --manifest-path src-tauri/Cargo.toml --no-deps --offline --format-version 1` ve `cargo test --manifest-path src-tauri/Cargo.toml` geçti (6/6). Kurulum betiği testleri de geçti (5/5):

```sh
python3 -m unittest discover -s scripts/tests -v
```

Linux release ikilisi `npm run build:desktop` ile oluşturulur. Kurulum ve kaldırma komutları README'deki Linux bölümündedir.

Kurulum betiği sahte `HOME` ve `XDG_DATA_HOME` altında da denendi. `install --skip-build` üretim ikilisini kopyaladı, `.desktop` girdisi `desktop-file-validate` kontrolünden geçti, yedi hicolor simgesi kuruldu ve `uninstall` kurduğu dosyaları kaldırdı. Denemede ayar dosyası ve not dosyası yerinde kaldı. Test yolu boşluk ve ters eğik çizgi içeriyordu. Gerçek kullanıcı kurulumunu bu QA denemesi değiştirmedi.

Ardından yetkili kullanıcı kurulumu başarıyla tamamlandı: `app.quicknote.local.desktop` doğrulandı, başlatıcı sabit kullanıcı ikilisine ve `app.quicknote.local` simgesine bağlandı, üretim ikilisi probu `production` verdi. Mevcut portal başlatıcısı yedeklendi/migrasyondan geçti. Uygulama bu adımdan sonra başlatılmadı; notlar, uygulama ayarları ve kısayollar değiştirilmedi.

İlk kurulum başlatıcısında `TryExec` değerinin etrafında tırnak vardı; masaüstü bunu gerçek ikili yolu olarak çözmediği için uygulama menüsünde göstermeyebilirdi. Kurucu düzeltildi ve kullanıcı kurulumu yeniden çalıştırıldı. Güncel girdide `TryExec` tırnaksız mutlak yol olarak mevcut ve çalıştırılabilir ikiliye gidiyor; `desktop-file-validate` ve `kbuildsycoca6 --noincremental` başarılı oldu. KDE menüsünde görsel olarak belirdiği yine de doğrulanmadı; uygulama başlatılmadı.

Bu çalışma alanındaki önceki ham `cargo build --release` ikilisi `custom-protocol` olmadan derlenmişti; Tauri bu durumda `devUrl` (`http://127.0.0.1:1420`) adresini seçer ve Vite sunucusu kapalıyken UI 127.0.0.1 bağlantı hatası verir. Desteklenen üretim komutu `npm run build:desktop` (`tauri build --no-bundle`) olup `dist` dosyalarını üretim protokolüyle release ikilisine gömer. Ham release ikilisi artık üretim protokolü etkin değilse açılışı reddedip doğru komutu bildirir. Güncellenmiş ikiliyi açmadan önce gizlenmiş eski örneği tepsi menüsündeki **Çıkış** ile tamamen kapat.

Güncel `npm run build:desktop` çıktısı Vite sunucusu kapalıyken ve geçici `XDG_CONFIG_HOME` ile başlatıldı. Tanı sırasıyla `QUICKNOTE_WEBVIEW_FINISHED tauri://localhost` ve `QUICKNOTE_FRONTEND_READY` verdi. Bu, `127.0.0.1:1420` yerine gömülü frontend URL'sinin yüklendiğini ve başlangıç IPC/ayar/dinleyici kurulumunun tamamlandığını doğrular. Pencerenin görünür olduğunu veya ekran kontrollerinin çalıştığını kanıtlamaz. Tanı için `QUICKNOTE_TRACE_WEBVIEW=1` ayarı kullanılır. 12 saniyelik deneme zaman aşımıyla sonlandırıldı; kullanıcı ayarları ve kısayollar değiştirilmedi.

Güncellenmiş doğrudan release ikilisiyle Wayland host portalına `Registry.Register(app.quicknote.local)` başarılı oldu. İkili, eşleşen `app.quicknote.local.desktop` girdisini `$XDG_DATA_HOME/applications` altında oluşturdu; bu dosya `desktop-file-validate` ile doğrulandı. Ardından `CreateSession` yanıtı `status 0` ve bir `session_handle` döndürdü. Testte geçici uygulama yapılandırma dizini kullanıldı ve kullanıcı kısayolları değiştirilmedi. Hiçbir kısayol atanmadığı için `BindShortcuts`/tuş aktivasyonu denenmedi. Yükseltilmiş 15 saniyelik denemede süreç zaman aşımıyla durdurulana dek çalıştı; sadece Ayatana kullanımdan kaldırma uyarısı kaydedildi. Bu, pencerenin ekranda açıldığını veya etkileşimlerin çalıştığını doğrulamaz. Normal sandbox denemesi GTK başlatma sırasında `Operation not permitted` hatasıyla durdu. Not klasörü seçilmemişti; denemede not dosyası oluşturulmadı. Ekran görüntüsü ya da Windows/macOS/X11 doğrulaması yok.

## Elle kontrol

- [ ] Temiz ilk açılışta uygulama görünür ve not klasörü seçme ekranı açılır.
- [ ] Klasör seçildikten sonra yeni hızlı not yazılır; Enter kaydeder, Shift+Enter yeni satır ekler ve klasörde bir `.md` dosyası oluşur.
- [ ] Uygulama yeniden başlatıldığında seçili klasör, ayarlar ve hızlı not taslağı korunur.
- [ ] Arşiv araması başlık ve içerik önizlemesiyle eşleşir; liste son değiştirilme zamanına göre sıralanır.
- [ ] Kaydedilmemiş düzenleme uygulama içinde başka nota geçerken taslak olarak korunur; kaydetme sürüm çakışmasını doğru bildirir.
- [ ] Not, uygulama dışında değiştirilip kaydedilmeye çalışıldığında çakışma uyarısı görünür; uygulama dışındaki içerik sessizce ezilmez.
- [ ] İki ayrı genel kısayol atanır; aynı kombinasyonun ikinci eyleme atanması reddedilir; temiz kurulumda varsayılan kısayol yoktur.
- [ ] Tepsi menüsündeki Hızlı not, Tüm notlar, Not arşivi, Ayarlar ve Çıkış eylemleri beklenen ekranı açar veya uygulamadan çıkar.
- [ ] Wayland'de portal kısayol izni verilir ve kısayollar tetiklenir; portal yoksa arayüz kullanılabilir uyarı gösterir ve tepsi yolu çalışır.
- [x] Vite sunucusu kapalıyken Tauri CLI release ikilisi `tauri://localhost` adresini yükledi ve `QUICKNOTE_FRONTEND_READY` verdi; ekranların görsel/etkileşim kontrolü bekliyor.
- [x] `QUICKNOTE_TRACE_WEBVIEW=1` tanısı `QUICKNOTE_WEBVIEW_FINISHED tauri://localhost` ve `QUICKNOTE_FRONTEND_READY` satırlarını verdi.
- [ ] `python3 scripts/install_linux.py install` ile geçerli kullanıcı için kur; uygulama menüsünden veya `~/.local/bin/quicknote` yolundan açıldığını doğrula.
- [ ] `python3 scripts/install_linux.py uninstall` ile kaldır; seçilen not klasörü ve uygulama ayarlarının korunduğunu doğrula.
- [ ] Linux X11, macOS ve Windows'ta kısayol, pencereyi gizleyip geri açma ve paket kurulumu ayrı ayrı denenir.
