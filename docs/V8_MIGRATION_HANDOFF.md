# V8 migration - handoff (2026-10-03)

Questo file è il punto di ripresa operativo della migrazione V8. Lo stato corrente riportato
qui prevale sulle note storiche più sotto.

## Documenti da leggere, in ordine

1. **[`docs/V8_MIGRATION.md`](./V8_MIGRATION.md)** — il piano architetturale completo (fasi 0-7,
   decisioni già prese, regola dura "nessun tipo `v8::*` deve uscire dal confine di scripting").
   Ha una "Status note" aggiornata in fondo a ogni sezione di fase con lo stato reale verificato.
   **Leggi questo per primo, per intero.**
2. **[`docs/V8_MIGRATION_PHASE0_INVENTORY.md`](./V8_MIGRATION_PHASE0_INVENTORY.md)** — inventario
   reale (non ipotizzato) di tutti gli usi di SpiderMonkey in `components/script`/
   `components/script_bindings`: conteggi per crate, file di rooting/GC chiave, dove vive il
   generatore di binding.
3. **[`../CUSTOMIZATIONS.md`](../CUSTOMIZATIONS.md)** — changelog prosa di *ogni* modifica fatta,
   in ordine cronologico inverso (la più recente in cima). Le entry dal 2026-09-29 in poi (cerca
   "V8 migration") coprono tutto il lavoro di questa migrazione, checkpoint per checkpoint, con i
   bug reali trovati e come sono stati corretti.
4. **[`../patches/servo-v0.5.0/`](../patches/servo-v0.5.0/)** — patch numerate `0001` → `0093`,
   una per checkpoint, applicabili a un checkout pulito del tag Servo pristine (vedi
   `../AGENTS.md` per come funziona il meccanismo patch/vendoring di questo repo).
5. **[`../components/roves-v8/src/lib.rs`](../components/roves-v8/src/lib.rs)** — runtime isolato
   con API engine-neutral e i test del pilot; verificato localmente in modalità normale e JIT-less.

## Stato aggiornato - 2026-10-03

Le sezioni storiche qui sotto documentano le decisioni e i checkpoint precedenti; se divergono
da questo blocco, fa fede lo stato attuale. L'utente ha richiesto autonomia continuativa: modificare,
testare localmente, avviare CI, correggere i fallimenti e proseguire fino al completamento dei
criteri in `V8_MIGRATION.md`.

CP26 (`b03e711f304`) è completo e verde. Verifiche locali: 21 test Python; pilot normale e JIT-less ciascuno con 59 unit + 3 integrazioni + 2 doctest; default 52 unit + 2 doctest; check integrato `servo-script`. Patch 0090 applica sulla serie pristine. CI verdi: V8 ([37144196093](https://github.com/DRincs-Productions/roves/actions/runs/37144196093)), Servo (patch validation, SDL3, Steam e tutti i bundle: [37144196126](https://github.com/DRincs-Productions/roves/actions/runs/37144196126)), Android ([37144196111](https://github.com/DRincs-Productions/roves/actions/runs/37144196111)), iOS ([37144196107](https://github.com/DRincs-Productions/roves/actions/runs/37144196107)). Wiki aggiornata e build Next.js verde al commit `92ac3ac`.

CP27 (`19bb9c8cd2e`) è completo e verde. ByteString operation arguments mappati a `Vec<u8>`; ToString valida ogni code unit <=255, per le forme required, nullable e optional. Verifiche locali: 22 test generatori; pilot normale/JIT-less ciascuno con 60 unit + 3 integrazioni + 2 doctest; default 52 unit + 2 doctest; check `servo-script`. Patch 0091 applica alla snapshot CP26 e al pristine nella CI. CI verdi: V8 ([37147151024](https://github.com/DRincs-Productions/roves/actions/runs/37147151024)), Servo (patch validation, SDL3, Steam e tutti i bundle: [37147151005](https://github.com/DRincs-Productions/roves/actions/runs/37147151005)), Android ([37147150994](https://github.com/DRincs-Productions/roves/actions/runs/37147150994)), iOS ([37147151023](https://github.com/DRincs-Productions/roves/actions/runs/37147151023)). Wiki build Next.js verde al commit `d36c050`.

CP28 (36da94054a7) complete and green: optional boolean/numeric defaults, including float and nullable types. Typed IDLValue/IDLNullValue; tests verify false/zero and infinity handling. String and non-primitive defaults still fail closed. Local: 23 Python tests; pilot normal/JIT-less each 61 unit + 3 integration + 2 doctests; default 52 unit + 2 doctests; integrated servo-script check; patch 0092 applies to CP27; wiki build 82 pages. CI green: V8 37151335289, Servo 37151335249 (six bundles, Steam, SDL3, patch validation), Android 37151335284, iOS 37151335248. Wiki commit 56203c4.
CP29 in progress: optional WebIDL string defaults for DOMString (UTF-16 vector), USVString (escaped scalar-valid Rust string) and ASCII ByteString (byte vector). Runtime covers literal backslash, supplementary Unicode, null and explicit input. Local green: 24 generator tests; pilot normal/JIT-less each 62 unit + 3 integration + 2 doctests; default 52 unit + 2 doctests; integrated servo-script check. Patch 0093 applies to the CP28 snapshot. Wiki build generates 82 pages; commit 462ed36 is published. CI V8/Servo/Android/iOS still pending.

Il checkpoint 25 è pubblicato con commit `6525db014e0`; patch 0089 applica pulitamente. Aggiunge argomenti required nullable per boolean, tipi numerici, `DOMString` e `USVString`: `null`/`undefined` diventano IDL null (`None`), altrimenti si applica la conversione del tipo interno. Verifiche locali complete: 20 test Python; pilot normale/JIT-less ciascuno con 58 unit + 3 integrazioni + 2 doctest; default 52 unit + 2 doctest; check integrato `servo-script`. CI verde: V8 ([37141033528](https://github.com/DRincs-Productions/roves/actions/runs/37141033528)), Servo con sei bundle/Steam/SDL3/patch validation ([37141033571](https://github.com/DRincs-Productions/roves/actions/runs/37141033571)), Android ([37141033507](https://github.com/DRincs-Productions/roves/actions/runs/37141033507)), iOS ([37141033514](https://github.com/DRincs-Productions/roves/actions/runs/37141033514)). Produzione resta SpiderMonkey.

Il checkpoint 24 e pubblicato con commit `02b695f74d0`; patch 0088 applica pulitamente. Aggiunge argomenti obbligatori DOMString/USVString con preservazione UTF-16 o scalar-value, coercizione ToString e propagazione degli errori. Verifiche locali complete: 19 test Python; pilot normale/JIT-less ciascuno con 57 unit + 3 integrazioni + 2 doctest; default 52 unit + 2 doctest; check integrato `servo-script`. Tutte le CI sono verdi: V8 6/6 ([37137152487](https://github.com/DRincs-Productions/roves/actions/runs/37137152487)), Servo bundle/test/Steam/patch validation ([37137152516](https://github.com/DRincs-Productions/roves/actions/runs/37137152516)), Android ([37137152568](https://github.com/DRincs-Productions/roves/actions/runs/37137152568)), iOS ([37137152538](https://github.com/DRincs-Productions/roves/actions/runs/37137152538)). Produzione ancora SpiderMonkey.

Il checkpoint 23 e stato pubblicato con commit `09ffe10fdcf`; patch 0087 e la CI sono verdi. Aggiunge conversioni degli argomenti numerici WebIDL interi e `float`/`unrestricted float`. Verifica locale: 18 test Python, pilot normale/JIT-less, default e cargo check integrato; run V8 37132532003, Servo 37132531959, Android 37132531996, iOS 37132532177.

Il checkpoint 22 e pubblicato con commit `323649b84f7` e patch 0086; aggiunge argomenti richiesti `double`, `unrestricted double` e `unsigned long`. Omissione e conversioni da Symbol falliscono prima del callback. Passano 18 test Python, runtime normale/JIT-less/default e check integrato; CI verde: V8 37117775121, Servo 37117775170, Android 37117775250, iOS 37117775238. Produzione resta SpiderMonkey.

Verifica CP23: `cargo test -p roves-v8 --features webidl-pilot` e la variante `webidl-pilot,jitless` passano con 56 unit + 3 integrazioni + 2 doctest ciascuna; il default passa con 52 unit + 2 doctest. `AWS_LC_SYS_NO_ASM=1 cargo check -p servo-script --features v8-bindings-pilot,js_jit --offline --locked` passa. Patch 0087 si applica pulitamente.
Il checkpoint 21 corregge la distinzione tra tipi floating-point WebIDL ristretti (`float`/`double`, nativi `FiniteF32/F64`) e `unrestricted float/double`, che conservano NaN e infinito. I tipi finiti non possono essere costruiti con valori non finiti; gli attributi mutabili `double` ora generano TypeError prima di cambiare stato se ToNumber produce NaN o infinito. L-adapter Screen usa il contratto finito sotto feature pilot. Passano 16 test Python, runtime normale/JIT-less/default e `cargo check` integrato di `servo-script`; patch 0085 e CI verdi: V8 6/6 (37108919478), sei bundle Servo con Steam e patch validation (37108919488), Android (37108919483), iOS (37108919477). Wiki non aggiornabile perché checkout `roves-wiki` assente.

Il checkpoint 20 estende i ritorni numerici delle operazioni a `byte`, `octet`, `short`, `unsigned short`, `long`, `long long`, `unsigned long long` e `float`, inclusi `float?` e `long long?`. I numeri WebIDL si mappano al tipo Rust corrispondente e a JavaScript Number; `Option<T>::None` diventa `null`. Passano 16 test Python, 56 unit + 3 integrazioni + 2 doctest in pilot normale e JIT-less, 52 unit + 2 doctest default e `cargo check` integrato. Patch 0084 si applica pulitamente. CI verde: V8 6/6 (37061193384), sei bundle Servo più Steam e patch validation (37061193356), Android (37061193392), iOS (37061193362). Il checkout `roves-wiki` manca, quindi non è stato possibile aggiornare la wiki.

Il checkpoint 18 supporta i ritorni `DOMString`, `DOMString?`, `USVString` e `USVString?` per le operazioni senza argomenti. `DOMString` conserva i code unit UTF-16, `USVString` usa stringhe Unicode scalari, e i risultati nullable preservano `null`. Passano 14 test Python, i runtime normali/jitless, il default e il `cargo check` integrato. Patch 0082 e CI sono verdi: matrice V8 6/6 (37041208311), sei bundle Servo con Steam e patch validation (37041208274), Android (37041208279), iOS (37041208300).

Il checkpoint 17 aggiunge parametri WebIDL obbligatori di tipo `boolean` alle operazioni a firma singola supportate. La truthiness JavaScript segue la conversione IDL; argomenti omessi diventano `false`, gli argomenti aggiuntivi sono ignorati. Parametri opzionali, nullable, variadici e altri tipi restano rifiutati. Passano 13 test Python, i runtime pilot normale/jitless, il runtime default e il `cargo check` integrato. Patch 0081 e CI sono verdi: matrice V8 6/6 (37036084532), sei bundle Servo con Steam e patch validation (37036084693), Android (37036084595), iOS (37036084598).

Il checkpoint 16 aggiunge metodi WebIDL senza argomenti e senza overload, con ritorni `undefined`, `boolean`, `double` e `unsigned long`. Il generatore usa i callback nativi gi? presenti e mantiene il controllo del receiver. Overload, parametri e tipi di ritorno non ancora supportati falliscono esplicitamente. Passano localmente 11 test Python, i runtime normali/jitless e il `cargo check` integrato dei binding. Patch 0080 e CI sono verdi: V8 37029579196, Servo 37029580090, Android 37029579200, iOS 37029579293.

Il checkpoint 15 aggiunge attributi WebIDL nullable `boolean?`, `double?` e `unsigned long?`, con `Option<T>` sul lato Rust, conversione diretta di `null` e le normali regole WebIDL per gli altri valori. La conversione precede la mutazione nativa; errori da Symbol preservano lo stato. Passano localmente 10 test Python, 56 unit test + 3 integrazioni + 2 doctest in pilot normale e jitless, 52 unit + 2 doctest senza pilot, e il `cargo check` del crate `servo-script-bindings` con pilot e `js/jit`. Patch 0079 e CI sono verdi: overlay e sei bundle, Steam inclusi (37024609945), matrice V8 6/6 (37024610041), Android (37024609737), iOS (37024609529).

Il checkpoint 14 aggiunge `USVString` e `USVString?` al generatore opt-in: i valori diventano `String`/`Option<String>` dopo la conversione scalar-value (i surrogate isolati sono sostituiti da U+FFFD); `null` nullable resta `null`. Passano localmente 56 unit test, 3 integrazioni e 2 doctest con pilot normale e jitless; 52 unit test + 2 doctest senza pilot; 9 test Python. La patch 0078 e la CI sono verdi: matrice V8 6/6 (run 37016952782), Servo completo con sei bundle, Steam e overlay (37016951496), Android (37016951470), iOS (37016951542). La produzione resta SpiderMonkey.

Il checkpoint 13 aggiunge `DOMString?` mutabile, preservando i code unit UTF-16 e `null`. Patch 0077 e CI sono verdi: run Servo 37009800175, Android 37009800200 e iOS 37009800118. La matrice V8 6/6 del commit sorgente 0077 era verde nel run 37004723223. Il test Steam attende ora il marker della pagina invece di un ritardo fisso.

Il checkpoint 12 aggiunge setter mutabili `boolean`, `double` e `unsigned long`; il checkpoint 11
aggiunge setter `DOMString`. Sul commit `de49cb9`, patch validation, Android, iOS e i bundle
salvo Linux deb sono passati; Linux portable ha completato il bootstrap, mentre Linux deb e Steam
hanno raggiunto il timeout di 45 minuti scaricando dipendenze apt. Il mirror Azure non ha risolto
il problema. Il commit `dbc6b9d` aumenta a 90 minuti i timeout del bootstrap Linux; la run Servo
37000099322 e ancora in corso, mentre iOS e Android sono passati.

Il README dichiara gia che le release usano ancora SpiderMonkey e che `roves-v8` e un runtime
isolato in fase di validazione. Il checkout adiacente `roves-wiki` non e presente, quindi la
wiki non e stata aggiornata. `roves-action` non e presente; questa modifica CI non cambia flag,
default o convenzioni di bundle dell'action.

Il checkpoint 7 avvia l'integrazione vera con `components/script_bindings`: un backend opt-in
usa il parser WebIDL esistente e genera `ValidityState` dalla definizione Servo reale, senza
toccare l'output predefinito del generatore SpiderMonkey. Il checkpoint 8 aggiunge anche
`Screen.webidl`, con tipi `double` e `unsigned long`. `v8-bindings-pilot` compila le binding
generate come percorso aggiuntivo; non seleziona V8 in produzione. Il checkpoint 9 collega la
binding `Screen` al tipo DOM nativo Servo con un adapter opt-in; non installa ancora l'oggetto in
un realm V8 e non sostituisce il reflector SpiderMonkey. Il checkpoint 10 aggiunge conversione
UTF-16 lossless e la binding WebIDL `DOMString`, compresi i code unit di surrogate isolate. Sono
passati localmente sei test Python; 53 unit test runtime, tre integrazioni WebIDL e due doctest
sia in modalità normale sia JIT-less; e 52 unit test più due doctest senza feature pilot. Build
Windows complete di `servo-script-bindings` e `servo-script` con i feature pilot erano riuscite
nei checkpoint precedenti. Per compilare `servo-script` è servito `AWS_LC_SYS_NO_ASM=1` perché
NASM non è installato localmente. La matrice V8 6/6 e l'overlay fino a 0072 sono verdi. Per il
commit e521bf0, `validate-servo-patches` (inclusa 0073) è passato; il workflow completo Servo è
ancora in esecuzione. La matrice V8, Android e iOS del checkpoint precedente è verde. Il job Steam della CI Servo per 31c2ae6 ? scaduto due volte nel bootstrap apt: il mirror standard scaricava lentamente e ha installato solo 272 pacchetti in 45 minuti. Il job ora usa il mirror Azure, che ha installato circa 500 pacchetti in 18 minuti in un altro job Linux, pi? la cache uv e il timeout di 45 minuti.

`codegen.py` è stato esteso solo con la classe opt-in `CGV8BindingRoot`; il percorso SpiderMonkey
predefinito è rimasto senza modifiche di output (1.441 file generati confrontati prima dell'ultimo
controllo di esposizione). Il pilot accetta solo interfacce esposte a Window, attributi di istanza
readonly boolean/`double`/`unsigned long`/`DOMString` senza attributi estesi, e senza
ereditarietà/costruttore. Tutte le altre forme falliscono esplicitamente. Patch overlay: 0071
aggiunge la prima interfaccia, 0072 aggiunge i numerici, 0073 collega l'adapter nativo Screen,
0074 aggiunge le stringhe UTF-16 lossless.
Il file `CLAUDE.md` è stato rinominato in `AGENTS.md`, entry point condiviso per tutti gli agenti.

Checkpoint 10 (`0074`, commit `9412dfe`) e checkpoint 11 (mutabili DOMString, in sviluppo locale)
sono dettagliati sopra e nel piano. La CI del commit 9412dfe è verde per matrice V8 (6/6), Android,
iOS e validazione overlay; i bundle macOS hanno mostrato che serve aggiungere Homebrew `ld.lld`,
ma lasciare il clang Apple selezionato: Homebrew `clang++` causa un panic in bindgen/mozangle.
Il workflow ora espone solo il binario della formula `lld`. Dopo la
patch 0075, proseguire con le conversioni WebIDL e poi wrapper/realm solo quando l'ownership è
validata. Il pilot non è il runtime di produzione; il piano resta aperto fino a tutti i criteri di
`V8_MIGRATION.md`.

## Stato reale, in breve

- **Fase 1 (isolate/context/eval)**: completa.
- **Fase 2 (conversione valori, Promise, moduli ES)**: completa.
- **Fase 3 (ownership/GC)**: completa — `create_wrapped`/`get_wrapped` con finalizzatore
  garantito, sicurezza sui cicli di riferimento dimostrata con test reali di stress GC (fino a
  200 oggetti).
- **Fase 4 (binding WebIDL/DOM)**: 23 checkpoint del pilot, con generazione WebIDL opt-in in `components/script_bindings/codegen.py`, adapter del DOM `Screen` e runtime verificato in `components/roves-v8`. Copre attributi e operazioni con conversioni primitive e stringhe, metodi e fixture runtime. Nessun percorso di produzione è ancora passato da SpiderMonkey a V8; restano wrapper DOM, interfacce, semantiche WebIDL e integrazione runtime. La fase è aperta.

**Perché tutto è isolato in `roves-v8` e non in produzione:** ispezionando una build reale
(`target/debug/build/servo-script-bindings-*/out/Bindings/`) si sono trovati **522 file di
binding generati**, ~700 righe anche per il più semplice (`ConsoleBinding.rs`). I moduli di
supporto scritti a mano che quei binding usano (`components/script_bindings/interface.rs`,
`proxyhandler.rs`, `finalize.rs`) sono **mutuamente interdipendenti** attorno al modello a
oggetti `JSClass` di SpiderMonkey — non si può sostituirne uno solo in isolamento (es. un
finalizzatore V8 non ha senso se l'oggetto è ancora creato con `JSClass`). Per questo si è
scelto di costruire e validare i primitivi di ownership (create_wrapped/finalizzazione
garantita/identità di interfaccia/proprietà/metodi) dentro una crate isolata **prima** di
toccare quei file di produzione — l'utente ha confermato esplicitamente questa scelta durante la
sessione.

## Bug reali trovati durante il lavoro (utile saperli prima di scrivere altro codice V8)

Tutti documentati per esteso in `CUSTOMIZATIONS.md` e nei commenti doc di `lib.rs`, qui solo
l'elenco rapido:

1. Le scope di questa versione della crate `v8` sono `!Unpin` e richiedono le macro
   `v8::scope!`/`v8::tc_scope!`, non `HandleScope::new`/`TryCatch::new` diretti.
2. Un solo isolate V8 può essere "entered" per thread alla volta (serve `Locker` per condividerne
   uno tra thread).
3. Il `Runtime` deve avere **un contesto persistente condiviso**, non uno nuovo per chiamata —
   altrimenti dati registrati in una chiamata (es. una funzione nativa) sono invisibili a una
   chiamata successiva.
4. Il tag per `set_aligned_pointer_in_internal_field` deve essere un valore piccolo (0 funziona,
   un valore arbitrario come `0xC0DE` fa abortire l'intero processo).
5. `FunctionCallbackArguments::data()` restituisce un `Local<Value>` diretto, non un `Option`.
6. **`PropertyCallbackArguments` non ha modo di recuperare il vero destinatario (`this`)** — solo
   `.holder()`, che per un accessor su un prototipo condiviso restituisce il prototipo stesso, non
   l'istanza. Ogni lettura tornava silenziosamente `Undefined`. Fix: installare l'accessor
   sull'**instance template**, non sul prototype template.
7. Sorpresa positiva: nonostante il fix sopra, `FunctionTemplate::inherit` propaga comunque gli
   accessor su instance template lungo la gerarchia — l'ereditarietà funziona gratis.
8. **`get_function()` materializza il prototipo *subito*, in modo permanente.** Chiamarlo
   dentro `define_interface` prima che tutte le `define_method`/`define_property` fossero
   registrate rompeva silenziosamente i metodi aggiunti dopo ("X is not a function"). Fix:
   l'esposizione del costruttore globale è rimandata alla prima `create_instance` per quella
   interfaccia.
9. Un valore `u16` arbitrario come tag di campo interno fa abortire V8 (vedi punto 4) — stesso
   bug categoria, voce separata perché è stato il primo trovato.

## Decisione in sospeso per la prossima sessione

L'utente ha chiesto esplicitamente, a fine sessione precedente: **"scegli tu se codegen.py o
continuare a prototipare in roves-v8 su uno specifico elemento della lista"** — questa scelta
non è stata ancora presa per mancanza di budget, non per indecisione tecnica. Le due opzioni
reali, con il relativo rischio:

- **Continuare a prototipare in `roves-v8`** (basso rischio, stesso pattern di tutta la
  sessione): il prossimo elemento naturale dalla lista di validazione del piano è un grafo di
  oggetti DOM minimale (`document.createElement` → `Element` con `appendChild`/`parentNode`,
  usando `link` già esistente per formare il grafo), oppure l'intercettazione di proprietà
  indicizzate/nominate (l'equivalente di `proxyhandler.rs`, mai prototipato).
- **Iniziare davvero su `codegen.py`/`components/script_bindings`** (rischio alto — tocca codice
  di produzione usato dalla build SpiderMonkey attuale). Il punto di partenza più sensato, visto
  quanto già costruito: scegliere **una singola interfaccia WebIDL reale e semplicissima** (es.
  `Console`, già ispezionata) e riscrivere a mano il suo `interface.rs`/`finalize.rs` equivalente
  in stile V8 usando esattamente i primitivi già validati in `roves-v8` (stesso pattern
  internal-field + `Weak::with_guaranteed_finalizer` + `FunctionTemplate`), **dietro un feature
  flag Cargo spento di default**, così la build di produzione corrente non viene toccata finché
  non si decide di attivarlo. Verificare con una build CI completa (non solo `cargo check`) che
  il percorso SpiderMonkey esistente resti intatto.

**Raccomandazione di chi scrive:** opzione 2 (toccare `codegen.py`/`script_bindings` per
un'interfaccia reale, dietro feature flag) è il passo che l'utente ha già approvato in linea di
principio ("si ti tocca modificare codegen.py... prendi la stessa decisione" per decisioni
simili) — ma è un salto di rischio reale, va fatto con calma, un passo alla volta, verificando la
build di produzione ad ogni modifica, non di fretta per "finire" qualcosa.

## Note operative per chi continua

- **Workflow di verifica**: per ogni modifica a `components/roves-v8/`, genera sempre una patch
  nuova in `patches/servo-v0.5.0/` (vedi `../AGENTS.md`, sezione "keep CUSTOMIZATIONS.md and
  patches up to date") — **anche per file nuovi**, non solo modifiche: è stato un errore reale
  fatto e corretto in questa sessione (vedi CUSTOMIZATIONS.md, entry "V8 migration Phase 1" per
  il dettaglio del fix).
- **Build locale funzionante**: questa macchina ha ora un toolchain Windows completo e verificato
  (LLVM/`lld-link.exe`/`libclang`, MSVC Build Tools, `depot_tools`, GStreamer reale) — vedi la
  memoria `project_roves_local_build_toolchain.md`. `cargo test -p roves-v8` gira in pochi
  secondi; non serve aspettare la CI per iterare.
- **CI**: `test.yml` si riattiva solo toccando `patches/**`/`test-page/**`/se stesso — un commit
  vuoto non basta. L'API GitHub anonima ha un rate limit (60/ora) facilmente superabile se si fa
  polling troppo stretto durante l'attesa della CI — spaziare le richieste.
- **Quirk noto**: lo strumento Edit può silenziosamente convertire un intero file da LF a CRLF
  — verificare con `file <path>` prima di committare (vedi memoria
  `feedback_edit_tool_crlf_quirk.md`).
- **Autonomia**: l'utente ha dato autonomia piena per questo lavoro — procedere senza chiedere
  conferma a ogni passo, fermarsi solo per blocchi reali o decisioni di scala/rischio come quella
  sopra.

## Continuation checkpoint ? 2026-10-01

The pending choice above is resolved: continue isolated prototyping with indexed read
interception, before the production generator jump. Added checkpoint 6 in `roves-v8`, patch
0069, six tests (47 total), passing locally with and without `--features jitless`.
`FunctionTemplate::inherit` does not propagate indexed interceptors: explicitly register on
children. This is not a complete collection binding: query/enumeration/descriptors,
assignment/deletion, named handlers and wrapper-valued results still need implementation.
Registration must precede creation of any interface/descendant instance; only direct late
registration is guarded. Production rooting replacement and wrapper reuse also remain open;
Phase 3 being described as complete above refers only to tested ownership primitives.
See CUSTOMIZATIONS.md and the Phase 4 status note for the precise scope.

## Autonomous continuation - ownership safety

The user requires continued implementation, local testing, CI and fixes until all migration
criteria are met. Shared instructions are now AGENTS.md. Ownership hardening in patch 0070
fixes get_wrapped's runtime lifetime, guards ancestor materialization, owns setter callback
allocations, and sweeps completed finalizers. 49 unit tests and 2 compile-fail doctests.
A separate v8.yml matrix runs normal/JIT-less tests on all desktop platforms. Production
WebIDL migration remains the next implementation step, starting from a small real interface;
do not claim that prototype ownership coverage completes Phase 3 in production.
