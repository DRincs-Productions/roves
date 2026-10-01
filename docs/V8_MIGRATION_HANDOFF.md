# V8 migration — handoff (2026-10-01)

Questo file esiste perché la sessione precedente ha esaurito il budget di contesto a metà
lavoro. Leggilo per intero prima di continuare: dice esattamente cosa è stato fatto, cosa
l'utente ha deciso, e qual è la prossima decisione in sospeso.

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
4. **[`../patches/servo-v0.5.0/`](../patches/servo-v0.5.0/)** — patch numerate `0057` → `0068`,
   una per checkpoint, applicabili a un checkout pulito del tag Servo pristine (vedi
   `../AGENTS.md` per come funziona il meccanismo patch/vendoring di questo repo).
5. **[`../components/roves-v8/src/lib.rs`](../components/roves-v8/src/lib.rs)** — il codice
   vero, un unico file, ~2000 righe, con commenti doc estesi su ogni funzione pubblica e su ogni
   bug reale trovato. **Non è pseudocodice o un prototipo giocattolo scadente** — è scritto con
   la stessa cura che si userebbe in produzione, solo isolato in una crate separata per
   sicurezza. 41 unit test, tutti verificati sia in locale (`cargo test -p roves-v8`) sia in CI.

## Stato aggiornato — 2026-10-01

Le sezioni storiche qui sotto documentano le decisioni e i checkpoint precedenti; se divergono
da questo blocco, fa fede lo stato attuale. L'utente ha richiesto autonomia continuativa: modificare,
testare localmente, avviare CI, correggere i fallimenti e proseguire fino al completamento dei
criteri in `V8_MIGRATION.md`.

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
ancora in esecuzione. Il workflow V8 multipiattaforma sarà riattivato dal checkpoint 0074.

`codegen.py` è stato esteso solo con la classe opt-in `CGV8BindingRoot`; il percorso SpiderMonkey
predefinito è rimasto senza modifiche di output (1.441 file generati confrontati prima dell'ultimo
controllo di esposizione). Il pilot accetta solo interfacce esposte a Window, attributi di istanza
readonly boolean/`double`/`unsigned long`/`DOMString` senza attributi estesi, e senza
ereditarietà/costruttore. Tutte le altre forme falliscono esplicitamente. Patch overlay: 0071
aggiunge la prima interfaccia, 0072 aggiunge i numerici, 0073 collega l'adapter nativo Screen,
0074 aggiunge le stringhe UTF-16 lossless.
Il file `CLAUDE.md` è stato rinominato in `AGENTS.md`.

Prossimo lavoro autonomo: finalizzare 0074, verificare l'overlay dal checkpoint 0073, committare e
pushare. Attendere la CI V8, la validazione patch e la CI completa; correggere ogni errore e poi
proseguire con conversioni e forme WebIDL, wrapper/realm e runtime production. La migrazione resta
in corso finché tutte le fasi e la validazione giochi in `V8_MIGRATION.md` non sono concluse.

## Stato reale, in breve

- **Fase 1 (isolate/context/eval)**: completa.
- **Fase 2 (conversione valori, Promise, moduli ES)**: completa.
- **Fase 3 (ownership/GC)**: completa — `create_wrapped`/`get_wrapped` con finalizzatore
  garantito, sicurezza sui cicli di riferimento dimostrata con test reali di stress GC (fino a
  200 oggetti).
- **Fase 4 (binding WebIDL/DOM)**: **5 checkpoint fatti**, tutti dentro `components/roves-v8`
  come esperimenti temporanei — esposizione oggetto globale (`window`), identità di interfaccia
  (`instanceof`, ereditarietà via `FunctionTemplate::inherit`), proprietà in lettura, proprietà in
  scrittura, metodi chiamabili. **Zero righe toccate in `components/script`/
  `components/script_bindings`/`codegen.py` finora.**

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
