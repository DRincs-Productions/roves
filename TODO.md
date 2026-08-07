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

## 2. Rimuovere il popup del menu contestuale (tasto destro) in `dialog.rs`

`Dialog::ContextMenu` in `ports/servoshell/desktop/dialog.rs` espone un menu tasto-destro in
stile browser (Back/Forward/Reload/View Source/Inspect...). Per un videogioco questo è
indesiderato: rompe l'immersione e espone voci come "View Source" che non hanno senso fuori
da un browser.

**Deciso:** disabilitarlo del tutto (no-op sul tasto destro), non sostituirlo con un menu
alternativo. Non ancora toccato — nessuna patch esiste per questo punto.

## 3. Rimuovere la possibilità di effettuare il reload della pagina

Per un videogioco il reload della pagina (scorciatoia da tastiera e/o voce di menu) è
indesiderato: può resettare stato di gioco in modo inatteso. Da individuare dove il reload è
attualmente esposto (es. `Dialog::ContextMenu` in `ports/servoshell/desktop/dialog.rs`,
eventuali keybinding come Ctrl+R/F5) e disabilitarlo. Non ancora toccato — nessuna patch
esiste per questo punto.

## 4. Rimuovere del tutto la possibilità di navigazione "indietro"

L'utente non deve poter tornare a una pagina precedente in nessun modo (scorciatoie da
tastiera, gesture del trackpad/mouse, ecc.) — per un videogioco tornare "indietro"
può rompere completamente lo stato di gioco. Da `CUSTOMIZATIONS.md` (voce del 2026-08-06
sulla rimozione dello stato di navigazione morto da `gui.rs`): la navigazione back/forward
reale (Alt+freccia sinistra/destra, ecc.) chiama `WebView::go_back`/`go_forward` direttamente
in `headed_window.rs`, bypassando `Gui` — è lì che va cercato e disabilitato il keybinding,
oltre a qualunque voce "Back" rimasta nel menu contestuale (vedi punto 2 sopra). Non ancora
toccato — nessuna patch esiste per questo punto.

## 5. Verificare che la GPU venga usata correttamente (no fallback software)

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

## 6. Schermata bianca su contenuto `file://` — causa risolta in questo fork, ma non ancora nella build "embedded" reale

**Stato:** causa individuata e patchata in questo fork (vedi `CUSTOMIZATIONS.md`, patch
`0007-stable-file-origin-for-module-script-loading`). **Verificato end-to-end il
2026-08-07**: `../test-page/` (bundle Vite multi-chunk, script esterno) carica ed esegue
correttamente su una build reale — niente più schermata bianca, i pulsanti diagnostici
rispondono. Quel test reale ha però scoperto una conseguenza dell'origine opaca che resta:
vedi il punto 6b sotto.

Causa: `ImmutableOrigin::new_opaque_for_file()` in `components/url/origin.rs` generava un
UUID casuale nuovo ad ogni chiamata per gli URL `file://`, quindi due chiamate a `.origin()`
sullo stesso URL `file://` non erano mai uguali tra loro. Questo faceva fallire
silenziosamente il fetch di qualunque `<script type="module" src="...">` esterno aperto via
`file://` (la fetch va in modalità "cors", che richiede same-origin, che per `file://` non
era mai vero) — lo script non veniva mai eseguito e la pagina restava bianca. Qualunque
bundle Vite "normale" (multi-chunk, script esterno) — sia quello di `../test-page/` sia
quello del progetto principale (root `vite.config.ts`, chunk multipli via `manualChunks`) —
ne era affetto. Il fix rende l'origine `file://` stabile (stesso id fisso per tutte le
origini `file://`) invece che casuale ad ogni chiamata — vedi `CUSTOMIZATIONS.md` per i
dettagli e i trade-off accettati (storage condiviso tra documenti `file://`, inerte per
questo fork perché apre sempre un solo documento `file://` e non espone navigazione ad
altri).

**Importante — questo fix da solo non basta per la build "embedded" reale:** il punto 1 di
questo file spiega che `../.github/workflows/embedded.yml` scarica oggi il binario
`servoshell` **stock, non patchato**, non il fork con le patch di questa cartella. Questo fix
vive solo nelle patch di questo fork, quindi finché il punto 1 non viene risolto (far
costruire/consegnare a `embedded.yml` il binario patchato), la build "embedded" realmente
distribuita continuerà ad avere questo identico bug, non toccata da questa patch.

Prossimi passi: una volta risolto anche il punto 1, ripetere la verifica con il `dist/`
reale del gioco (non solo `../test-page/`).

## 6b. Storage (`localStorage`/`indexedDB`) inutilizzabile su contenuto `file://` — limite strutturale, non un bug della patch del punto 6

**Scoperto:** 2026-08-07, testando `../test-page/` su una build reale — `DiagnosticsPanel`/
`StorageButton` riportano `SecurityError: Cannot access localStorage from opaque origin.`;
`IndexedDbButton` fallisce nello stesso modo.

**Causa:** il fix del punto 6 rende l'origine `file://` *stabile* (stesso id fisso ad ogni
chiamata), ma resta comunque un'origine **opaca** (`ImmutableOrigin::Opaque`, non `Tuple`) —
`ImmutableOrigin::new()` in `components/url/origin.rs` tratta `file://` come opaco a
prescindere, e `components/script/dom/storage/storagemanager.rs` (righe 136/181/237) nega
esplicitamente qualunque storage shelf per origini opache: `"Storage is unavailable for
opaque origins"`. Questo è comportamento voluto dallo Storage Standard, non un difetto della
patch del punto 6 — nessuna origine opaca, in nessun browser conforme, può avere
`localStorage`/`indexedDB`/Cache API. Non è quindi "risolvibile" restando su `file://`.

**Perché conta per Roves:** un gioco web tipico usa `localStorage`/`indexedDB` per i
salvataggi. Se il bundle viene aperto come oggi (`file://`, via `--content-dir`/
`--html-file` di `./mach bundle`), i salvataggi lato-storage **non funzioneranno mai**, su
nessuna piattaforma — non è un bug da aspettare che si risolva, è strutturale a `file://`.

**Possibile via d'uscita (non implementata, da valutare):** servire il bundle tramite uno
schema custom con un host (es. `resource://app/index.html` invece di
`resource:///percorso`, o un nuovo schema dedicato) invece che `file://`. Un `ProtocolHandler`
del genere esiste già come pattern (`protocols/resource.rs`), ma oggi non ha host e comunque
`ImmutableOrigin::new()` delegherebbe a `url.origin()`, che per uno schema non "speciale"
(non in ftp/file/http/https/ws/wss) ritorna comunque opaco — servirebbe un secondo
special-case in `origin.rs`, analogo a quello già presente per `file`, che costruisca un
`ImmutableOrigin::Tuple` per quello schema. Non fatto: è una scelta architetturale (aggiunge
un'origine "vera" navigabile, da valutare rispetto al modello kiosk single-document attuale),
non un fix a riga singola.

**Nota collaterale:** anche `navigator.clipboard` è risultato `undefined` nello stesso test —
causa diversa, non legata all'origine: è dietro il pref Servo `dom_async_clipboard_enabled`
(`components/config/prefs.rs:407`), `false` di default upstream, più `[SecureContext]` nel
WebIDL (`Navigator.webidl:77`). Si abilita passando `--pref dom_async_clipboard_enabled=true`
al lancio di `servoshell`; se serve acceso di default per i giochi Roves, è un candidato per
un futuro override dei pref di default in questo fork (non fatto).

## Note

- Punto risolto nella sessione del 2026-08-06: stato di navigazione browser morto
  (location/back-forward/load-status/favicon) rimosso da `gui.rs` — vedi
  `CUSTOMIZATIONS.md`, patch `0003-strip-dead-browser-navigation-state-and-favicon-pipeline`.
  Lasciato intenzionalmente intatto `browser_tab`/`toolbar_button`: sono dead code senza
  alcun chiamante, già eliminati dal compilatore nelle build di release, quindi rimuoverli
  non avrebbe alcun effetto sul pacchetto di gioco finale.
