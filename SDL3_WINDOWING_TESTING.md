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

## Stato compilazione (CI), per riferimento — non è quello che serve testare qui

- Linux: compila ✅ (confermato via CI reale, 2026-09-18)
- Windows: compila ✅ (confermato via CI reale, 2026-09-18)
- macOS: da verificare — l'ultimo run CI è rimasto bloccato oltre il tempo normale (~85+ minuti
  contro i ~20-25 minuti tipici); non è chiaro se sia un hang reale (come quello già noto e
  irrisolto del gamepad, ora ignorato per questa settimana su richiesta) o solo lentezza. Non
  investigato ulteriormente per risparmiare budget — prima cosa da controllare quando si riprende
  in mano il branch.

## Perché la compilazione pulita NON significa "funziona"

**Zero input è stato portato a SDL3 finora.** Tutto quello che segue è un gap reale, non un
dettaglio minore:

- **Tastiera**: nessun evento tasto arriva al gioco. `desktop/keyutils.rs` (mappatura codici
  tasto winit → DOM/Servo) non è stato toccato — va rifatto per i codici SDL3.
- **Mouse**: nessun click, movimento o rotellina arriva al gioco o alla UI egui.
- **Touch/gesture**: non portati.
- **IME** (composizione testo per cinese/giapponese/coreano e simili): non portato — vedi
  `headed_window.rs`'s `show_ime`, ora uno stub vuoto.
- **AccessKit/accessibilità**: rimosso interamente insieme a `egui_winit` — nessun bridge
  sostitutivo scritto ancora (c'è però una roadmap concreta in `TODO.md`, non un buco nero).
- **Icona finestra/taskbar**: non portata (funzione minore, ma reale).
- **Finestre trasparenti** (`no_native_titlebar`): non portate.

In pratica: oggi, su qualsiasi piattaforma, una finestra SDL3 si apre, mostra lo splash animato
di avvio, si ridimensiona, e si chiude — ma non è possibile giocarci, cliccare, scrivere, o
usare l'interfaccia egui in alcun modo. Questo è esattamente il tipo di rottura che una CI
headless da 15 secondi non può notare, perché non manda mai un vero evento tastiera/mouse.

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
