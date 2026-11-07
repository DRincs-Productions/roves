# Handoff — Migrazione del motore rendering/DOM da Servo a Blitz con il runtime Deno/V8 esistente

- **Data:** 6 novembre 2026
- **Repository:** `roves`, `main` a `81bb7e99d1f`. Working tree già sporco prima di questo
  handoff (`CUSTOMIZATIONS.md`, `TODO.md`, `docs/ROVES_PERFORMANCE.md`, submodule `roves-action`
  e `roves-wiki`, `patches/servo-v0.5.0/0462-gameplay-smoothness-plan.patch` non tracciata):
  **non toccato**, non è parte di questo lavoro e non va incluso nei commit della migrazione.
- **Predecessore:** `docs/handoff/2026-11-06-valutazione-blitz-aurora.md` (valutazione con
  raccomandazione provvisoria "mantenere Servo"). **Superata dalla decisione esplicita
  dell'utente** riportata al punto 1; le sue sezioni su gate e spike restano utili come base.
- **Ambito iniziale:** piano e primo incarico. L'aggiornamento 6 novembre documenta anche la
  prima prova tecnica e lo stato della delega Claude; il codice resta isolato in
  `support/blitz-v8-probe/`.

## 0. Livelli di evidenza e fonti

- **[CONFERMATO]** verificato in questa sessione su file locali di `roves`.
- **[RICORDO]** conoscenza pregressa del modello (al più metà 2026), **non verificata**.
- **[IGNOTO]** nessuna informazione affidabile.
- **[INFERENZA]** deduzione, con i fatti da cui deriva.

**Fonti primarie lette e verifica locale in questa sessione:**

- [Repository/README DioxusLabs Blitz](https://github.com/DioxusLabs/blitz): stato beta,
  assenza di binding JavaScript, moduli `blitz-dom`, `blitz-html`, `blitz-paint`,
  `blitz-shell`, uso di Stylo/Taffy/Parley e renderer AnyRender. Il repo spiega anche che il
  wrapper HTML statico non è interattivo.
- [Discussione upstream sul runtime JS](https://github.com/DioxusLabs/blitz/issues/119):
  il maintainer conferma che Blitz può ospitare un runtime JS ma non offre attualmente binding;
  interattività e identità/riferimenti DOM restano responsabilità da implementare.
- [API documentate di Blitz DOM](https://docs.rs/blitz-dom/latest/blitz_dom/) versione 0.2.4 e
  [crate aggregatore Blitz](https://docs.rs/blitz/latest/blitz/) versione 0.2.1: `BaseDocument`
  è headless e pensato per essere guidato da un embedder; `Document` espone eventi UI e poll.
- [Fork PathScale di Blitz](https://github.com/pathscale/ps-blitz), i cui crate `ps-blitz-dom`,
  `ps-blitz-html`, `ps-blitz-shell`, `ps-blitz` e `ps-blitz-dom-api` risultano alla versione
  0.6.5 nel registry, `MIT OR Apache-2.0`, Rust 1.89.0. `ps-blitz-dom` usa Stylo 0.20.0 ma
  anche `ps-stylo-taffy`/`ps-taffy`; non è il repository upstream DioxusLabs.
- [Facade DOM runtime-agnostica `ps-blitz-dom-api`](https://docs.rs/ps-blitz-dom-api/latest/blitz_dom_api/)
  0.6.5, e [mappatura operazioni](https://docs.rs/crate/ps-blitz-dom-api/0.6.5/source/MAPPING.md):
  estrae operazioni DOM dalla binding Boa in funzioni Rust indipendenti dal runtime e copre 46
  operazioni. Non gestisce eventi/listener, identità/cache wrapper, coercizione JS, custom element,
  invalidazione layout, flush o redraw. Il crate corrispondente
  [`ps-blitz-script`](https://docs.rs/ps-blitz-script/latest/ps_blitz_script/) integra Boa: è
  incompatibile con il requisito Roves e non va aggiunto.
- `cargo info` e sorgente crates.io consultati in locale: PS Blitz `0.6.5` pubblicato il
  14 ottobre 2026 (commit confezionato `307b283899b3d15fab6fa9466d7bef1c2e4d5c1d`); il repo
  PathScale ha commit `ec2f62439925b4732b6a89e5370d899b0d115567` su main durante questa sessione.
  Questa attività recente non certifica da sola governance o compatibilità futura.

Queste fonti confermano perimetro e versioni di riferimento, non il funzionamento intero del
probe, la compatibilità WebGL, il compositing di canvas, o la parità multipiattaforma. Il
confronto locale del 6 novembre ha inoltre trovato che PS Blitz 0.6.5 compila i crate ma fallisce
il check Windows con 197 errori nella dipendenza `nagoya 0.1.19`, che usa API POSIX non
disponibili nel target. Il probe adotta quindi la linea ufficiale DioxusLabs. `blitz 0.2.1` è
usato come riferimento per versioni coerenti, non come launcher: il suo `launch_*` prende il
controllo dell'event loop, mentre Roves deve governarlo.

Evidenze locali del probe:

- `support/blitz-v8-probe/README.md`, `Cargo.toml`, `Cargo.lock`, fixture HTML e workflow.
- `cargo tree -i v8`: una sola V8 `150.4.0`, via `deno_v8 0.5.0` → `deno_core 0.413.0`.
- `cargo check --locked` e `cargo run --locked` passano su Windows per Blitz DOM/HTML/shell/
  paint/AnyRender e Deno/V8.
- Il run verifica uno script V8 che chiama un op `deno_core` sincrono, mutazione del testo nel
  documento Blitz e `BaseDocument::resolve(0.0)`; box root 640×360. Il op è dichiarato `#[op2(fast)]`.
  Wrapper/identità DOM JS, shell visibile, paint, input/eventi, RAF e Motion restano non
  implementati. Nessuna misura di fluidità o performance raccolta.

Fonti da verificare nei gate successivi:

- https://github.com/DioxusLabs/blitz/releases e file `Cargo.toml`/workspace del commit pinnato
- https://github.com/DioxusLabs/anyrender
- https://docs.rs/blitz-dom, https://docs.rs/blitz-html, https://docs.rs/blitz-paint,
  https://docs.rs/blitz-shell, https://docs.rs/blitz-net, https://docs.rs/blitz-traits
- https://crates.io/api/v1/crates/blitz-dom (versione e data dell'ultima release)
- https://blitz.is e https://github.com/DioxusLabs/blitz/issues?q=canvas
- https://docs.rs/deno_core, https://github.com/denoland/deno/tree/main/ext (`deno_web`,
  `deno_webgpu`, `deno_webstorage`, `deno_fetch`) per il riuso di estensioni Web standard
- https://github.com/orottier/web-audio-api-rs (candidato Web Audio)

File locali letti: `AGENTS.md`, il predecessore sopra, `docs/LIBRERIE_E_CONSOLE.md` (§ Console,
righe 95–129), `docs/ROVES_PERFORMANCE.md` (righe 1–90), `Cargo.toml` (workspace),
`Cargo.lock` (versioni), `support/deno-core-probe/Cargo.toml`.

## 1. Direzione autorizzata e vincoli che restano

**Priorità del runtime:** Deno + V8 gestiscono tutto il codice JavaScript del gioco e il suo runtime (isolate, moduli, microtask/event loop); le Web API standard vanno riusate da estensioni Deno mantenute quando adatte. Blitz assume DOM, CSS, layout e rendering e riceve dal runtime gli aggiornamenti/eventi tramite un ponte sottile. Non aggiungere un secondo runtime JS o un secondo event loop indipendente.

**Architettura obiettivo (decisione esplicita dell'utente, 6 novembre 2026):** sostituire lo
stack Servo di DOM, stile, layout, rendering e shell con **Blitz** (DioxusLabs), integrando il
**runtime Deno/V8 già presente** (`deno_core` fork vendorizzato in
`components/roves-js/vendor/deno_core`, `v8 150.4.0` [CONFERMATO]). I giochi restano scritti in
JavaScript per il web (HTML/CSS/JS compilati, ad es. con Vite). L'implementazione è delegata a
Claude con la procedura AGENTS.

Per questa migrazione la decisione **supera** in AGENTS i vincoli "conservare la shell Servo" e
"la shell Servo e DOM/layout/rendering Servo restano", e il principio che `roves-js` mantenga la
superficie JSAPI compatibile con Servo. **Codex deve registrare la nuova direzione in `AGENTS.md`
e `TODO.md` prima di delegare T1** (questo handoff non li modifica).

Restano validi senza eccezioni:

- **Un solo runtime JS:** un solo V8 nel processo e un solo `JsRuntime`/isolate per il documento;
  nessun secondo motore (Boa, QuickJS, V8 duplicato) e nessun secondo grafo runtime in produzione.
- **Nessuna API privilegiata Deno esposta ai giochi** (`Deno.*`, file system, processi, env,
  FFI, permessi). Eventuali estensioni `deno_*` sono ammesse solo per API Web standard.
- **Documento unico:** un solo documento di gioco per finestra; route interne con History API e
  frammenti; reload e navigazioni complete bloccate prima di sostituire il documento; avviso
  discreto solo in sviluppo, log nelle build distribuite; link esterni nel browser di sistema
  solo dopo clic utente reale.
- **Sicurezza di caricamento:** protocollo `game://`, origini, CSP, import map, contratti degli
  errori di moduli/script e microtask già definiti in AGENTS (inclusa la deviazione autorizzata
  `unhandledrejection` per gli errori di valutazione dei moduli statici e il frammento
  `#roves-inline-module-N` per i moduli inline).
- **JITless** deve restare possibile (requisito console, `docs/LIBRERIE_E_CONSOLE.md` [CONFERMATO]).
- Procedura AGENTS: build pesanti online in CI, `CUSTOMIZATIONS.md` + patch per ogni modifica,
  sincronizzazione `roves-action`/`roves-packmaster`/wiki quando cambia la superficie
  pubblicata, nessuna pubblicazione di WIP, nessuna falsa verifica UI su Windows bloccato.

## 2. Natura del lavoro

È una **migrazione di motore browser**, non la sostituzione di un renderer. DioxusLabs Blitz
fornisce DOM nativo in Rust, parsing HTML, stile (Stylo), layout (Taffy), testo (Parley), paint
astratto (`anyrender`) e una shell `winit`; **non fornisce un motore JavaScript né binding
JavaScript** [CONFERMATO da fonti upstream §0]. In Servo questa parte è il crate `script` già
adattato a V8 (`roves-js`). Con Blitz vanno quindi forniti, riusando librerie dove possibile:

binding DOM per JS, eventi e propagazione, CSSOM/`getComputedStyle`/`getBoundingClientRect`,
timer/rAF/microtask legati al frame, canvas 2D, **WebGL/WebGL2 per PixiJS**, Web Audio, media,
storage persistente, `fetch`/moduli con CSP e origini, History API, Web Animations
(`element.animate`, `getAnimations`), font, input/IME/gamepad, accessibilità, policy shell,
Steam, overlay di diagnostica, packaging su cinque piattaforme.

[INFERENZA] Il guadagno atteso non è nello stile/layout (Servo usa già `stylo 0.20.0` e
`taffy 0.12.2` [CONFERMATO]) ma nella rimozione del codice Servo vendorizzato e delle sue patch,
a fronte di un livello script nuovo da costruire. Il piano deve far emergere presto se questo
scambio è fattibile.

## 3. Strategia: fasi che avanzano, non attesa di ricerca

1. **Fase S (spike isolato):** vertical slice Blitz + V8 fuori dal workspace di produzione
   (`support/blitz-v8-probe`, stesso schema di `support/deno-core-probe`: `[workspace]` proprio,
   `Cargo.lock` proprio, `publish = false`). Rimozione = cancellare la cartella.
2. **Fase M (mappa e implementazione API):** matrice di compatibilità sul template (§6); per
   ogni API mancante scegliere libreria mantenuta o codice di collegamento e implementarla
   nella probe, una famiglia per incarico.
3. **Fase P (produzione):** solo dopo i gate G3–G5 (§7), su branch dedicato
   `blitz-migration`: nuovo crate shell/engine nel workspace, rimozione progressiva dei
   componenti Servo, Servo ancora rilasciabile da `main` finché non c'è parità verificata.

Ogni gate ha un deliverable concreto e una decisione (avanti / correggi / stop). Un gate fallito
porta a una decisione di Codex o dell'utente con le opzioni elencate, **non** a un nuovo giro di
ricerca senza codice. Se le fonti upstream restano inaccessibili, T1 procede comunque usando
i crate pubblicati (il codice sorgente scaricato da Cargo è fonte primaria) e annota gli ignoti.

## 4. Crate e responsabilità

| Area | Candidato | Stato per Roves | Lacuna / da fare |
| --- | --- | --- | --- |
| DOM nativo | `blitz-dom` upstream 0.2.4 oppure `ps-blitz-dom` fork PathScale 0.6.5 | [CONFERMATO] API Rust nativa; entrambi usano Stylo/Taffy/Parley | confrontare superficie shell e dipendenze; pin esatto |
| Parsing HTML | `blitz-html` (html5ever) | [RICORDO]; `html5ever 0.39.0` già nel lock [CONFERMATO] | `innerHTML`/fragment parsing da esporre a JS |
| Binding DOM riusabile | `ps-blitz-dom-api` 0.6.5 (fork PathScale; MIT/Apache) | [CONFERMATO] 46 operazioni indipendenti da Boa/V8 | non include eventi, oggetti JS, layout dirty/flush/redraw; T1 lo valuta e non aggiunge `ps-blitz-script` (Boa) |
| Stile | Stylo (via Blitz) | [CONFERMATO] già usato da Servo; versione Blitz [IGNOTO] | in produzione la versione di Servo sparisce con Servo; transizioni/animazioni CSS: supporto [IGNOTO] |
| Layout | Taffy (via Blitz) | [CONFERMATO] già nel lock | inline/testo via Parley; `position`, `overflow`, `transform`: copertura [IGNOTO] |
| Testo/font | Parley, fontique, skrifa | [RICORDO] usati da Blitz; `skrifa` già nel lock [CONFERMATO] | `FontFace`/`document.fonts` da fornire |
| Paint | `blitz-paint` + `anyrender` (Vello GPU, Vello CPU) | [RICORDO]; `vello 0.9.0`, `wgpu 29.0.4` già nel lock [CONFERMATO] | versione wgpu di Blitz [IGNOTO]; meccanismo per texture esterne (canvas) [IGNOTO] |
| Shell/finestra | `blitz-shell` (winit) | [RICORDO]; oggi Roves usa SDL3 (`sdl3 0.20.0`) e `winit 0.30.13` è nel lock [CONFERMATO] | scegliere winit (meno codice) o SDL3; gamepad: winit non lo copre [RICORDO] → `gilrs` o SDL3 |
| Rete | `blitz-net` / `NetProvider` | [RICORDO] basato su reqwest | provider `game://`, CSP, origini, nessun accesso di rete non autorizzato |
| JS | `deno_core` fork vendorizzato, `v8 150.4.0` | [CONFERMATO] runtime di produzione unico | ponte DOM da scrivere; event loop da legare al frame Blitz |
| API Web base | estensioni `deno_web` (timer, URL, encoding, streams, structuredClone, event target) | [RICORDO] MIT, mantenute da Deno | verificare che non espongano `Deno.*`; compatibilità con il fork 0.413 |
| DOM per JS | Opzione A: facciata sottile su op `deno_core` verso `blitz-dom`. Opzione B: DOM JS mantenuto (`linkedom` ISC, `happy-dom` MIT) [RICORDO] con mutazioni rispecchiate in `blitz-dom` | [INFERENZA] B riduce il codice nostro per l'API DOM, A riduce memoria e doppio stato | **Deciso il 7 novembre: opzione A.** `happy-dom` richiede built-in Node (`vm`, `fs`, `buffer`); `linkedom` ha dispatch eventi semplificata senza vera fase capture e MutationObserver non standard. Event/EventTarget/DOMException da `deno_web 0.291.0` (dispatch spec sul `parentNode` dei nodi). Dettagli nel README della probe |
| WebGL/WebGL2 | Nessuna libreria Rust mantenuta che implementi WebGL su wgpu [RICORDO]. Candidati: (1) ANGLE/EGL + texture condivisa D3D11→D3D12 importata in wgpu; (2) backend GLES di wgpu su ANGLE con FBO nello stesso contesto; (3) riuso del codice WebGL di Servo (resta codice nostro) | **lacuna principale**, [IGNOTO] | spike T2; nessun readback CPU per frame |
| WebGPU (alternativa) | `deno_webgpu` (wgpu) [RICORDO]; PixiJS v8 ha un renderer WebGPU [RICORDO] | [INFERENZA] può condividere il device wgpu di Blitz se le versioni coincidono | verificare se Pixi'VN consente di scegliere WebGPU; non sostituisce WebGL finché il template non lo usa |
| Canvas 2D | Vello o `tiny-skia` [RICORDO] | [IGNOTO] | serve se il template/typewriter lo usa (da matrice) |
| Web Audio | `web-audio-api` (orottier) [RICORDO], MIT | [IGNOTO] per Roves | verificare licenza, cpal sulle 5 piattaforme, copertura nodi usati da Tone/@pixi/sound |
| Media `<video>` | GStreamer oggi [CONFERMATO da AGENTS/README] | [IGNOTO] con Blitz | solo se la matrice lo richiede |
| Storage | `deno_webstorage` (localStorage, SQLite) [RICORDO]; IndexedDB: `fake-indexeddb` JS + persistenza nostra [INFERENZA] | oggi storage "a forma di IndexedDB" su file [CONFERMATO README] | compatibilità dei salvataggi esistenti |
| History/single-document | nostro (piccolo) | policy attuale in `ports/servoshell` [CONFERMATO AGENTS] | ricostruire su Blitz: History API in JS, blocco navigazioni nella shell |
| Animazioni Web | `element.animate`/`getAnimations` | Roves v0.7.16: `getAnimations` mancante [CONFERMATO `docs/ROVES_PERFORMANCE.md:84-88`] | implementare o polyfill mantenuto; verificare Motion 12.38.0 |
| Accessibilità | AccessKit | [RICORDO] integrato in Blitz; `accesskit 0.24.1` nel lock [CONFERMATO] | verificare versione e piattaforme |
| IME/input | winit IME [RICORDO] | [IGNOTO] in Blitz | composizione testo nei campi del gioco (nome giocatore) |
| Piattaforme | winit copre Win/Linux/macOS/Android/iOS [RICORDO] | supporto Blitz mobile [IGNOTO] | ordine: Windows → Linux → macOS → Android → iOS |

I manifest dei crate selezionati dichiarano Blitz/anyrender `MIT OR Apache-2.0`; il probe resta
`license = "MPL-2.0"` come `deno-core-probe`. Verificare le licenze transitive prima della
distribuzione.

## 5. Spike: obiettivo esatto

**Piattaforma:** Windows per primo. **Runtime:** il fork `deno_core` vendorizzato
(`path = "../../components/roves-js/vendor/deno_core"`); se il suo `Cargo.toml` usa ereditarietà
del workspace e non compila isolato, usare `deno_core = "=0.413.0"` da crates.io e registrare la
differenza. Un solo `JsRuntime` per processo, creato una volta.

Prima di pinnare il motore, confrontare la distribuzione DioxusLabs Blitz (`blitz-dom` 0.2.4,
`blitz` 0.2.1 nelle docs.rs correnti) con il fork PathScale PS Blitz (`ps-blitz-*` 0.6.5,
MIT/Apache, crate pubblicati il 14 ottobre 2026). Il fork ha la facade runtime-agnostica
`ps-blitz-dom-api` (46 operazioni DOM); non usare `ps-blitz-script`, che porta Boa. T1 può
scegliere il fork solo se l'API DOM, la shell e il grafo versioni si integrano al singolo
deno_core/V8 con meno codice mantenuto da Roves e senza regressioni di sicurezza. Motivare la
scelta nel report; non mescolare tipi dei due grafi Blitz.

### T1 — Blitz + V8 + DOM + eventi + Motion (probe in corso)

**Checkpoint T1a/T1b-op completato da Codex (6 novembre):** crate isolato e versioni fissate,
verificati con `cargo check --locked`, `cargo run --locked` e `cargo tree -i v8`. La probe
conferma coesistenza Deno/V8 + documento Blitz + style/layout e una chiamata JS→op che muta
testo Blitz. Non verifica ancora oggetti DOM esposti ai giochi né completa T1. PS Blitz 0.6.5
è escluso temporaneamente per il fallimento Windows in `nagoya`.

**Checkpoint del 7 novembre (Claude):** wrapper DOM stabili, `getElementById`, `createElement`,
`createTextNode`, `appendChild` (con spostamento), `removeChild`, `textContent`, attributi,
errori `DOMException`, e clic da hit-test Blitz a listener JS in ordine capture/target/bubble con
`isTrusted`, tramite l'EventTarget di `deno_web`. `Deno`/`__bootstrap` rimossi dal global del
gioco. Verifiche locali Windows e due verifiche negative registrate nel README della probe.
Aggiunti poi: render offscreen con controllo pixel e screenshot artifact, e modalità JIT-less reale in CI
(prima la gamba `jitless` girava con JIT). Restano: shell/finestra, rAF/microtask per frame, input reale, `classList`/`style`, Motion,
ciclo di vita GC dei wrapper (Blitz riusa gli id dei nodi eliminati).

**Sottofase precedente (storica):** sostituire l'op fixture-specifico con metodi DOM usabili dagli script;
valutare librerie mantenute per identità dei wrapper V8↔DOM e poi aggiungere eventi/listener.
Non importare Boa.

Obiettivo funzionale, su una fixture HTML locale:

1. `blitz-html` analizza la fixture in un documento `blitz-dom`; `blitz-shell` la presenta.
2. Script classico eseguito dal `JsRuntime` crea e modifica nodi (`createElement`,
   `appendChild`, `removeChild`, `textContent`, `setAttribute`, `classList`, `style`).
3. Il layout CSS (flex, testo, colori, bordi) riflette le mutazioni al frame successivo.
4. Click: hit-test Blitz → dispatch al listener JS → mutazione visibile.
5. `requestAnimationFrame` guidato dal frame della shell; microtask e timer drenati dal loop
   `deno_core` una volta per frame, senza rientro mentre il runtime è preso in prestito.
6. Se fattibile: una animazione Motion (versione pinnata, transform + opacity) su un elemento;
   altrimenti registrare quali API mancano (`element.animate`, `getAnimations`, CSS transform).
7. Ogni metodo DOM/Web non implementato chiamato dagli script viene registrato (ad es. con
   `Proxy` di diagnostica in sviluppo), così la lacuna è un elenco, non un crash muto.

Ponte DOM in T1: PS Blitz è escluso per il blocco Windows corrente; usare l'API Rust upstream
ufficiale o una libreria mantenuta compatibile. In ogni caso il binding deve mantenere identità
JS stabile dei nodi, coercizione ECMAScript, custom elements se usati, invalidazione del layout,
flush prima delle query geometriche, richiesta redraw e dispatch/listener/eventi; la facade non
fornisce questi contratti. Documentare quali restano per M1.

### T2 — Canvas WebGL sulla stessa superficie (secondo incarico)

1. Elemento `<canvas>` con `getContext('webgl')` che restituisce un contesto reale.
2. Prima pagina: WebGL grezzo, `clearColor` + `clear` + un triangolo.
3. Seconda pagina: PixiJS (versione usata da Pixi'VN `^1.9.8`, da risolvere dal lock del template)
   con renderer WebGL forzato: clear e un triangolo/sprite.
4. La texture del canvas è composta nella scena Blitz **senza readback CPU per frame**:
   nessun `glReadPixels`, `map_async` o copia su buffer host nel percorso di presentazione.
5. Registrare il candidato scelto fra (1)/(2)/(3) di §4 e il backend wgpu effettivo.

### Verifiche funzionali (automatiche, CI)

- La probe accetta `--fixture <file>` e `--exit-after-frames N`; stampa righe
  `ROVES_PROBE_RESULT {json}` con esito di ogni controllo ed esce con codice ≠ 0 se uno fallisce.
- Click sintetico inviato **tramite l'API eventi di Blitz** (non input OS), dichiarato come tale.
- Screenshot del frame (renderer offscreen) e controllo del colore di pixel noti: sfondo, testo
  mutato, centro del triangolo. Il controllo pixel su CI non prova la presentazione su GPU reale.
- Su runner senza GPU registrare adapter e backend (WARP/software); la prova "senza readback"
  su GPU reale e la verifica visiva in finestra restano **da eseguire** quando Windows è
  sbloccato. Non dichiararle fatte prima.

### Strumentazione (nessuna soglia inventata)

- Dimensione dell'eseguibile release (stripped e non) e delle DLL accompagnatorie.
- Timestamp monotoni: avvio processo, V8 pronto, HTML analizzato, primo layout, primo frame
  presentato, primo evento JS gestito.
- RSS/private bytes a riposo e dopo T2 (`GetProcessMemoryInfo` o crate mantenuto tipo `sysinfo`).
- Tempi dei frame lato CPU (inizio frame → `present`) in CSV, con nome adapter e backend.
- Tempo di build pulita in CI. Confronto con Roves v0.7.16 e Chrome solo nelle condizioni di
  `docs/ROVES_PERFORMANCE.md`; la fluidità resta del flusso performance separato.

## 6. Matrice di compatibilità sul template

**Riferimento:** commit `bb9ea77910dda74aced20b001131c8daa22e7cfc` (4 novembre 2026), checkout
locale `D:/roves-template-bb9ea779` [CONFERMATO `docs/ROVES_PERFORMANCE.md:68-75`]. Prima di M1:
registrare il remote del checkout, controllare `main` corrente (`git ls-remote <remote>
refs/heads/main`) e, se è cambiato, annotare il diff di `package.json`/lock e decidere se
aggiornare il riferimento. Dipendenze note: Pixi'VN `^1.9.8`, PixiJS via Pixi'VN,
`markdown-typewriter` 2.2.0, Motion 12.38.0 transitivo [CONFERMATO stessa fonte].

| Caso | Passo/procedura | Chrome stable | Roves v0.7.16 | Blitz probe |
| --- | --- | --- | --- | --- |
| Boot e menu | avvio, menu visibile, voci cliccabili | | menu OK [CONFERMATO :65-76] | |
| Narrativa | nuova partita → prime N battute, avanzamento clic/tasto | | prima battuta OK | |
| markdown-typewriter | battuta lunga, skip | | `getAnimations` mancante [CONFERMATO] | |
| Motion (UI React) | apertura/chiusura menu e dialoghi, transform/opacity | | | |
| `getAnimations` | chiamata dal viewport del dialogo | presente | assente | |
| Pixi'VN/PixiJS | sfondo, sprite, `animations.label.ts`, `start.label.ts` | | | |
| Storage/save | salva, chiudi, riapri, carica; salvataggi preesistenti | | | |
| Audio | musica, effetto, cambio traccia | | | |
| Routing/History | route interne, back, blocco reload/navigazione completa | | | |
| Asset | immagini grandi, font, JSON, moduli, `game://`, CSP | | | |
| Resize/DPI | ridimensiona, cambia DPR, fullscreen | | | |
| Input | mouse, tastiera, focus, IME, gamepad se usato | | | |
| Console/errori | errori JS, rejection, overlay | | | |

Condizioni: stesso hardware e OS, stessa build web, stessa viewport/DPR, stesso salvataggio,
ordine alternato, finestra visibile (protocollo `docs/ROVES_PERFORMANCE.md` §§ 17–36). Chrome
tramite Chrome DevTools MCP quando disponibile, dichiarandolo; altrimenti indicare lo strumento.
Ogni cella: esito, screenshot/log, primo punto di divergenza. Celle vuote = da eseguire.

## 7. Gate, criteri di stop e checkpoint reversibili

| Gate | Evidenza richiesta | Fail-fast |
| --- | --- | --- |
| G0 governance | versioni pinnate, licenze dai `Cargo.toml` scaricati, data ultima release, URL letti | licenza incompatibile con Steam/store → stop e decisione utente |
| G1 slice T1 | verifiche §5 T1 verdi in CI Windows; misure registrate | mutazioni non visibili o rientro JS non evitabile senza secondo runtime → report con opzioni |
| G2 canvas T2 | WebGL grezzo e PixiJS su superficie condivisa senza readback | se nessuno dei tre candidati riesce entro T2 e un incarico di correzione: decisione fra contributo upstream, WebGPU-only, stop |
| G3 matrice | §6 compilata per Chrome/Roves/probe; elenco API mancanti con scelta libreria/codice | API bloccanti (WebGL, Web Audio, storage) senza percorso → stop |
| G4 template | partita del template fino a un salvataggio ricaricato nella probe su Windows | |
| G5 sicurezza | CSP, origini, `game://`, documento unico, link esterni: test negativi/positivi equivalenti agli attuali | qualsiasi regressione di sicurezza blocca la Fase P |
| G6 piattaforme | Linux e macOS in CI (macOS sui tag `v*.*.*` come da AGENTS, o `workflow_dispatch` del probe), poi Android, iOS | piattaforma non raggiungibile → decisione esplicita, non rilascio parziale silenzioso |
| G7 costi | binario, RSS, avvio, build CI, frame pacing a confronto con Roves | soglie fissate dall'utente; nessuna soglia inventata qui |
| G8 produzione | branch `blitz-migration` con parità G3–G7; JITless verde | |

- **Checkpoint:** un commit locale (Codex) per incarico completato; branch `blitz-probe` per la
  fase S/M, `blitz-migration` per la fase P; `main` resta rilasciabile con Servo finché G8 non
  passa. Non usare tag `v*.*.*` per checkpoint della probe (avviano la release).
- **Conservazione:** log CI come artifact, risultati `ROVES_PROBE_RESULT` e misure in un file
  di evidenze sotto `docs/` per ogni gate; branch e run non vanno cancellati a gate fallito.
- **Nessuna dichiarazione di rilascio, parità o prestazioni** senza le evidenze del gate.
- **Manutenzione:** dipendenze con versioni esatte; nessun fork salvo l'eccezione AGENTS per
  difetti bloccanti (riproduzione, issue/PR upstream, TODO di rimozione).

## 8. Primo incarico Claude (T1) — prompt operativo

**Obiettivo originale:** creare `support/blitz-v8-probe` e raggiungere i criteri di §5 T1 con
verifiche automatiche in CI Windows. Due tentativi Claude non hanno prodotto file (accesso
web/Cargo bloccato, poi diniego di scrittura). Codex ha proseguito in autonomia: T1a e una prova
JS→op→mutazione documento sono verdi in locale; il workflow Windows è pronto, non ancora
eseguito in CI.

**Prima di scrivere codice:** usa i fatti già verificati in §0 e §4. Non tentare accesso web
né comandi Cargo nell'ambiente Claude (sono stati bloccati); non inventare versioni o API.
Annota come [DA VERIFICARE DA CODEX] ogni informazione dei sorgenti Cargo che non puoi
leggere. Deno_core/V8 resta l'unico runtime JS: Blitz gestisce DOM/CSS/layout/rendering e
shell; non aggiungere crate Boa, incluso `ps-blitz-script`.

**File inizialmente consentiti:** solo `support/blitz-v8-probe/**`. In seguito Codex ha integrato
`.github/workflows/blitz-probe.yml`, `CUSTOMIZATIONS.md` e
`patches/blitz-probe/0463-blitz-v8-probe.patch` dopo verifica locale. Nessun file di produzione
Servo o runtime principale è stato modificato.

**Vietato:** modificare workspace principale, `Cargo.lock` di produzione, `components/`,
`ports/`, `AGENTS.md`, `TODO.md`, handoff esistenti, file già sporchi; esporre `Deno.*`; creare un
secondo `JsRuntime`; build locali pesanti del browser (la probe si compila in CI; build locale
della sola probe solo se Codex lo concorda); input alla schermata di accesso Windows.

**Consegna:** diff, design e file creati, elenco preciso delle parti compilabili/non verificate,
API mancanti emerse, fatti/ignoti separati. Poi **fermarsi**: niente commit, push o nuovi
incarichi. Codex completa lockfile, workflow e controlli Cargo/CI.

## 9. Ruoli

- **Claude:** implementa un incarico circoscritto alla volta (T1, poi T2, poi gli incarichi M),
  esegue le verifiche concordate, consegna evidenze e si ferma; niente commit, push o avvio
  autonomo di nuovi incarichi.
- **Codex:** aggiorna `AGENTS.md`/`TODO.md` con la nuova direzione, revisiona diff ed evidenze,
  delega le correzioni, esegue le verifiche concordate, gestisce commit, push, CI e decisioni
  di gate secondo la procedura AGENTS; README e wiki si aggiornano quando una modifica diventa
  visibile agli utenti (Fase P), non per la probe isolata.

## 10. Prossimo passo concreto

1. Codex registra la direzione in `AGENTS.md`/`TODO.md` (completato).
2. Claude ha tentato T1 due volte senza produrre file; Codex ha costruito e verificato in locale
   la probe Windows e il primo op DOM. Il grafo scelto è Blitz DioxusLabs senza Boa.
3. Completato il 7 novembre: wrapper Node stabili, API DOM di base ed eventi (vedi T1). Prossimo
   passo: shell/finestra Blitz, frame loop con rAF e microtask, input reale; poi `classList`/`style`
   e Motion. Workflow non pubblicato e CI pendente. Non avviare
   T2 WebGL o misure di performance finché non c'è una pagina interattiva.
