# TODO — cose da fare su Roves

Backlog di lavoro noto ma non ancora fatto: player CEF desktop (`support/cef-*`) e, per
Android/iOS, il motore Servo al tag `v0.7.15`. Le modifiche già fatte sono nella cronologia
git (il vecchio changelog, fermo al 28 novembre 2026, è a [roves 791eff000a6](https://github.com/DRincs-Productions/roves/blob/791eff000a6/CUSTOMIZATIONS.md)) e
[`AGENTS.md`](./AGENTS.md) descrive il protocollo da seguire quando si chiude uno di questi
punti.

Questo file contiene solo lavoro **non ancora fatto**. Una volta completata una voce, va
rimossa da qui (il dettaglio resta nel messaggio di commit e nel README del componente).

## Mod e DLC — player, roves-api e roves-cli fatti, resto da fare

Fatto il 27-28 novembre 2026: scoperta, compatibilità, DLC ufficiali tramite registro
(`roves-dlc.json` -> `game/roves-game.json`), versione più recente vince, stato, route e modulo
`mods` nei tre player (`support/cef-windows-player/src/mods.rs`), roves-api (`mods`,
`process.relaunch`) e roves-cli (`roves mod ...`, `bundle --dlc/--no-mods`). Da fare:

- [ ] **Mod kit:** `roves bundle` scrive per i modder i tipi dell'API passata a `loadAll`, le librerie condivise, nome e versione del gioco; `roves mod init --kit`. (Il resto di roves-cli è fatto: `mod init/dev/check/pack`, registro `roves-dlc.json`, `bundle --dlc/--no-mods/--project-dir`.)
- [ ] **`roves mod dev` su macOS/Linux:** provato solo con test sintetici; verificare l'avvio del player e il riavvio su modifica con un pacchetto reale.
- [ ] **Librerie condivise (fase 3):** import map generato da roves-cli/plugin Vite per le librerie che il gioco condivide, chunk separati; `shared` in `roves-game.json`. Le mod con `peerDependencies` restano incompatibili finché il gioco non condivide le librerie.
- [ ] **roves-action, Packmaster:** opzioni `--mods`, `--dlc`; Packmaster con selettore "Game / Mods & DLC" e schermata per creare e impacchettare una mod.
- [ ] **Release e pin:** pubblicare roves-api (versione nuova), release del player, aggiornare i pin; poi togliere "Coming soon" dalla pagina wiki `integrations/mods-dlc` e pubblicarla.
- [ ] **Verifiche reali:** `core/relaunch` sui tre sistemi (macOS: avvio del binario interno al bundle), DLC grandi (tempo dello SHA-256 all'avvio: valutare una cache per dimensione e data nel profilo), mod con `main` caricata da un gioco Pixi'VN reale.
- [ ] **Steam Workshop (rinviato):** iscrizione alle mod tramite UGC di steamworks-rs.

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

- [ ] **iOS:** contenitore e packaging completi, firma/archive/export, icone e metadati per gioco, test su simulatori/dispositivi. Il contenitore WKWebView (PR #7, unita il 13 settembre 2026) carica file locali: restano origine stabile, fetch/moduli, routing e storage compatibili con i giochi reali.
- [ ] **Bridge mobile Roves:** definire capability e contratti asincroni per salvataggi, servizi store e funzionalità native; evitare di presumere che `game://`, `steam:` e i protocolli desktop esistano automaticamente nelle WebView.
- [ ] **Verifiche Android aperte:** firma release effettiva; bundling su Windows; caricamento di routing, asset assoluti, moduli e storage su dispositivo; pausa/ripresa, rotazione, fullscreen, audio e gamepad. Il codice presente e una build debug riuscita non chiudono questi test.
- [ ] **iOS su dispositivo/simulatore reale:** mai verificato — nessun macOS/Xcode/simulatore/dispositivo disponibile in nessuna sessione finora. Il codice (`App.swift`) è stato riletto riga per riga contro gli stessi bug trovati su Android (URL di boot, logica di fallback) e non li ha — carica già la radice nuda `game://content/`, fallback con vero `FileManager.fileExists` — ma resta un'analisi statica, mai confermata su un device/simulatore reale. Fare lo stesso test fatto su Android (build reale via `roves-action`'s `ios: 'true'` o `support/ios/bundle.py`, avvio su device/simulatore reale, ispezione con Safari Web Inspector).
- [ ] **Manifest residuo:** definire quali campi hanno un equivalente mobile sensato (`background_color`, display, lingua ed entry point) e testarli. Name/short_name/orientation, candidati manifest e risoluzione icona sono già presenti: non reimplementarli. Non reintrodurre la status bar contro il design precedente.

## Desktop: prestazioni, contenuti e salvataggi

- [ ] **Salvataggi locali robusti:** le scritture sono già atomiche (host API, `tempfile` + rename); restano backup/recupero, schema/versioni e migrazioni.
- [ ] **Steam Cloud nel player CEF:** c'era nella shell Servo, non nel player CEF. Valutare prima Steam Auto-Cloud (configurazione su Steamworks della cartella `saves` del profilo, nessun codice) e solo se serve la Remote Storage API; definire conflitti, quota e offline senza perdere il salvataggio locale.
- [ ] **Steam avanzato, opzionale:** valutare classifiche e ulteriori servizi richiesti dai giochi. Obiettivi, statistiche, DLC e overlay sono già implementati: concentrare il backlog sulle capability mancanti e sui test con il client Steam reale.
- [ ] **Aggiornamenti contenuti, opzionali:** progettare aggiornamento/versionamento dei contenuti senza ricreare sempre il bundle, con integrità, rollback e coerenza della cache. Decidere prima se serve un updater completo o basta il meccanismo di aggiornamento della piattaforma store.

### Prestazioni del player CEF

Vedi il [backlog prestazioni](docs/ROVES_PERFORMANCE.md) per le misure e gli interventi ancora aperti.

## Dipendenze, release e manutenzione

- [ ] **Grafo desktop effettivo:** controllare feature e duplicazioni del target scelto prima di rimuovere dipendenze o deduplicare major incompatibili; non trattare tutto il lockfile workspace come contenuto del binario.
- [ ] **Matrice di regressione:** aggiungere/verificare smoke test delle app confezionate con routing, JS, storage, salvataggi, grafica, audio e API native; separare test della build, test della firma e test sul dispositivo.
- [ ] **Contenuto impacchettato su macOS/Linux:** la CI ordinaria prova gli archivi solo su Windows (lettore condiviso, test unitari ovunque). Alla prossima release verificare dal pacchetto roves-action (impostazioni di default) che il gioco parta dagli archivi anche su macOS e Linux (log `gate: game content`), e misurare su hardware reale avvio, p95/p99 dei frame e accesso agli asset rispetto ai file sciolti.
- [ ] **cef-rs / download-cef — file dei pacchetti:** `download-cef` 3.0.1 scarta `LICENSE.txt` della distribuzione CEF ma conserva `CMakeLists.txt`, `archive.json` e `CREDITS.html`, e i bundler `build_util` (Windows/Linux) copiano ogni file di primo livello, inclusi quelli solo di build. Oggi i nostri bundler li rimuovono, spostano `CREDITS.html` in `licenses/` e usano `resources/licenses/CEF-LICENSE.txt` (2026-11-27). Aprire issue/PR upstream (bundler: solo file di runtime; download-cef: conservare `LICENSE.txt`) e togliere il nostro codice quando una release lo corregge.
- [ ] **Console:** definire priorità e feasibility per i target roadmap prima di considerarli supportati; implementazioni e validazione richiedono gli SDK e ambienti appropriati.

## Regola per chiudere i punti

Distinguere sempre **implementato**, **verificato** e **proposto in PR**. Un punto
si chiude con evidenza pertinente al comportamento finale, poi va rimosso da questo file
(il dettaglio va nel messaggio di commit e nel README del componente).


## Link e navigazione: approfondire la compatibilita nel documento unico

- [ ] Valutare come giochi e librerie usano link HTML ordinari e router: frammenti,
  query, path relativi/assoluti, link dello stesso origin e link esterni. Un href che
  cambia query/path senza intercettazione SPA normalmente carica un nuovo documento,
  operazione bloccata da Roves. Il fix delle sole fixture non dimostra compatibilita
  generale e non autorizza una conversione automatica di tutti gli anchor nel motore.
- [ ] Definire contratti e prove per clic reale/sintetico, tastiera (Enter), Ctrl/Shift,
  tasto centrale, target=_blank, preventDefault e handler/router annidati. Distinguere
  History API, popstate e hashchange, ritorno avanti/indietro, stato e documento preservati.
- [ ] Documentare per gli sviluppatori quando usare un router/History API o un frammento,
  e che cosa viene bloccato. Valutare diagnostica piu chiara per i link interni bloccati
  senza alert modali, falsi successi o reset dello stato del gioco. Per gli esterni resta
  il browser di sistema solo dopo attivazione utente affidabile, mai apertura automatica.
- [ ] Confrontare librerie/router mantenuti e casi reali prima di introdurre un adattatore
  condiviso. Preservare sicurezza, document lifetime e policy prodotto; non allentare il
  blocco delle navigazioni complete solo per fare funzionare un link di prova.

## Player CEF: punti aperti

Il player è pubblicato nelle release v0.9.0 e v0.9.1 sui tre sistemi (standard, Steam, MCP).
Dettagli nei README di `support/cef-windows-player`, `support/cef-macos-gate`,
`support/cef-linux-gate`; prestazioni nel [backlog prestazioni](docs/ROVES_PERFORMANCE.md).

- [ ] **Prove su hardware reale (mai fatte in CI, che non ha GPU):**
  - Windows con GPU (GTX 1050): rilascio della schermata di avvio, input, icona in Esplora risorse e barra delle applicazioni, salvataggio che sopravvive al riavvio, seconda istanza (exit 6), display al 150%.
  - Mac reale: frame IOSurface -> Metal, Cmd+Q e pulsante rosso, tastiera non US, tasti morti/IME, HiDPI, audio, icona nel Dock.
  - Desktop Linux con GPU (Intel/Mesa su X11/Xwayland): import DMA-BUF e frame mostrati; AMD/RADV atteso `IMPLICIT_MODIFIER` sulla riga wgpu-29.
  - PC senza GPU: fallback software (`OnPaint`).
  - Client Steam reale (overlay, proprietà dei DLC) al posto dell'emulatore GSE della CI.
  - Bridge MCP con un client MCP reale su un gioco vero, sui tre sistemi.
  - Link esterni con un browser di sistema reale (la fixture Windows non clicca link).
- [ ] **Fork temporaneo di welding** (`support/vendor/wgpu-weld`, `ROVES_PATCHES.md`): aprire su merely-made/wgpu-weld issue e PR per il fallback software `OnPaint` e per il veto delle navigazioni del main frame (testi pronti in `ROVES_PATCHES.md`), registrare qui i link; a ogni release di welding ricontrollare e tornare alla dipendenza pubblicata appena entrambe le modifiche sono incluse. Misurare il costo CPU dell'upload software.
- [ ] **Segnalazioni upstream a welding:** `cef/accelerated_osr` porta nel grafo un wgpu 30 inutilizzato; `OnContextCreated`/`extra_info` non esposti all'host (servirebbero per il marker di pagina senza bootstrap `about:blank`).
- [ ] **macOS/Linux, decisioni di distribuzione:** firma e notarizzazione del `.app` (e della dylib Steam); keychain reale o `--use-mock-keychain`; App Translocation; `--password-store=basic` su Linux; Wayland nativo; Steam Linux Runtime (pressure-vessel, sandbox setuid); integrazione desktop Linux (`.desktop`, icone hicolor); IME in composizione.
- [ ] **MCP, da decidere:** server Rust (`rmcp`) per togliere il requisito Node; strumento screenshot (`request_snapshot_png` di welding).
- [ ] Ricontrollare a ogni salto di versione il pin `cef =154.5.0` del player rispetto alle release di cef-rs e di welding.
