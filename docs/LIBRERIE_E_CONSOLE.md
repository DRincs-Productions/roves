# Roves: librerie, GPU e console

Obiettivo a lungo termine: Roves come assemblaggio di librerie mantenute da altri, con solo il
codice necessario a farle lavorare insieme. Questo documento elenca cosa si può togliere o
sostituire, in ordine di rapporto guadagno/rischio.

Le rimozioni e migrazioni completate sono registrate in `CUSTOMIZATIONS.md`.

## Shell Servo e riduzione dei servizi — decisione del 27 ottobre 2026

La shell Servo resta il contenitore di Roves; preservare la sua architettura interna non è
un vincolo. Moduli, gestione asincrona e altri servizi Servo possono essere sostituiti e
rimossi quando una libreria mantenuta copre il lavoro. DOM, layout e rendering rimangono
inizialmente in Servo: deno_core non fornisce un motore completo della pagina.

rusty_v8 è già utilizzato. deno_core è autorizzato come candidato per una prova isolata di
moduli e operazioni asincrone, non ancora adottato in produzione. La prova deve verificare
compatibilità V8/JITless, frame multipli, import statici/dinamici/JSON, cicli, top-level await,
microtask, CSP/origini e rilascio dei contesti. Distinguere test del runtime da integrazione
HTML effettivamente dimostrata. Usare API pubbliche mantenute; niente fork nostro o doppio
percorso permanente. Rimuovere il sottosistema sostituito e mantenere solo il collegamento
necessario. Non aggiungere una dipendenza per un uso futuro o un risparmio ipotetico.

Nessuna installazione separata di Deno/Chromium, nessun cambiamento intenzionale alle API
JavaScript dei giochi. Valutare licenze, piattaforme, dipendenze transitive e costi reali di
avvio/memoria. Non promettere percentuali di risparmio. Handoff corrente in V8_MIGRATION.md.

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
