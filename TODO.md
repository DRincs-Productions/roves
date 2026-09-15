# TODO — cose da fare su questo fork di Servo

Backlog di lavoro noto ma non ancora fatto sulla copia vendorizzata/patchata di Servo in
questa cartella. Vedi [`CUSTOMIZATIONS.md`](./CUSTOMIZATIONS.md) per le modifiche già
applicate e [`CLAUDE.md`](./CLAUDE.md) per il protocollo da seguire quando si chiude uno di
questi punti (aggiornare `CUSTOMIZATIONS.md` + rigenerare la patch nella stessa sessione).


## Backlog attuale — revisione 2026-09-13

Questa sezione è la lista operativa aggiornata dopo il controllo di `main`.
Le sezioni numerate sotto sono conservate come storico: le loro dichiarazioni
su repository sibling e workflow dormienti non descrivono necessariamente la
struttura attuale. Le modifiche nei repository esterni non sono state verificate
in questa revisione e non vengono considerate completate per inferenza.

### Android: integrazione con Google Play — da implementare

In `main` non risultano SDK o bridge per Google Play Games Services, Billing o
Play Asset Delivery. La presenza di `google()` nei repository Gradle serve a
risolvere dipendenze e non costituisce un'integrazione Google Play.

- [ ] **Pubblicazione:** aggiungere output Android App Bundle (`.aab`) nel percorso `mach bundle`, mantenendo l'APK per test/sideload. Gestire application ID per gioco e versionCode/versionName espliciti: oggi il modulo usa `org.servo.servoshell` e una versione del motore.
- [ ] **Configurazione release:** collegare upload key/Play App Signing al flusso documentato; verificare firma reale, aggiornamento di una versione installata e gestione delle credenziali in CI. Controllare i requisiti target SDK e delle librerie native richiesti al momento della pubblicazione, senza fissare nel TODO requisiti che cambiano nel tempo.
- [ ] **Play Games Services:** integrare autenticazione/stato del giocatore, obiettivi e classifiche tramite SDK Android nativo e un bridge asincrono per il gioco web; configurare ID, certificati e account di test. Definire comportamento offline, annullamento ed errori.
- [ ] **Salvataggi cloud Google Play:** integrare Saved Games se previsto per il gioco; definire conflitti tra dispositivi, metadati/versioni e recupero offline. Non presumere che i salvataggi Steam Cloud offrano già un backend Android equivalente.
- [ ] **Acquisti in-app, opzionali:** aggiungere Play Billing per i giochi che lo richiedono, con catalogo/configurazione per gioco, ripristino degli acquisti, gestione pending/cancellazioni e verifica/acknowledgement delle transazioni. Definire quali operazioni richiedono un backend del gioco.
- [ ] **Asset grandi, opzionali:** integrare Play Asset Delivery con contenuti install-time/fast-follow/on-demand; collegare disponibilità, progresso, errori e percorsi degli asset al loader. Non confondere asset pack Android con gli archivi tar/zstd desktop.
- [ ] **Distribuzione di test:** documentare Play Console e provare una release su un track di test e un dispositivo reale. La pubblicazione automatica tramite Developer Publishing API è un passo successivo configurabile; non attivarla automaticamente nei workflow esistenti.
- [ ] **API e strumenti esterni:** esporre configurazione e capability in `roves-api`, `roves-action` e Packmaster con interventi dedicati nei rispettivi repository; assenza dei servizi deve produrre uno stato non disponibile prevedibile.

Riferimenti ufficiali: [Android App Bundle](https://developer.android.com/guide/app-bundle),
[Play Games Services](https://play.google.com/console/about/playgamesservices/),
[Play Billing](https://developer.android.com/google/play/billing),
[Play Asset Delivery](https://developer.android.com/guide/playcore/asset-delivery),
[Publishing API](https://developers.google.com/android-publisher).
Le funzionalità opzionali non sono prerequisiti universali per pubblicare un gioco.

### Decisione API Google Play — libreria opzionale

- [ ] Realizzare il supporto Google Play come libreria separata, integrata nella libreria API JavaScript (`roves-api`), senza renderlo una dipendenza obbligatoria del core Roves. Nome del pacchetto da definire.
- [ ] Dichiarare il pacchetto Google Play in `peerDependencies` della libreria API, con `peerDependenciesMeta["<pacchetto-google-play>"].optional = true`. Il gioco installa esplicitamente il pacchetto solo quando vuole utilizzare l'integrazione. Non aggiungerlo alle dipendenze obbligatorie.
- [ ] Usare import/entry point opzionali: importare il core API senza il pacchetto Google Play deve funzionare e non deve caricare il modulo Google. Un peer opzionale da solo non rende sicuro un import statico obbligatorio.
- [ ] Implementare il backend Android con SDK ufficiale Google in Kotlin e bridge asincrono verso JavaScript, senza introdurre un wrapper Rust/JNI salvo una necessità concreta di logica condivisa.
- [ ] Rendere l'integrazione Android disabilitata per default e abilitabile nel packaging; aggiungere SDK Gradle e configurazione Google soltanto alle build che la richiedono. Le dipendenze native Gradle non sono peer dependency npm.
- [ ] Esporre un contratto coerente nello stile con Steam: disponibilità, stato autenticazione/profilo, obiettivi e classifiche; distinguere servizio assente, utente non autenticato, annullamento ed errore. API asincrone con comportamento documentato anche fuori da Android.
- [ ] Mantenere Play Billing un modulo separato e opzionale. Verificare installazione/build del core senza integrazione e del gioco con pacchetto e backend Google abilitati.

Questa decisione specifica la voce Google Play sopra; l'implementazione della
libreria API richiede un intervento nel suo repository, non viene effettuata da
questo aggiornamento del TODO dell'engine.

### iOS: Game Center e API opzionale — da implementare

- [ ] Integrare Game Center tramite il framework ufficiale GameKit, con backend Swift e bridge asincrono verso JavaScript nella WKWebView. Non introdurre un wrapper Rust salvo una necessità concreta di logica condivisa.
- [ ] Realizzare una libreria JavaScript Game Center separata (nome da definire), collegata a `roves-api` come `peerDependencies` con `peerDependenciesMeta["<pacchetto-game-center>"].optional = true`. Il gioco installa il pacchetto esplicitamente; il core API deve funzionare senza di esso, evitando import statici obbligatori.
- [ ] Rendere l'integrazione disabilitata per default e attivabile nel packaging iOS. Aggiungere capability/entitlement Game Center e configurazione del progetto soltanto quando richiesti; documentare configurazione degli ID in App Store Connect.
- [ ] Esporre disponibilità, stato autenticazione/profilo giocatore, obiettivi e classifiche con API asincrone coerenti nello stile con Steam e Google Play. Gestire utente non autenticato, annullamento, errori e uso fuori dalla piattaforma supportata.
- [ ] Valutare salvataggi GameKit con requisiti iCloud/capability appropriati, sincronizzazione tra dispositivi, conflitti, versioni e funzionamento offline; mantenere distinto il salvataggio locale dal backend cloud.
- [ ] Valutare matchmaking e multiplayer GameKit come estensione opzionale successiva, se richiesti dai giochi.
- [ ] Implementare acquisti in-app tramite StoreKit in un modulo separato e opzionale, con verifica delle transazioni, ripristino e gestione degli stati di acquisto.
- [ ] Verificare build senza integrazione e build abilitata, autenticazione e funzionalità su dispositivi reali, con account/configurazione di test appropriati. Coordinare libreria API e strumenti di packaging nei rispettivi repository.

Riferimenti ufficiali: [Game Center](https://developer.apple.com/game-center/),
[GameKit](https://developer.apple.com/documentation/gamekit),
[StoreKit](https://developer.apple.com/documentation/storekit).
Questa voce aggiunge il lavoro al backlog; non implementa né abilita i servizi Apple.

### Epic Online Services: API opzionale — da implementare

- [ ] Realizzare l'integrazione EOS come libreria JavaScript separata (nome da definire), collegata a `roves-api` in `peerDependencies` con `peerDependenciesMeta["<pacchetto-eos>"].optional = true`. Il gioco installa esplicitamente il pacchetto; il core API deve funzionare senza di esso, evitando import statici obbligatori.
- [ ] Rendere il backend EOS disabilitato per default e abilitabile nel packaging. Includere SDK e librerie native soltanto nelle build che lo richiedono; iniziare dai target desktop Windows/macOS/Linux e verificare separatamente supporto e requisiti mobile.
- [ ] Valutare SDK ufficiale EOS tramite FFI Rust e wrapper mantenuti: verificare compatibilità, licenza, distribuzione dei binari, callback, gestione della memoria, inizializzazione/tick e arresto. Non presumere l'esistenza di un SDK Rust ufficiale.
- [ ] Configurare prodotto, sandbox, deployment e client policy nel Developer Portal. Distinguere autenticazione Epic Account Services e identità EOS Connect/Product User ID, scegliendo il flusso richiesto dal gioco; non distribuire credenziali privilegiate o segreti del backend nel bundle.
- [ ] Esporre disponibilità, autenticazione/stato giocatore, obiettivi, statistiche e classifiche con API asincrone coerenti nello stile con Steam, Google Play e Game Center. Documentare capability effettive e gestire servizio assente, offline, annullamento ed errori.
- [ ] Integrare Player Data Storage come backend cloud opzionale, con quote, versioni, conflitti tra dispositivi e recupero offline; preservare i salvataggi locali e mantenere distinti i backend Steam/Google/Apple/EOS.
- [ ] Valutare lobby, sessioni, matchmaking, P2P e voce come estensioni opzionali successive, secondo le esigenze dei giochi.
- [ ] Mantenere distinta l'integrazione EOS dalla distribuzione e dalle funzionalità commerciali dell'Epic Games Store: non rendere la pubblicazione nello store un prerequisito dell'API.
- [ ] Verificare build del core senza EOS e build abilitata, caricamento delle librerie distribuite, callback/lifecycle, autenticazione con account di test, obiettivi/classifiche e conflitti cloud. Coordinare libreria API e strumenti di packaging nei rispettivi repository.

Riferimento ufficiale: [Epic Online Services](https://dev.epicgames.com/docs/epic-online-services/eos-overview).
Questa voce pianifica l'integrazione opzionale; non introduce SDK o funzionalità runtime.

### Mobile: completare il percorso applicativo

- [ ] **WebView native Android/iOS:** proposta nella [PR #7](https://github.com/DRincs-Productions/roves/pull/7), non presente in `main` alla revisione. Verificare prima di sostituire il percorso Servo/JNI corrente.
- [ ] **iOS:** contenitore e packaging completi, firma/archive/export, icone e metadati per gioco, test su simulatori/dispositivi. Il contenitore proposto in #7 carica file locali: restano origine stabile, fetch/moduli, routing e storage compatibili con i giochi reali.
- [ ] **Bridge mobile Roves:** definire capability e contratti asincroni per salvataggi, servizi store e funzionalità native; evitare di presumere che `game://`, `steam:` e i protocolli desktop esistano automaticamente nelle WebView.
- [ ] **Verifiche Android aperte:** firma release effettiva; bundling su Windows; caricamento di routing, asset assoluti, moduli e storage su dispositivo; pausa/ripresa, rotazione, fullscreen, audio e gamepad. Il codice presente e una build debug riuscita non chiudono questi test.
- [ ] **Manifest residuo:** definire quali campi hanno un equivalente mobile sensato (`background_color`, display, lingua ed entry point) e testarli. Name/short_name/orientation, candidati manifest e risoluzione icona sono già presenti: non reimplementarli. Non reintrodurre la status bar contro il design precedente.

### Desktop: prestazioni, contenuti e salvataggi

- [ ] **Baseline hardware:** confermare GPU/renderer e assenza di fallback software sui target reali Windows/macOS/Linux; fissare giochi, risoluzioni, cache e metodologia di misura.
- [ ] **Frame pacing e rendering:** valutare refresh/vsync del monitor, costo composizione della shell e repaint separati GUI/contenuto. Indagine nella [PR #8](https://github.com/DRincs-Productions/roves/pull/8), non implementazioni completate.
- [ ] **I/O e asset:** evitare letture/decompressione bloccanti nei percorsi sensibili; misurare copie e code dei blocchi; valutare prefetch e pack per livello con cancellazione e invalidazione cache coerenti.
- [ ] **Consumi in background:** verificare e collegare occlusione/minimizzazione al throttling desktop, preservando la politica del gioco per audio e multiplayer.
- [ ] **GC incrementale:** includere il lavoro lungo di correttezza Servo–SpiderMonkey (inventario pre-barriere, correzioni, stress test, poi benchmark). Non abilitarlo semplicemente tramite flag: `script_runtime.rs` segnala pre-barriere non corrette. Piano nella PR #8.
- [ ] **Salvataggi locali robusti:** valutare scritture atomiche, backup/recupero da interruzioni, schema/versioni e migrazioni; il protocollo attuale usa scritture dirette ai file.
- [ ] **Conflitti Steam Cloud:** il backend esiste già, ma documenta una politica locale-prima con download cloud quando manca il file locale. Implementare confronto/versioni e risoluzione dei conflitti se richiesti; verificare errori/quota/offline senza perdere il salvataggio locale.
- [ ] **Steam avanzato, opzionale:** valutare classifiche e ulteriori servizi richiesti dai giochi. Obiettivi, statistiche, DLC, overlay e Steam Cloud sono già implementati: concentrare il backlog sulle capability mancanti e sui test reali.
- [ ] **Aggiornamenti contenuti, opzionali:** progettare aggiornamento/versionamento dei contenuti senza ricreare sempre il bundle, con integrità, rollback e coerenza della cache. Decidere prima se serve un updater completo o basta il meccanismo di aggiornamento della piattaforma store.

### Dipendenze, release e manutenzione

- [ ] **Aggiornamenti mirati:** valutare mozjs nella serie attuale e runtime nativo GStreamer, poi prove coordinate egui/ANGLE/zstd/allocatore. Analisi separata nella [PR #9](https://github.com/DRincs-Productions/roves/pull/9); nessun bump è già stato applicato.
- [ ] **Grafo desktop effettivo:** controllare feature e duplicazioni del target scelto prima di rimuovere dipendenze o deduplicare major incompatibili; non trattare tutto il lockfile workspace come contenuto del binario.
- [ ] **Distribuzione del fork:** i workflow test/release/android sono alla radice di questo repository e quindi non sono qui dormienti. Verificare separatamente che i consumer esterni scarichino Roves patchato; il vecchio punto 1 non dimostra un difetto ancora presente in quei repository.
- [ ] **Matrice di regressione:** aggiungere/verificare smoke test delle app confezionate con routing, JS, storage, salvataggi, grafica, audio e API native; separare test della build, test della firma e test sul dispositivo.
- [ ] **Riproducibilità:** ogni modifica runtime deve essere riproducibile da upstream + patch e mantenere coerenti documentazione, lockfile, bundle e asset CI. Gli aggiornamenti del solo backlog non richiedono una patch del motore.
- [ ] **Console:** definire priorità e feasibility per i target roadmap prima di considerarli supportati; implementazioni e validazione richiedono gli SDK e ambienti appropriati.

### Regola per chiudere i punti

Distinguere sempre **implementato**, **verificato** e **proposto in PR**. Un punto
si chiude con evidenza pertinente al comportamento finale. Il presente aggiornamento
è una revisione statica del backlog: non certifica build/device test né modifica
funzionalità del motore.

## Storico delle verifiche e degli interventi precedenti

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

## 3. Android: leggere tutto `manifest.webmanifest` (non solo `orientation`), override via parametro, e riflettere tutto in `roves-action`/Roves Packmaster (`roves-packmaster`)

**Stato: fatto (2026-09-01/02, branch `android` su tutti e tre i repo).** Copertura completa
del manifest (`name`/`short_name`/`orientation`, non più solo `orientation`) + override
espliciti lato engine (vedi `CUSTOMIZATIONS.md`, voce "`mach bundle --android`: full manifest
coverage..."); `roves-action` espone `android-app-name`/`android-orientation`/
`android-theme-color`; Roves Packmaster (`roves-packmaster`) ha sia la UI (card Mobile, switch
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
- **Roves Packmaster** (cartella sibling `roves-packmaster`, package.json name `roves-packmaster` —
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
backend Android di Roves Packmaster (`roves-packmaster/src-tauri/src/android.rs`).

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
sessione). `check_android_availability()` in `roves-packmaster/src-tauri/src/android.rs` va comunque
aggiornato per smettere di bloccare Windows, e il tutto va riverificato su un runner Windows
reale prima di considerare questo punto davvero chiuso — vedi il punto 6 sotto.

## 5. Firma dell'APK Android (release signing)

**Stato: lato motore fatto (2026-09-10). Il percorso debug (senza `--android-release`) è
verificato end-to-end via CI reale (vedi punto 3 sopra); `--android-release` in sé resta non
verificato** (nessun ambiente CI con un keystore reale lo esercita ancora). Lato Roves
Packmaster: fatto (2026-09-10, `src-tauri/src/signing.rs`), anch'esso non verificato con una
build reale — vedi `roves-packmaster/TODO.md` #2. `roves-action` ancora da fare (vedi sotto). Vedi
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
`roves-packmaster/src-tauri/src/android.rs` va comunque riscritto per il nuovo bundling
Gradle-only — vedi `roves-packmaster/TODO.md`.

**Stato (storico, pre-pivot): sbloccato dal punto 4, ma non ancora chiuso.** Il blocco Gradle (`ndk-build` senza
fallback `.cmd`) è risolto (punto 4), ma restano da fare, in ordine:

1. Rimuovere il blocco esplicito in `check_android_availability()`
   (`roves-packmaster/src-tauri/src/android.rs`) che oggi disabilita Android su Windows in Packmaster
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
Servo to native WebView".**

**Aggiornamento 2026-09-14 — punti 1 e 2 chiusi:**

1. **Migrare `roves-action` e `roves-packmaster`/Packmaster al nuovo bundling Gradle-only —
   fatto**, verificato leggendo il codice attuale di entrambi i repo (non solo da questa
   sessione: già così quando controllato). `roves-action`'s `android`/`ios` non scaricano più
   `roves_android_native_arm64.zip`; Packmaster's `android.rs` non fa più bootstrap
   NDK/Rust — solo JRE + Android SDK + Gradle, esattamente come descritto qui sopra.
2. **Wire iOS staging into `mach bundle` — fatto (2026-09-14).** `_bundle_ios`/
   `_sign_and_export_ios_release` in `post_build_commands.py`: `--ios`/`--ios-app-name`/
   `--ios-bundle-id`/`--ios-release` reali, con firma via `IOS_SIGNING_CERTIFICATE_P12_PATH`/
   `_PASSWORD`/`IOS_SIGNING_PROVISIONING_PROFILE_PATH`/`IOS_SIGNING_TEAM_ID` (stesso schema
   "env var in, rifiuta se mancano" di `--android-release`). Vedi `CUSTOMIZATIONS.md`, voce
   "Wire `--ios`/`--ios-release` into `mach bundle`", per il dettaglio completo — incluso
   **perché** la firma Android e quella iOS non sono simmetriche (keystore autofirmata vs.
   certificato che solo Apple può controfirmare). Stessa giornata: `roves-action` ha guadagnato
   `android-release`/`android-keystore-*` e `ios-release`/`ios-certificate-*`/
   `ios-provisioning-profile-*`/`ios-team-id`; Roves Packmaster ha guadagnato un'intera sezione
   iOS (`ios.rs`/`ios_signing.rs` + UI in `configure.tsx`), parallela a quella Android già
   esistente (la firma Android in Packmaster era **già completa** prima di questa sessione —
   trovata leggendo `configure.tsx`, non mancante come una prima ricognizione superficiale
   aveva sospettato).
   **Non verificato end-to-end su una build reale**: nessun toolchain Rust funzionante su
   questa macchina Windows (linker mancante, vedi i gap di toolchain già noti altrove in
   questo file) per un `cargo check` di Packmaster, e nessun macOS/Xcode per eseguire
   `xcodebuild` per davvero da nessuna parte in questa sessione. Verificato staticamente
   (rilettura riga per riga, un test Python isolato per `_bundle_ios`/
   `_sign_and_export_ios_release` con `subprocess`/`security`/`xcodebuild` mockati, `tsc`/
   Biome puliti per il lato TypeScript di Packmaster) — la vera verifica resta la CI reale
   (`ios.yml`/`android.yml` del motore, `test.yml` di Packmaster) più, alla fine, qualcuno con
   un Mac reale che prova davvero `--ios-release`/il tab iOS di Packmaster. La firma iOS
   *release* end-to-end resta non verificabile del tutto finché non arriva un certificato
   Apple Distribution + provisioning profile reali (richiede un account Apple Developer
   Program reale — generata una CSR a questo scopo, consegnata fuori banda, non committata).
3. **Verifica su dispositivo reale, entrambe le piattaforme** — **Android: fatta (2026-09-14)**,
   su un dispositivo reale (Pixel 8 Pro) con un APK reale (`pixi-vn-react-template` via
   `roves-action`). Ha trovato e corretto bug reali, non solo confermato che "funzionava":
   barre di stato/navigazione mai nascoste di default, body del 404 nullo invece di reale,
   fallback SPA dead-code (`AssetsPathHandler.handle()` non ritorna mai `null`, a differenza
   di quanto assunto), registrazione Service Worker fallita (serve un
   `ServiceWorkerControllerCompat` separato), e — la causa vera dietro un primo "Not Found"
   visto in due round di fix precedenti — l'app caricava `.../index.html` invece della radice
   `/`, facendo sì che il router lato client del gioco (non Android) rendesse la propria
   pagina 404. Vedi `CUSTOMIZATIONS.md`, voci del 2026-09-14, per il dettaglio completo di
   ogni bug e come sono stati diagnosticati (Chrome DevTools via `chrome://inspect`, non solo
   lettura del codice — la lezione esplicita di quella sessione è che "CI verde + code review"
   da sola aveva mancato la causa vera per due round consecutivi).
   **iOS: ancora non verificato** — nessun macOS/Xcode/simulatore/dispositivo disponibile in
   nessuna sessione finora. Il codice (`App.swift`) è stato riletto riga per riga contro gli
   stessi identici bug trovati su Android (URL di boot, logica di fallback) e **non li ha**:
   carica già la radice nuda `game://content/`, e il suo fallback usa un vero controllo
   `FileManager.fileExists` invece di un `?:` su un valore che non è mai `nil` — ma questo
   resta un'analisi statica, non una conferma su un device/simulatore reale. **Se capita di
   avere del tempo e un Mac/dispositivo iOS a disposizione**: fare lo stesso identico test
   fatto su Android (build reale via `roves-action`'s `ios: 'true'` o `support/ios/bundle.py`,
   avvio su un device/simulatore reale, ispezione con Safari Web Inspector — l'equivalente
   iOS di `chrome://inspect`) e condividere il risultato sul Discord del progetto
   (<https://discord.gg/E95FZWakzp>) — utile sia se conferma che tutto funziona, sia
   soprattutto se emerge un problema analogo a quelli trovati su Android.
4. **APK signing** (punto 5 sopra) resta valido e non affetto dal pivot — `--android-release`
   funziona identicamente nel nuovo `_bundle_android`.

**Aggiornamento 2026-09-15 — bug CI del job `ios` diagnosticato e risolto:** `mach bundle --ios`
falliva in CI in ~6 secondi, prima ancora di entrare in `_bundle_ios` — causa: il decoratore
`binary_selection` di `command_base.py` risolve `servo_binary` chiamando
`self.get_binary_path(...)` a meno che `self.target.needs_packaging()` sia vero, e questo è vero
per Android/OpenHarmony (hanno una propria `BuildTarget` subclass) ma **non** per iOS (che non ne
ha una — resta sul target host macOS di default). Risultato: `get_binary_path` cercava un
binario `servoshell` mai compilato (`ios.yml` non esegue mai `mach build`) e falliva con "No
Servo binary found" prima che il branch `--ios` di `bundle()` venisse mai raggiunto. Vedi
`CUSTOMIZATIONS.md`, voce "Fix `mach bundle --ios` failing immediately", per il dettaglio
completo. **Confermato su CI reale (2026-09-15)**: il job `ios` è ora verde end-to-end (build,
upload artifact, packaging, upload release "test" tutti riusciti) — vedi
`CUSTOMIZATIONS.md` per il link al run. **Rimane aperto, non collegato a questo fix**:
`ios-release-signing-smoke` fallisce separatamente allo step `security import` — causa non
ancora confermata (nessuna annotation utile, nessun PAT GitHub disponibile per leggere il log
reale in questa sessione).

**Decisione ancora in sospeso, chiesta esplicitamente all'utente in una sessione precedente
(2026-09-14), risposta: "aspetta" — non ancora ridecisa in questa sessione.** Perché
`roves-action`/Roves Packmaster possano davvero usare `--ios`/`--ios-release` (oggi puntano a un
tag motore già pubblicato che non li contiene affatto), serve tagliare una nuova release del
motore (vedi `CLAUDE.md`, "Cutting a versioned release") e poi aggiornare il pin in
`roves-action/action.yml` e `roves-packmaster/src/lib/shell-version.ts`'s `TARGET_SHELL_VERSION`.
L'utente aveva chiesto di rivedere prima il codice — da richiedere conferma esplicita prima di
procedere, non tagliare la release di propria iniziativa solo perché il bug del job `ios` è
stato risolto.

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
