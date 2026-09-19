# SDL3 windowing — cosa serve testare su hardware reale

Questo file esiste perché chi ha scritto il codice qui (un'istanza Claude, in una sessione
agentica su una macchina Windows senza accesso a un Mac reale, senza monitor multipli, e con
solo CI headless come mezzo di verifica) **non può testare interattivamente nulla di quello che
descrive sotto**. È un handoff esplicito per chi (umano o un'altra AI con accesso reale
all'hardware) riprenderà questo lavoro con la possibilità di premere davvero dei tasti, muovere
davvero un mouse, e guardare davvero uno schermo.

Vedi `TODO.md`'s sezione "SDL3" e `CUSTOMIZATIONS.md`'s entry del 2026-09-17/18 per il contesto
tecnico completo di *cosa* è stato scritto. Questo file si concentra solo su *cosa va verificato
a mano e perché la CI non basta*.

## Il problema in una frase

Il branch `sdl3-windowing` sostituisce `winit` con SDL3 per finestra ed event loop. La CI
(`test.yml`) verifica solo che il codice **compili** e che un binario si **avvii e non crashi**
in ~15 secondi su un runner headless (nessun monitor, nessuna sessione grafica reale, nessuna
interazione umana) — non verifica se la finestra è realmente utilizzabile.

## Stato compilazione e smoke test

La run GitHub Actions 35401939171 del 18 settembre 2026 è verde su tutta la matrice:

- Linux portable e `.deb` ✅
- Windows portable e `.msi` ✅
- macOS portable e `.dmg` ✅
- variante Steam + smoke test Xvfb ✅

Questo prova applicazione delle patch, compilazione, bundle e sopravvivenza del processo durante
lo smoke test. Non prova la correttezza semantica dell'input.

## Perché la compilazione pulita NON significa "funziona"

Tastiera e mouse di base sono ora tradotti dagli eventi SDL3 e inoltrati al `WebView`; la CI
compila questo percorso, ma non genera ancora eventi sintetici contro una finestra reale. Restano
gap funzionali reali:

- **Tastiera**: la mappatura SDL3 → DOM/Servo esiste; layout non-US, dead key e testo composto
  richiedono ancora test e completamento tramite `TextInput`/IME.
- **Mouse**: movimento, click e rotellina arrivano al gioco; l'inoltro alla UI egui è ancora
  incompleto.
- **Touch**: portato da SDL3 a Servo con ID stabili; richiede prova hardware multi-touch.
- **Gesture/pinch**: non portati.
- **File drop**: collegato da SDL3 al caricamento del file URL; richiede ancora prova hardware.
- **IME** (composizione testo per cinese/giapponese/coreano e simili): non portato — vedi
  `headed_window.rs`'s `show_ime`, ora uno stub vuoto.
- **AccessKit/accessibilità**: rimosso interamente insieme a `egui_winit` — nessun bridge
  sostitutivo scritto ancora (c'è però una roadmap concreta in `TODO.md`, non un buco nero).
- **Icona finestra/taskbar**: non portata (funzione minore, ma reale).
- **Finestre trasparenti** (`no_native_titlebar`): non portate.

In pratica la baseline è avviabile e l'input gameplay essenziale è cablato, ma non è ancora
corretto dichiarare conclusa la sostituzione di winit finché IME, egui, AccessKit e i residui di
dipendenza non sono stati eliminati.

## Piramide di test da costruire

### Livello 1 — contratti statici, a ogni push

Implementato in `support/check_sdl3_windowing_contracts.py` e nel job
`sdl3-windowing-contracts`:

- ogni variante `WindowEvent` deve avere un target di tracing esplicito;
- ogni variante deve comparire nel dispatch della finestra;
- tastiera e mouse gameplay devono restare presenti nella traduzione SDL3;
- `0001-desktop-shell-core.patch` deve essere sintatticamente valido.

Questo livello deve restare privo di dipendenze e terminare in pochi secondi, prima della matrice
Servo.

### Livello 2 — unit test Rust su Linux

Da aggiungere man mano che i componenti vengono separati dall'event loop nativo:

- tabella `Scancode`/`Keycode` → `keyboard_types::{Code, Key, Location, Modifiers}`;
- conversione coordinate mouse con toolbar e HiDPI;
- direzione e unità della rotellina;
- conversione `TextInput`/`TextEditing` in eventi IME;
- traduzione touch/finger e gesture;
- conversione SDL3 → `egui::Event`;
- transizioni focus, fullscreen e richiesta redraw.

Questi test non devono creare una finestra: le conversioni vanno mantenute come funzioni pure.

### Livello 3 — integrazione virtual-display su Linux

Da eseguire con Xvfb e un piccolo harness SDL3:

- avvio della pagina diagnostica;
- injection SDL di key down/up, movimento, click e wheel;
- conferma nel DOM che ordine, coordinate, tasto e modificatori siano corretti;
- resize e redraw senza crash o frame nero permanente;
- file drop su una fixture temporanea;
- apertura/chiusura IME almeno a livello di protocollo SDL.

### Livello 4 — matrice packaging

È il workflow `test.yml` esistente: Linux portable/deb, Windows portable/MSI, macOS
portable/DMG e Steam. Va eseguito dopo i contratti veloci e ai checkpoint significativi, non per
ogni micro-correzione.

### Livello 5 — hardware reale

Resta obbligatorio per IME reale, screen reader, DPI/multi-monitor, fullscreen, gesture e gamepad.
Queste verifiche non sono sostituibili in modo affidabile dai runner GitHub hosted.

## Cosa serve testare a mano, per piattaforma, una volta che l'input sarà portato

Questa lista è pensata per DOPO che tastiera/mouse/touch/IME saranno reimplementati — è
l'elenco di regressioni plausibili che solo un umano (o un'AI con accesso reale allo schermo)
può notare:

- **Finestra**: apertura, resize (anche trascinando i bordi), ridimensionamento da tastiera/
  sistema, minimizza/massimizza, fullscreen (ingresso e uscita, anche con la scorciatoia da
  tastiera del gioco), multi-monitor (spostare la finestra tra schermi con DPI diversi),
  chiusura pulita (nessun processo residuo).
- **Input**: ogni tasto rilevante per il gioco, combinazioni/scorciatoie (copia/incolla, zoom,
  gli shortcut definiti in `handle_intercepted_key_bindings`), click sinistro/destro/centrale,
  rotellina, drag, hover (tooltip egui).
- **IME**: aprire un campo di testo con una lingua che richiede composizione (es. giapponese) e
  verificare che appaia la finestra di composizione nella posizione corretta.
- **Accessibilità**: screen reader (VoiceOver su macOS, Narrator su Windows, Orca su Linux) —
  la UI egui e la pagina web dentro Servo devono restare navigabili.
- **HiDPI**: schermi con scala diversa da 100% (Retina su macOS, scaling Windows) — verificare
  che testo/UI non siano sfocati o mal dimensionati.
- **Gamepad**: su Windows/Linux (macOS è escluso a prescindere, vedi sopra) — verificare che i
  controller funzionino ancora dopo la migrazione a SDL3 (già portato prima di questo branch,
  ma va riverificato qui dato quanto è cambiato intorno).

## Perché macOS specificamente ha bisogno di un umano

Oltre al problema generale sopra, macOS ha già una storia di comportamenti che una CI headless
non riesce a riprodurre fedelmente (vedi il gamepad hang, ignorato per ora ma documentato in
`CUSTOMIZATIONS.md`) — probabilmente legati a permessi di sistema (TCC) o alla mancanza di una
vera sessione grafica sui runner GitHub Actions. È plausibile che SDL3 windowing stesso nasconda
problemi simili (creazione finestra, focus, fullscreen) che solo un Mac reale, con una sessione
utente vera, può rivelare.

## Come riprendere questo lavoro

1. Branch: `sdl3-windowing` (non mergiato su `main`).
2. Leggere `CUSTOMIZATIONS.md`, le entry datate 2026-09-17/18 (sezione SDL3 windowing), per il
   dettaglio tecnico di ogni file toccato.
3. Leggere `TODO.md`, sezione SDL3, per la roadmap di cosa manca (inclusa la parte AccessKit,
   già scoperta/derisked ma non implementata).
4. Verificare prima di tutto lo stato di compilazione macOS (vedi sopra).
5. Implementare l'input (tastiera/mouse/touch/IME) — il pezzo di lavoro più grande rimasto.
6. Solo dopo, procedere con i test manuali elencati sopra, su hardware reale per ciascuna delle
   tre piattaforme desktop.
