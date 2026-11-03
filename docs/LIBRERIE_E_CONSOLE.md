## Runtime V8 + deno_core in the embedded Servo shell — current status

The embedded `servoshell` production target uses V8 through `roves-js` and `roves-v8`, with
`deno_core` owning the isolate/context, single-document module map and module event loop.
The workspace binds Servo's `js` dependency to `roves-js` unconditionally: there is no
runtime `if` that selects SpiderMonkey or another JS engine. `Cargo.lock` resolves one
`v8` package version (`150.4.0`); deno_core is pinned at `0.413.0`. `roves-js` keeps Servo's
JSAPI-compatible surface and DOM bindings. `support/deno-core-probe` is an isolated
historical experiment, not a product fallback. Games receive no Deno namespace or privileged
ops, and no separate Deno/Chromium installation is required.

Static module loading uses the cooperative 0457/0458 poll: fetches and already-ready sources
yield between steps, dynamic imports remain asynchronous, while compiling an individual
source, resolving it and final linking remain synchronous. The user accepted this provisional
tradeoff, pending the separate real-scene responsiveness tests. No latency or performance
claim is established. The migration handoff records current CI, limitations and open work.

# Roves: librerie, GPU e console

Obiettivo a lungo termine: Roves come assemblaggio di librerie mantenute da altri, con solo il
codice necessario a farle lavorare insieme. Questo documento elenca cosa si può togliere o
sostituire, in ordine di rapporto guadagno/rischio.

Le rimozioni e migrazioni completate sono registrate in `CUSTOMIZATIONS.md`.

## Shell Servo e documento unico — decisione aggiornata del 27 ottobre 2026

Roves mantiene la shell Servo e un solo documento di gioco, con route interne tipo TanStack
Router. History API/pushState/replaceState/back/forward e frammenti restano interni. Tab,
finestre aggiuntive e iframe non sono obbligatori; reload e cambi pagina completi non devono
essere accessibili al gioco. Caricamento iniziale shell e chiusura restano necessari.

La policy single-document è implementata nel motore: reload e navigazioni complete sono
bloccati prima di sostituire il documento; le richieste automatiche sono segnalate, i link
esterni usano il browser di sistema solo dopo un clic nativo dell'utente. La prova Windows
ha verificato blocco, avviso in sviluppo e apertura del browser esterno. La chiusura OS della
finestra durante un fetch statico resta da verificare end-to-end. Vedi
[handoff0380](handoff/0380-single-document-runtime.md) e il punto operativo in V8_MIGRATION.md.

DOM, layout e rendering restano in Servo; i servizi di moduli e runtime JavaScript sostituiti
sono gestiti da deno_core. L'adozione è presente nel codice di produzione, ma non equivale a
una release finale né a conformità generale del browser. Le verifiche ancora aperte su CSP,
microtask, GC, sicurezza, scene reali e JITless sono elencate nel handoff. Nessun fallback
SpiderMonkey e nessun secondo grafo runtime vanno reintrodotti.

### Prova0381 isolata (storica; non è il runtime di produzione)

[Handoff0381](handoff/0381-deno-single-document-probe.md): workspace isolato in
support/deno-core-probe, indipendente dal binario di produzione. Dieci scenari logici in due test
Rust, una sola thread per gli scenari V8, stessa suite normale/JITless su Linux/Windows.
Metadata/parse e contratto YAML/lock verificati; CI40539608247 verde4/4, Linux/Windows
normale/JITless. Due testRust perjob coprono dieci scenari logici, non integrazione DOM. RustyV8150 espone il flag globale di contesto per eval/Function, non la callback per
singola stringa di152: gap CSP/TrustedTypes ancora da valutare. Nessun DOM/router simulato,
nessuna prova di integrazione reale o misura prestazionale. Prova bootstrap0382 completata dopo il reset: rimuove globali interni prima del gioco e
riesegue scenari moduli/async con import interni rifiutati, via API pubbliche. Metadata/parse
verificati, nuove asserzioni in attesa CI; non è isolamento generale o integrazione DOM.
Ricontrollo19:02 eliminato. Prossima prova classe nativa/cppgc; mantenere un solo servizio.
Vedere handoff/0382-deno-bootstrap-isolation.md.

### Bootstrap0382 verificato, classe nativa0383 pronta

Bootstrap0382 CI40556912335 verde4/4: moduli/async dopo rimozione globale e import interni
rifiutati. Non dimostra isolamento universale. [Classe0383](handoff/0383-deno-native-class.md)
usa op2/cppgc/SameObject mantenuti per metadati posseduti e durata di vita di oggetti nativi,
senza framework/cache nostro. Metadata/sintassi verificati; nuove asserzioni attendonoCI.
Resta da provare il DOM reale, le conversioni/ereditarietà, la raccolta generale, CSP e il
servizio moduli/async unico. Prossima prova indipendente: completamento/cancellazione opnative.

### Op nativo asincrono0384 pronto per CI

[Prova0384](handoff/0384-deno-native-async.md): FutureRust reale sul driverDeno e Tokio
esistenti, senza un nuovo executor/queue nostro. Contatori verificano avvio effettivo,
input/output posseduti, risultato/errore e cancellazione a shutdown, non solo logJS.
Dodici scenarilogici/due testRust; metadata/sintassi verificati, esecuzione ancoraCIpending.
Classi0383 e async0384 non dimostrano il DOM o il reale loopServo: prossimo studio bounded
del servizio da sostituire e dei checkpointHTML, rimuovendo il vecchio owner nel port.

## Prossimi passi, codice e librerie

| Area | Oggi | Direzione |
| --- | --- | --- |
| WebGPU | Attivo per default (wgpu + naga); thread già avviati su richiesta e poller sospeso quando inattivo | Tenerlo: serve ai giochi. Misurare il lavoro effettivo prima di proporre altre modifiche |
| `mach bundle` (`post_build_commands.py`, ~1,7k righe nostre) | Packaging scritto a mano | Valutare `cargo-packager` per msi/dmg/deb; tenere solo la logica Roves (contenuti, icone, Steam) |
| Tastiera e gamepad | `code` dal crate `keycode`; `key` e gamepad mappati a mano | Restano come glue: usare il più possibile i tipi del crate `sdl3` |
| roves-js (~22k righe) | Emula l'API mozjs sopra V8 | Ridurlo spostando il codegen dei binding verso il crate `v8` diretto (ArrayBuffer, ValueSerializer per structured clone), togliendo l'emulazione jsapi pezzo per pezzo |
| egui | Solo splash, dialoghi ed errori | Valutare dialoghi nativi SDL3 e uno splash disegnato direttamente: egui potrebbe uscire del tutto |

## GPU

- Windows usa ANGLE (GL → D3D11). Più compatibile; il GL nativo è più veloce su NVIDIA/AMD ma
  fragile con i driver Intel. Opzione futura: scelta automatica per vendor, con ANGLE di riserva.
- Il costo per frame misurato è quasi tutto nel render di WebRender e nel driver (vedi
  `ROVES_PERFORMANCE.md`). Il guadagno vero passa da un renderer che parla direttamente
  D3D12/Vulkan/Metal.

## Console

| Vincolo | PlayStation / Switch | Xbox |
| --- | --- | --- |
| JIT | Vietato: serve V8 `jitless` (feature già presente; niente WebAssembly) | Permesso in GDK |
| Grafica | Niente OpenGL: serve un renderer non GL | D3D12; ANGLE su D3D11 forse in modalità GDK desktop |
| Processi | Un solo processo | Un solo processo |
| Media | GStreamer non praticabile: decoder della piattaforma | Media Foundation |
| SDK, SDL3, Rust | Sotto NDA tramite i programmi sviluppatore | GDK |

Percorso a tappe:

1. **Desktop come banco di prova:** modalità single-process, `jitless` e un backend media senza
   GStreamer, validati su Windows e Linux.
2. **Xbox su PC tramite GDK:** è Windows, con JIT e D3D11 permessi; la tappa più vicina.
3. **Renderer non GL:** wgpu/Vello al posto di WebRender GL (Servo ha già un lavoro `vello`). È il
   collo di bottiglia per tutte le console.
4. **Switch e PlayStation:** dopo l'accesso agli SDK, build di V8 e del codice Rust per quelle
   piattaforme.

Le tappe 3 e 4 sono lavori di mesi; le tappe 1 e 2 si possono iniziare subito.


### Eccezione confermata29ottobre: difetti delle librerie che bloccano giochi

L'utente autorizza fork e patch minime con prova negativa/positiva, issue/PR upstream e
TODO di ricontrollo/rimozione dopo releaseufficiale corretta. Caso attuale: snapshotMIT
deno_core0.413, issue37097/draftPR37098 in denoland/deno (libs/core). Il vecchio repository
deno_core e archiviato dopo il trasferimento; il progetto resta mantenuto nel monorepo.
La priorita al riuso rimane: niente grafo duplicato, API private o manutenzione permanente
non necessaria. Native117/116verdi; copertura browser23check da confermare inCI.
Dettagli, provenienza e criteri di rimozione: handoff0428 e ROVES_PATCHES.md nello snapshot.
Windows sara sbloccato piu tardi: non simulare prove manuali o interagire col blocco accesso.
