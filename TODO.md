# TODO — cose da fare su questo fork di Servo

Backlog di lavoro noto ma non ancora fatto sulla copia vendorizzata/patchata di Servo in
questa cartella. Vedi [`CUSTOMIZATIONS.md`](./CUSTOMIZATIONS.md) per le modifiche già
applicate e [`CLAUDE.md`](./CLAUDE.md) per il protocollo da seguire quando si chiude uno di
questi punti (aggiornare `CUSTOMIZATIONS.md` + rigenerare la patch nella stessa sessione).

---

## 1. Collegare `embedded.yml` al build patchato invece del binario Servo stock

**Stato:** noto, in sviluppo — non ancora prioritario.

`../.github/workflows/embedded.yml` scarica oggi il binario `servoshell` ufficiale
precompilato da `servo/servo` (vedi `SERVO_TAG`), non il fork patchato in questa cartella.
Finché resta così, ogni release "embedded" mostra ancora la UI browser stock (toolbar/tab
strip) che le patch in `CUSTOMIZATIONS.md` rimuovono solo nel codice, non nel binario
distribuito.

Quando si affronta questo punto, `servo/.github/workflows/test.yml` è già il punto di
partenza corretto: builda da sorgente (tag pristine + `patches/`) — oggi è dormiente perché
GitHub Actions scopre i workflow solo alla radice di un repo reale (vedi commento in testa a
quel file). Le opzioni sono, in ordine di probabile complessità:
- pubblicare `servo/` come repo standalone (es. `BlackRam-oss/servo`, già menzionato in
  `CLAUDE.md`) e far scaricare a `embedded.yml` gli artifact di build di quel repo;
- oppure integrare uno step di build-from-source direttamente dentro `embedded.yml` stesso.

## 2. Verificare che la GPU venga usata correttamente (no fallback software)

Da verificare che questa build di Servo usi effettivamente l'accelerazione hardware per il
rendering (WebGL/WebGPU) e non finisca su un fallback software (es. llvmpipe/SwiftShader),
che per un gioco significherebbe prestazioni inaccettabili. `../test-page/` (vedi
`CUSTOMIZATIONS.md`, voce sul bridge `steam:`) ha due pulsanti — "Test PixiJS render" e
"Test Three.js render" — pensati apposta per questa verifica, ed entrambi ora riportano anche
gli fps a schermo, non solo "ok/failed". Da fine 2026-08-07 la pagina include anche
`GpuInfoPanel`, che legge `WEBGL_debug_renderer_info` e stampa il renderer/vendor GPU
*effettivo* (mascherato e non mascherato) e un'euristica "software renderer" — questo è
esattamente il "quale renderer/GPU viene effettivamente riportato" che mancava. Restano da
fare: controllare i log di Servo/ANGLE al lancio, e soprattutto **verificarlo su una build
reale** su ciascuna piattaforma della matrice CI (Windows/macOS/Linux) — non ancora fatto.

## 3. Android: leggere tutto `manifest.webmanifest` (non solo `orientation`), override via parametro, e riflettere tutto in `roves-action`/Roves Packmaster (`roves-ui`)

**Stato: fatto (2026-09-01/02, branch `android` su tutti e tre i repo).** Copertura completa
del manifest (`name`/`short_name`/`orientation`, non più solo `orientation`) + override
espliciti lato engine (vedi `CUSTOMIZATIONS.md`, voce "`mach bundle --android`: full manifest
coverage..."); `roves-action` espone `android-app-name`/`android-orientation`/
`android-theme-color`; Roves Packmaster (`roves-ui`) ha sia la UI (card Mobile, switch
webmanifest, campi disabilitati che mostrano i valori reali del manifest) sia un vero backend
(`src-tauri/src/android.rs`) che genera davvero un `.apk` — non più solo placeholder.

**Aggiornamento 2026-09-10:** il flusso base (`mach bundle --android`, build debug, host
Linux) è ora **verificato end-to-end via CI reale**, non più solo per lettura del codice —
vedi `roves-action`'s nuovo job `build-android` e i tre bug reali trovati e corretti proprio
grazie a quella verifica (voci "Fix `mach bundle --android` crashing unconditionally" e le due
voci "Fix `_bundle_android`'s ... output-`.apk` lookup" in `CUSTOMIZATIONS.md`) — prima d'ora
questo comando non aveva **mai** funzionato con successo, nonostante fosse presente da giorni.
Restano non verificati: `--android-release` (firma reale, punto 5), e l'intero percorso su host
Windows (punto 6).
`theme_color`/status bar **non** implementato: rimosso dalla UI su richiesta esplicita ("la
status bar non deve esserci"), quindi non è più nel design finale, non solo rimandato.

Da fare, in ordine indicativo di dipendenza:

- **Copertura completa del web app manifest**, non solo `orientation`: `name`/`short_name`
  (etichetta app), `icons` (icona app — vedi punto icona sotto), `theme_color`/
  `background_color` (colore status bar/splash), `display`, `lang`, ecc. — ogni campo dello
  standard [Web App Manifest](https://developer.mozilla.org/en-US/docs/Web/Manifest) che ha un
  equivalente Android sensato. Considerare anche che `manifest.webmanifest` non è l'unico nome
  file in uso in pratica (`manifest.json` è già gestito da `_resolve_window_title`/
  `_resolve_android_orientation`; verificare se altre convenzioni — es. `site.webmanifest` di
  alcuni tool — vanno aggiunte all'elenco dei candidati).
- **Default-da-manifest con override esplicito**: se `manifest.webmanifest` (o equivalente)
  esiste nel `--content-dir`, i valori vengono presi automaticamente da lì; ognuno deve poter
  essere sovrascritto passando il parametro corrispondente a `mach bundle` esplicitamente
  (stesso pattern già in uso per `--icon-png`/`--icon-ico` su desktop: un flag esplicito vince
  sempre sull'auto-detect).
- **Icona**: non reinventare un percorso Android-specifico — riusare la stessa logica di
  auto-detect già esistente per desktop (`icon.png`/`icon.ico`/fallback `favicon.ico` da
  `--content-dir`, patch `0051`/`0052` in `CUSTOMIZATIONS.md`) invece di leggere `icons[]` dal
  manifest in modo indipendente, cioè "quella che viene già usata per Windows ecc." — così un
  solo meccanismo di risoluzione icona serve tutte le piattaforme.
- **`roves-action`** (`DRincs-Productions/roves-action`): una volta che `mach bundle --android`
  supporta questi parametri, `action.yml` deve esporli come input (mirroring — vedi
  `CLAUDE.md`, sezione "keep `roves-action` in sync"). Non toccato in questo giro perché il
  lavoro Android è stato scoperto esplicitamente alla sola cartella dell'engine.
- **Roves Packmaster** (cartella sibling `roves-ui`, package.json name `roves-packmaster` —
  stesso progetto descritto in `CLAUDE.md`, solo nome di cartella diverso): aggiungere una
  nuova sezione "Mobile" (per ora solo Android), parallela all'esistente sezione Desktop/
  `PortableSettings` in `src/lib/settings.ts` — una card abilitabile/disabilitabile come quella
  desktop. Dentro la card, un accordion con le impostazioni avanzate mobile. Se
  `manifest.webmanifest` è presente nel content dir del progetto, mostrare uno switch "prendi
  le info da webmanifest": **on di default quando il manifest esiste**; quando è on, tutti i
  campi avanzati mobile vanno disabilitati (grigi/non editabili), dato che i valori arrivano
  dal manifest; quando è off, i campi tornano editabili manualmente (equivalente UI
  dell'override via parametro di `mach bundle` sopra).

## 4. Bundling Android su Windows: `ndk-build` invocato senza fallback `.cmd`

**Superato (2026-09-13):** Android non compila più Rust/NDK affatto — vedi
`CUSTOMIZATIONS.md`, voce "Mobile pivots from Servo to native WebView". `servoview/`
(il modulo che invocava `ndk-build`) è stato eliminato del tutto. Questo intero punto,
incluso il punto 6 sotto, non si applica più: non c'è nessun `ndk-build`/NDK da invocare,
su nessuna piattaforma. Lasciato qui solo come riferimento storico.

**Stato (storico, pre-pivot): fatto (2026-09-10), non verificato su un build Windows reale.** Vedi
`CUSTOMIZATIONS.md`, voce "Fix `ndk-build` invocation for Windows", e
`patches/servo-v0.5.0/0004-android.patch` (rigenerata). Scoperto il 2026-09-02 lavorando al
backend Android di Roves Packmaster (`roves-ui/src-tauri/src/android.rs`).

`support/android/apk/servoview/build.gradle.kts` (upstream Servo, non una customizzazione di
questo fork) invoca l'NDK con `getNdkDir() + "/ndk-build"` — letteralmente senza estensione,
mai `ndk-build.cmd` — per il task che copia `libservoshell.so`/`libc++_shared.so` nella
cartella `jniLibs/` dell'APK (vedi il commento in cima a `jni/Android.mk`/quel file Gradle).
Su Windows l'NDK fornisce solo `ndk-build.cmd` (uno script batch), non un file chiamato
`ndk-build` senza estensione — e Java/Gradle (`ProcessBuilder`/`Exec` task) non fa la
risoluzione delle estensioni via `PATHEXT` come farebbe `cmd.exe`. Quindi, **così com'è
scritto oggi, questo step Gradle probabilmente fallisce su Windows a prescindere da chi lo
invoca** (`mach build/bundle --android` da Windows — già comunque bloccato più a monte da
`python/servo/platform/build_target.py`'s restrizione Linux/macOS-only sulla cross-compilazione
Rust — o Roves Packmaster, che invece *potrebbe* girare su Windows dato che non compila Rust,
ma è stato scoperto proprio per questo e quindi bloccato esplicitamente lì,
`check_android_availability()` in `android.rs`).

**Non verificato di persona** (nessun ambiente Windows con NDK reale disponibile in questa
sessione per confermarlo empiricamente) — dedotto leggendo il codice Kotlin e il comportamento
noto di `ProcessBuilder` su Windows, non testato.

**Fix applicato:** `getNdkDir() + "/ndk-build"` → sceglie `.cmd` su Windows via
`org.gradle.internal.os.OperatingSystem.current().isWindows`. Verificato solo che la patch si
applichi pulita a un'estrazione pristine di v0.5.0 — non testato contro un vero
Gradle/NDK/`ndk-build.cmd` su Windows (nessun toolchain Android disponibile in questa
sessione). `check_android_availability()` in `roves-ui/src-tauri/src/android.rs` va comunque
aggiornato per smettere di bloccare Windows, e il tutto va riverificato su un runner Windows
reale prima di considerare questo punto davvero chiuso — vedi il punto 6 sotto.

## 5. Firma dell'APK Android (release signing)

**Stato: lato motore fatto (2026-09-10). Il percorso debug (senza `--android-release`) è
verificato end-to-end via CI reale (vedi punto 3 sopra); `--android-release` in sé resta non
verificato** (nessun ambiente CI con un keystore reale lo esercita ancora). Lato Roves
Packmaster: fatto (2026-09-10, `src-tauri/src/signing.rs`), anch'esso non verificato con una
build reale — vedi `roves-ui/TODO.md` #2. `roves-action` ancora da fare (vedi sotto). Vedi
`CUSTOMIZATIONS.md`, voce "`mach bundle --android-release`", per il dettaglio completo.

`mach bundle --android --android-release` ora sceglie la variante Gradle `Release` invece di
`Debug`, riusando il meccanismo di firma **già esistente upstream**
(`support/android/apk/buildSrc/src/main/kotlin/Android.kt`'s `getSigningKeyInfo`, non
patchato, legge 4 variabili d'ambiente: `APK_SIGNING_KEY_STORE_PATH`/`_STORE_PASS`/`_ALIAS`/
`_PASS`) — non serviva inventare nulla lato Gradle, solo collegare `mach bundle` e rifiutarsi
di procedere se `APK_SIGNING_KEY_STORE_PATH` non è impostata (altrimenti Gradle firmerebbe
comunque con la chiave di debug, silenziosamente). **Non verificato con una build reale**
(nessun toolchain Android né un keystore vero disponibili in questa sessione per confermare
che l'apk risultante sia genuinamente firmato, es. via `apksigner verify`).

**Resta da fare:**

- **`roves-action`**: nuovi input `android-keystore-*` (verosimilmente un keystore
  base64-encoded via GitHub Secret, decodificato in un file temporaneo, con le 4 variabili
  d'ambiente impostate prima di invocare `mach bundle`). Non toccato in questo giro.

## 6. Bundling/generazione dell'APK Android anche su Windows

**Superato (2026-09-13):** vedi la nota nel punto 4 sopra — niente più NDK/Rust da
cross-compilare, su nessuna piattaforma, quindi "farlo funzionare su Windows" non è più
un problema NDK ma solo Java+Android SDK+Gradle, già cross-platform di per sé.
`roves-ui/src-tauri/src/android.rs` va comunque riscritto per il nuovo bundling
Gradle-only — vedi `roves-ui/TODO.md`.

**Stato (storico, pre-pivot): sbloccato dal punto 4, ma non ancora chiuso.** Il blocco Gradle (`ndk-build` senza
fallback `.cmd`) è risolto (punto 4), ma restano da fare, in ordine:

1. Rimuovere il blocco esplicito in `check_android_availability()`
   (`roves-ui/src-tauri/src/android.rs`) che oggi disabilita Android su Windows in Packmaster
   — non ha più motivo di esistere una volta verificato il punto 4.
2. **Verificare per davvero su un runner/macchina Windows con Android SDK/NDK reale** — il
   fix del punto 4 non è mai stato eseguito contro un `ndk-build.cmd` vero, solo verificato
   sintatticamente. Finché non succede, "sbloccato" è una previsione, non una conferma.
3. Per il solo percorso `mach build/bundle` da sorgente (non Packmaster, che non compila Rust)
   resta comunque la restrizione Linux/macOS-only sulla cross-compilazione Rust in
   `python/servo/platform/build_target.py`, indipendente dal problema Gradle — todo separato,
   non toccato da questo punto.

## 7. Portare il protocollo `game://` anche su Android

**Superato (2026-09-13):** Android non usa più Servo/`game://`/`file://` affatto — vedi
`CUSTOMIZATIONS.md`, voce "Mobile pivots from Servo to native WebView". Il problema che
questo punto risolveva (router lato client, path assoluti rotti) è ora risolto in modo
diverso: `WebViewAssetLoader` serve il gioco su una vera origine
`https://appassets.androidplatform.net/`, non `file://`, quindi non c'è più bisogno del
protocollo `game://` su Android. Lasciato qui come riferimento storico — il lavoro sotto
resta valido per desktop/OpenHarmony, che continuano a usare Servo.

**Stato (storico, pre-pivot): fatto (2026-09-12) — vedi `CUSTOMIZATIONS.md`, voci "Port `file:`'s content-root
rebasing to Android" e "Port the full `game://content/` protocol to Android", per il dettaglio
completo. In attesa di conferma finale su dispositivo reale + CI (`android.yml`/`test.yml`)
prima di considerarlo definitivamente chiuso.**

Il problema dei percorsi assoluti (`src="/assets/..."`) rotti sotto `file://` è stato risolto
**a livello di motore, non più con un patch Kotlin a regex**: `ports/servoshell/desktop/
protocols/file.rs`'s `rebase_to_content_root` (già usato con successo su desktop) è stato
spostato in un nuovo modulo condiviso (`ports/servoshell/protocols/`) e collegato anche a
`egl/app.rs` (Android/OpenHarmony). Il fix a regex nell'HTML estratto (`MainActivity.kt`) resta
presente ma è ridondante/superato, non rimosso.

**Correzione rispetto alla nota precedente:** qui sotto era scritto che il problema del router
lato client "non si è manifestato" per `pixi-vn-react-template` (TanStack Router). Era
**sbagliato** — era solo mascherato dall'errore di caricamento asset più vistoso, visibile
nello screenshot precedente. Una volta risolto quello (v0.4.16), lo screenshot successivo ha
mostrato esattamente il sintomo previsto: schermo nero con testo "Not Found", la pagina di
fallback del router. Il fix completo — portare anche `game.rs`/`GameProtocolHandler` (non solo
il fallback di `file.rs`) sullo stesso modulo condiviso, e far avviare `egl/app.rs` su
`game://content/` invece del `file://` letterale quando c'è un lancio bundled — è stato
implementato nella stessa giornata.

## 8. Rifinire il pivot mobile a WebView nativo (Android/iOS)

**Stato: motore fatto (2026-09-13) — vedi `CUSTOMIZATIONS.md`, voce "Mobile pivots from
Servo to native WebView". Restano aperti, in ordine di priorità:**

1. **Migrare `roves-action` e `roves-ui`/Packmaster al nuovo bundling Gradle-only** — vedi
   i `TODO.md` di quei due repo. `roves-action` scaricava `roves_android_native_arm64.zip`/
   `roves_android_project.zip` dalla release "test" del motore (che `android.yml` non
   pubblica più); Packmaster's `android.rs` faceva il bootstrap NDK/Rust/JRE che non serve
   più affatto — va riscritto per scaricare solo Java+Android SDK e lanciare Gradle.
2. **Wire iOS staging into `mach bundle`** — `support/ios/bundle.py` è oggi uno script
   standalone, non collegato a `post_build_commands.py`/`mach bundle --ios`. Servirebbe un
   `is_ios(self.target)` branch analogo a `is_android`, e capire come/se firmare/costruire
   automaticamente (richiede macOS+Xcode, non disponibile in questa sessione).
3. **Verifica su dispositivo reale, entrambe le piattaforme** — nessun emulatore/dispositivo
   Android né macOS/Xcode disponibili in questa sessione. Da verificare: timing dello splash,
   persistenza storage, seeking video/audio, rotazione, fullscreen, comportamento del router
   lato client con un gioco reale (`pixi-vn-react-template`).
4. **APK signing** (punto 5 sopra) resta valido e non affetto dal pivot — `--android-release`
   funziona identicamente nel nuovo `_bundle_android`.

## Note

- Punto risolto nella sessione del 2026-08-06: stato di navigazione browser morto
  (location/back-forward/load-status/favicon) rimosso da `gui.rs` — vedi
  `CUSTOMIZATIONS.md`, patch `0003-strip-dead-browser-navigation-state-and-favicon-pipeline`.
  Lasciato intenzionalmente intatto `browser_tab`/`toolbar_button`: sono dead code senza
  alcun chiamante, già eliminati dal compilatore nelle build di release, quindi rimuoverli
  non avrebbe alcun effetto sul pacchetto di gioco finale.
- Punto risolto nella sessione del 2026-08-06/07: schermata bianca su contenuto `file://` —
  vedi `CUSTOMIZATIONS.md`, patch `0007-stable-file-origin-for-module-script-loading`.
  **Verificato end-to-end il 2026-08-07** su `../test-page/` (bundle Vite multi-chunk, script
  esterno carica ed esegue correttamente su una build reale). Resta un limite noto, non un
  problema di questa patch: il fix vive solo nelle patch di questo fork, quindi finché il
  punto 1 non viene risolto (far costruire/consegnare a `embedded.yml` il binario patchato),
  la build "embedded" realmente distribuita continua ad avere questo stesso bug — da
  riverificare con il `dist/` reale del gioco (non solo `../test-page/`) una volta risolto
  anche il punto 1.
- Punto risolto nella sessione del 2026-08-07: `localStorage`/`sessionStorage`/`indexedDB`/
  `navigator.storage` bloccati dall'origine opaca di `file://` — vedi `CUSTOMIZATIONS.md`,
  patch `0008-allow-storage-for-file-origin`. Insieme a questo, tutti i 18 pref del bundle
  `EXPERIMENTAL_PREFS` di upstream (clipboard, IndexedDB, WebGL2, WebGPU, OffscreenCanvas,
  Notifications, CSS Grid/Container Queries, ecc.) sono ora accesi di default — vedi patch
  `0009-default-on-experimental-web-platform-prefs`. Non ancora verificato end-to-end su una
  build reale (solo `cargo check` sui crate coinvolti, e `servo-script` non è stato
  verificabile in questa sandbox — vedi caveat nella voce 0008 di `CUSTOMIZATIONS.md`) —
  stesso caveat delle altre voci di questo file in attesa del punto 1 (build patchata reale).
- Punto risolto nella sessione del 2026-08-07: menu contestuale (tasto destro) disabilitato
  del tutto, no-op, nessun menu alternativo — vedi `CUSTOMIZATIONS.md`, patch
  `0010-disable-context-menu-popup`. `Dialog::ContextMenu` (rendering, costruttore, variante)
  rimosso interamente da `dialog.rs` in quanto diventato dead code.
- Punto risolto nella sessione del 2026-08-07: scorciatoie di reload (`Ctrl+R`/`F5`) rimosse
  da `headed_window.rs` — vedi `CUSTOMIZATIONS.md`, patch
  `0011-remove-page-reload-shortcuts`. Lasciata intenzionalmente intatta l'API di embedding
  nativo (`egl::App::reload()`, Android/OpenHarmony), fuori scope.
- Punto risolto nella sessione del 2026-08-07: navigazione "indietro"/"avanti" rimossa da
  tutti gli input path lato giocatore — scorciatoie da tastiera e pulsanti laterali del mouse
  in `headed_window.rs` — vedi `CUSTOMIZATIONS.md`, patch
  `0012-remove-back-forward-navigation`. Le voci equivalenti nel menu contestuale sono coperte
  dalla rimozione del menu stesso, sopra. Stessa nota di scope: API di embedding nativo
  (`egl::App::go_back`/`go_forward`) lasciata intatta.
  Nessuna di queste tre voci è stata verificata contro una build reale in questa sessione:
  `cargo check -p servoshell` in questa sandbox non completa per una lacuna di toolchain
  preesistente e indipendente da queste modifiche (manca `libclang` per la build script di
  `mozangle`, necessaria per qualunque binario `servoshell` su qualsiasi piattaforma — vedi
  `CUSTOMIZATIONS.md`). Verificate invece applicando le tre patch in sequenza contro
  un'estrazione pristine del tag `v0.4.0` e confrontando byte-per-byte il risultato con la
  copia di lavoro di questo fork — le patch riproducono fedelmente la modifica, ma il codice
  non è stato controllato dal compilatore in questa sessione. Stesso caveat delle altre voci
  di questo file in attesa del punto 1 (build patchata reale) e di un ambiente con `libclang`
  disponibile.
