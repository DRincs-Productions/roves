# TODO — cose da fare su questo fork di Servo

Backlog di lavoro noto ma non ancora fatto sulla copia vendorizzata/patchata di Servo in
questa cartella. Vedi [`CUSTOMIZATIONS.md`](./CUSTOMIZATIONS.md) per le modifiche già
applicate (changelog storico completo) e [`CLAUDE.md`](./CLAUDE.md) per il protocollo da
seguire quando si chiude uno di questi punti (aggiornare `CUSTOMIZATIONS.md` + rigenerare la
patch nella stessa sessione).

Questo file contiene solo lavoro **non ancora fatto**. Una volta completata una voce, va
rimossa da qui (il suo racconto/dettaglio storico vive in `CUSTOMIZATIONS.md`, non qui).

## Android: integrazione con Google Play — da implementare

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

## iOS: Game Center e API opzionale — da implementare

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

## Epic Online Services: API opzionale — da implementare

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

## Mobile: completare il percorso applicativo

- [ ] **WebView native Android/iOS:** proposta nella [PR #7](https://github.com/DRincs-Productions/roves/pull/7), non presente in `main` alla revisione. Verificare prima di sostituire il percorso Servo/JNI corrente.
- [ ] **iOS:** contenitore e packaging completi, firma/archive/export, icone e metadati per gioco, test su simulatori/dispositivi. Il contenitore proposto in #7 carica file locali: restano origine stabile, fetch/moduli, routing e storage compatibili con i giochi reali.
- [ ] **Bridge mobile Roves:** definire capability e contratti asincroni per salvataggi, servizi store e funzionalità native; evitare di presumere che `game://`, `steam:` e i protocolli desktop esistano automaticamente nelle WebView.
- [ ] **Verifiche Android aperte:** firma release effettiva; bundling su Windows; caricamento di routing, asset assoluti, moduli e storage su dispositivo; pausa/ripresa, rotazione, fullscreen, audio e gamepad. Il codice presente e una build debug riuscita non chiudono questi test.
- [ ] **iOS su dispositivo/simulatore reale:** mai verificato — nessun macOS/Xcode/simulatore/dispositivo disponibile in nessuna sessione finora. Il codice (`App.swift`) è stato riletto riga per riga contro gli stessi bug trovati su Android (URL di boot, logica di fallback) e non li ha — carica già la radice nuda `game://content/`, fallback con vero `FileManager.fileExists` — ma resta un'analisi statica, mai confermata su un device/simulatore reale. Fare lo stesso test fatto su Android (build reale via `roves-action`'s `ios: 'true'` o `support/ios/bundle.py`, avvio su device/simulatore reale, ispezione con Safari Web Inspector).
- [ ] **Manifest residuo:** definire quali campi hanno un equivalente mobile sensato (`background_color`, display, lingua ed entry point) e testarli. Name/short_name/orientation, candidati manifest e risoluzione icona sono già presenti: non reimplementarli. Non reintrodurre la status bar contro il design precedente.

## Desktop: prestazioni, contenuti e salvataggi

- [ ] **Baseline hardware:** confermare GPU/renderer e assenza di fallback software sui target reali Windows/macOS/Linux; fissare giochi, risoluzioni, cache e metodologia di misura.
- [ ] **Frame pacing e rendering:** valutare refresh/vsync del monitor, costo composizione della shell e repaint separati GUI/contenuto. Indagine nella [PR #8](https://github.com/DRincs-Productions/roves/pull/8), non implementazioni completate. Un utente reale ha confrontato una build Roves con lo stesso gioco (PixiJS) su Chrome, trovando Chrome nettamente più fluido e leggero (CPU/RAM) — nessuna causa specifica ancora identificata lato Roves oltre alla generale minore maturità della pipeline grafica di Servo rispetto a Chromium; da investigare con profiling reale (vedi voce GPU/renderer sotto e la nuova voce "Ottimizzazione prestazioni" più sotto).
- [ ] **I/O e asset:** evitare letture/decompressione bloccanti nei percorsi sensibili; misurare copie e code dei blocchi; valutare prefetch e pack per livello con cancellazione e invalidazione cache coerenti.
- [ ] **Consumi in background:** verificare e collegare occlusione/minimizzazione al throttling desktop, preservando la politica del gioco per audio e multiplayer.
- [ ] **GC incrementale:** includere il lavoro lungo di correttezza Servo–SpiderMonkey (inventario pre-barriere, correzioni, stress test, poi benchmark). Non abilitarlo semplicemente tramite flag: `script_runtime.rs` segnala pre-barriere non corrette. Piano nella PR #8.
- [ ] **Salvataggi locali robusti:** valutare scritture atomiche, backup/recupero da interruzioni, schema/versioni e migrazioni; il protocollo attuale usa scritture dirette ai file.
- [ ] **Conflitti Steam Cloud:** il backend esiste già, ma documenta una politica locale-prima con download cloud quando manca il file locale. Implementare confronto/versioni e risoluzione dei conflitti se richiesti; verificare errori/quota/offline senza perdere il salvataggio locale.
- [ ] **Steam avanzato, opzionale:** valutare classifiche e ulteriori servizi richiesti dai giochi. Obiettivi, statistiche, DLC, overlay e Steam Cloud sono già implementati: concentrare il backlog sulle capability mancanti e sui test reali.
- [ ] **Aggiornamenti contenuti, opzionali:** progettare aggiornamento/versionamento dei contenuti senza ricreare sempre il bundle, con integrità, rollback e coerenza della cache. Decidere prima se serve un updater completo o basta il meccanismo di aggiornamento della piattaforma store.
- [ ] **Verificare che la GPU venga usata correttamente** (no fallback software): confermare che questa build di Servo usi effettivamente l'accelerazione hardware per il rendering (WebGL/WebGPU) su una build reale di ciascuna piattaforma della matrice CI (Windows/macOS/Linux) — non ancora fatto. `../test-page/` ha già i pulsanti "Test PixiJS render"/"Test Three.js render" (con fps a schermo) e `GpuInfoPanel` (legge `WEBGL_debug_renderer_info`, mostra renderer/vendor GPU effettivo ed euristica "software renderer") pronti per questa verifica.
- [ ] **Collegare `embedded.yml` al build patchato invece del binario Servo stock:** `../.github/workflows/embedded.yml` scarica oggi il binario `servoshell` ufficiale precompilato da `servo/servo` (vedi `SERVO_TAG`), non il fork patchato in questa cartella. Finché resta così, ogni release "embedded" mostra ancora la UI browser stock che le patch in `CUSTOMIZATIONS.md` rimuovono solo nel codice, non nel binario distribuito. Opzioni: pubblicare `servo/` come repo standalone e far scaricare a `embedded.yml` i suoi artifact di build, oppure integrare uno step di build-from-source direttamente dentro `embedded.yml`.

### Ottimizzazione prestazioni — punto di partenza per la prossima sessione

Richiesta esplicita dell'utente (2026-09-22): valutare ottimizzazioni concrete, partendo dal
presupposto che Rust/questo stack (rispetto a un motore C++ come Chromium) *dovrebbe* poter
competere almeno sull'uso di memoria, anche se Chrome ha vent'anni di ottimizzazione in più sulla
pipeline grafica. Nessun profiling reale è ancora stato fatto (nessun hardware/tool di profiling
GPU/CPU disponibile in questa sessione) — punti da investigare quando sarà possibile:

- [ ] Profilare con strumenti reali (non solo lettura del codice) dove va il tempo/la memoria in
  un caso concreto (es. `visual-novel-template` con PixiJS), confrontando con lo stesso contenuto
  in Chrome, per capire se il gap è nella pipeline di compositing di Servo (webrender/surfman),
  nell'architettura offscreen-render-poi-blit di `headed_window.rs` (`rendering_context`/
  `window_rendering_context`, vedi i loro doc comment), o altrove.
- [ ] Verificare se Tracy/Perfetto (già integrati, vedi sotto) possono essere accesi su una build
  reale per ottenere dati concreti invece di ipotesi.
- [ ] Valutare se il fast path wgpu per WebView singola fullscreen (vedi sezione dedicata sotto)
  è la stessa ottimizzazione che risolverebbe questo gap, o se è un problema distinto.

## Diagnostica prestazioni: Tracy e Perfetto

Tracy e Perfetto sono già collegati (vedi `CUSTOMIZATIONS.md`) ma non ancora usati per una vera
sessione di profiling. Restano da fare:

- [ ] Costruire una telemetria Roves comune con clock monotono, frame ID,
  aggregati p50/p95/p99, frame oltre budget e buffer circolare preallocato.
  Evitare log, allocazioni e serializzazione per ogni frame.
- [ ] Nelle release includere soltanto diagnostica Roves leggera, disabilitata
  per default e attivabile esplicitamente dall'utente. Nessun upload automatico;
  omettere URL, percorsi e dati sensibili per default.
- [ ] Aggiungere overlay opzionale e trigger manuale/automatico per congelare gli
  ultimi secondi intorno a uno scatto. L'overlay deve leggere aggregati esistenti
  e non modificare il percorso di rendering quando è nascosto.
- [ ] Eseguire prove A/B con strumentazione spenta, diagnostica release attiva e
  profiling completo, dichiarando overhead e campioni persi.

Riferimenti: [Tracy](https://github.com/wolfpld/tracy) e
[Perfetto](https://perfetto.dev/docs/).

## SDL3 windowing: gap residuo

La migrazione da `winit`/`gilrs` a SDL3 (finestra, event loop, input, gamepad) è completa e
mergiata su `main` (rilasciata da `v0.4.24` in poi) — vedi `CUSTOMIZATIONS.md` per lo storico
completo e [`SDL3_WINDOWING_TESTING.md`](./SDL3_WINDOWING_TESTING.md) per la checklist di verifica
hardware ancora aperta (IME CJK/dead key, screen reader, ecc.).

- [ ] **Gamepad disabilitato su macOS, causa non confermata:** `sdl3::init().gamepad()` si blocca
  indefinitamente su CI macOS (un run ha bruciato le 6 ore di timeout di GitHub Actions prima di
  essere cancellato forzatamente). Un'ipotesi (disabilitare solo il backend IOKit) è stata provata
  e confermata via CI **non** risolvere l'hang — vedi `CUSTOMIZATIONS.md` per l'analisi completa.
  Serve un Mac reale interattivo per osservare cosa succede davvero, oppure un esperimento mirato
  a basso costo (un workflow minimo che isola solo `sdl3::init().gamepad()` dal resto della build,
  così un tentativo sbagliato costa minuti di CI e non ore). Nel frattempo il gamepad resta
  disattivato specificamente su macOS (gate `not(target_os = "macos")` in `app.rs`/
  `running_app_state.rs`/`window.rs`) — un vero gap funzionale rispetto a GilRs su quella
  piattaforma.

## Rendering futuro: fast path wgpu — backlog

- [ ] Progettare un fast path interno a Servo per il caso di una singola
  WebView/canvas fullscreen, inizialmente WebGPU, riducendo composizione, blit,
  clear e copie GPU non necessari. Mantenere WebRender come percorso completo.
- [ ] Attivarlo solo quando la pagina e lo stato grafico soddisfano condizioni
  verificabili; DOM/CSS, overlay, dialoghi, trasparenza o feature incompatibili
  devono tornare automaticamente al percorso generale senza differenze visibili.
- [ ] Preservare resize/HiDPI, color space e alpha, screenshot/capture, context
  loss, metriche, accessibilità e presentazione coerente su Windows, macOS e
  Linux.
- [ ] Implementarlo soltanto dopo telemetria e baseline: confrontare copie GPU,
  composizione, latenza, p95/p99 e frame oltre budget. Questa è
  un'ottimizzazione futura, non una priorità dell'intervento corrente.

## Dipendenze, release e manutenzione

- [ ] **Aggiornamenti mirati:** valutare mozjs nella serie attuale e runtime nativo GStreamer, poi prove coordinate egui/ANGLE/zstd/allocatore. Analisi separata nella [PR #9](https://github.com/DRincs-Productions/roves/pull/9); nessun bump è già stato applicato.
- [ ] **Grafo desktop effettivo:** controllare feature e duplicazioni del target scelto prima di rimuovere dipendenze o deduplicare major incompatibili; non trattare tutto il lockfile workspace come contenuto del binario.
- [ ] **Distribuzione del fork:** verificare che i consumer esterni (`roves-action`, Packmaster) scarichino Roves patchato al tag corretto — vedi anche il pin `roves-ref`/`TARGET_SHELL_VERSION`, va bumped a ogni release rilevante (vedi `CLAUDE.md`).
- [ ] **Matrice di regressione:** aggiungere/verificare smoke test delle app confezionate con routing, JS, storage, salvataggi, grafica, audio e API native; separare test della build, test della firma e test sul dispositivo.
- [ ] **Riproducibilità:** ogni modifica runtime deve essere riproducibile da upstream + patch e mantenere coerenti documentazione, lockfile, bundle e asset CI. Gli aggiornamenti del solo backlog non richiedono una patch del motore.
- [ ] **Console:** definire priorità e feasibility per i target roadmap prima di considerarli supportati; implementazioni e validazione richiedono gli SDK e ambienti appropriati.
- [ ] **APK signing — `roves-action`:** nuovi input `android-keystore-*` (verosimilmente un keystore base64-encoded via GitHub Secret, decodificato in un file temporaneo, con le 4 variabili d'ambiente `APK_SIGNING_KEY_STORE_PATH`/`_STORE_PASS`/`_ALIAS`/`_PASS` impostate prima di invocare `mach bundle`). Lato motore e Packmaster già fatto (vedi `CUSTOMIZATIONS.md`, voce "`mach bundle --android-release`").

## Regola per chiudere i punti

Distinguere sempre **implementato**, **verificato** e **proposto in PR**. Un punto
si chiude con evidenza pertinente al comportamento finale, poi va rimosso da questo file
(il dettaglio va in `CUSTOMIZATIONS.md`).
