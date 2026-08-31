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

**Stato:** noto, non ancora iniziato — richiesto esplicitamente come lavoro successivo al primo
giro di bundling Android (vedi `CUSTOMIZATIONS.md`, voce "`mach bundle --android`: pack
`--content-dir`...", 2026-08-31), che oggi legge solo il campo `orientation`.

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
