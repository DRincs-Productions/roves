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

La policy deve impedire richieste non supportate prima di sostituire il documento, senza
interrompere il gioco, e segnalarle almeno nei log/console con operazione e motivo. Preferire
diagnostica/UI esistenti; limitare ripetizioni e non usare alert modali. Un avviso visibile
è discreto e non modale solo in sviluppo; nel gioco distribuito usare solo i log.
I link esterni possono aprire il browser di sistema solo dopo un clic reale dell'utente;
le richieste automatiche restano bloccate e segnalate. Non si naviga dentro Roves.
Ordine scelto: prima la prova deno_core per un documento, poi policy e diagnostica insieme
al runtime selezionato. Blocco e nuove segnalazioni sono DA IMPLEMENTARE, non già disponibili.
Dettagli e casi di accettazione: [handoff0380](handoff/0380-single-document-runtime.md).

Si possono sostituire e rimuovere servizi interni Servo, non solo affiancarli. DOM/layout/
rendering restano inizialmente Servo. rusty_v8 è già usato; deno_core è da rivalutare per
il singolo documento, senza i vecchi vincoli di frame multipli/WindowProxy fra navigazioni.
La [valutazione0378](handoff/0378-deno-core-feasibility.md) resta evidenza del precedente
perimetro: lock194pacchetti/metadata/parse verificati, nessun test runtime o misura eseguiti.
Non costituisce una bocciatura del nuovo prodotto, né prova di fattibilità dell'integrazione.

Verificare API pubbliche mantenute per DOM/eventi, moduli statici/dinamici/JSON/cicli/TLA,
microtask HTML/Promise, CSP/origini, GC/chiusura e JITless. Adottare solo con rimozioni reali
dei servizi sostituiti, senza fork nostro o doppio percorso permanente. Nessuna dipendenza
produttiva deno_core è ancora aggiunta, nessun privilegio Deno ai giochi o installazione
separata Chromium/Deno. Valutare licenze, dipendenze e costi effettivi senza promettere
percentuali. Build pesanti online; handoff corrente in V8_MIGRATION.md.

### Prova0381 verificata online, bootstrap0382 pronto per CI

[Handoff0381](handoff/0381-deno-single-document-probe.md): workspace isolato in
support/deno-core-probe, nessuna dipendenza produttiva. Dieci scenari logici in due test
Rust, una sola thread per gli scenari V8, stessa suite normale/JITless su Linux/Windows.
Metadata/parse e contratto YAML/lock verificati; CI40539608247 verde4/4, Linux/Windows
normale/JITless. Due testRust perjob coprono dieci scenari logici, non integrazione DOM. RustyV8150 espone il flag globale di contesto per eval/Function, non la callback per
singola stringa di152: gap CSP/TrustedTypes ancora da valutare. Nessun DOM/router simulato,
nessuna prova di integrazione reale o misura prestazionale. Prova bootstrap0382 completata dopo il reset: rimuove globali interni prima del gioco e
riesegue scenari moduli/async con import interni rifiutati, via API pubbliche. Metadata/parse
verificati, nuove asserzioni in attesa CI; non è isolamento generale o integrazione DOM.
Ricontrollo19:02 eliminato. Prossima prova classe nativa/cppgc; mantenere un solo servizio.
Vedere handoff/0382-deno-bootstrap-isolation.md.

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
