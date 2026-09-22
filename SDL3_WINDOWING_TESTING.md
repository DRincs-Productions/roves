# SDL3 windowing — cosa serve testare su hardware reale

Questo file esiste perché chi ha scritto/mantenuto il codice qui finora (istanze Claude, in
sessioni agentiche su una macchina Windows senza accesso a un Mac reale, senza monitor multipli,
e con solo CI headless come mezzo di verifica) **non può testare interattivamente nulla di quello
che descrive sotto**. È un handoff esplicito per chi (umano o un'altra AI con accesso reale
all'hardware) porta avanti questo lavoro con la possibilità di premere davvero dei tasti, muovere
davvero un mouse, e guardare davvero uno schermo.

La migrazione da `winit` a SDL3 (finestra, event loop, input) è mergiata su `main` e rilasciata
(a partire da `v0.4.24`, con fix successivi fino a `v0.4.26` — vedi `CUSTOMIZATIONS.md`, entry dal
2026-09-17 in poi). La CI verifica solo che il codice **compili** e che un binario si **avvii e non
crashi** su un runner headless (nessun monitor, nessuna sessione grafica reale, nessuna interazione
umana) — non verifica se la finestra è realmente utilizzabile su hardware vero.

## Cosa resta da verificare a mano, per piattaforma

Tastiera/mouse/touch/IME sono implementati; questo è l'elenco di regressioni plausibili che solo
un umano (o un'AI con accesso reale allo schermo e all'hardware) può notare:

- **Finestra**: apertura, resize (anche trascinando i bordi), ridimensionamento da tastiera/
  sistema, minimizza/massimizza, fullscreen (ingresso e uscita, anche con la scorciatoia da
  tastiera del gioco), multi-monitor (spostare la finestra tra schermi con DPI diversi),
  chiusura pulita (nessun processo residuo).
- **Branding/decorazioni**: icona corretta in finestra e taskbar (incluso override `icon.png`),
  e rendering/comandi finestra corretti con `no_native_titlebar`.
- **Input**: ogni tasto rilevante per il gioco, combinazioni/scorciatoie (copia/incolla, zoom,
  gli shortcut definiti in `handle_intercepted_key_bindings`), click sinistro/destro/centrale,
  rotellina, drag, hover (tooltip egui), cursore nascosto e forme pointer/testo/resize/not-allowed.
- **Gesture**: pinch-in/pinch-out su trackpad o touchscreen, verificando centro e direzione dello
  zoom anche dopo aver cambiato finestra/focus.
- **IME**: aprire un campo di testo con una lingua che richiede composizione (es. giapponese) e
  verificare che appaia la finestra di composizione nella posizione corretta; composizione CJK e
  dead key restano da verificare su hardware reale.
- **Accessibilità**: screen reader (VoiceOver su macOS, Narrator su Windows, Orca su Linux) —
  la UI egui e la pagina web dentro Servo devono restare navigabili. L'adapter AccessKit nativo
  SDL3 esiste ma non è mai stato provato con uno screen reader reale.
- **HiDPI**: schermi con scala diversa da 100% (Retina su macOS, scaling Windows) — verificare
  che testo/UI non siano sfocati o mal dimensionati.
- **Gamepad**: su Windows/Linux (macOS è escluso a prescindere, vedi sotto) — verificare che i
  controller funzionino ancora dopo la migrazione a SDL3.
- **Reattività con contenuto WebGL/canvas continuo** (PixiJS, Three.js, o qualsiasi pagina che
  usa `requestAnimationFrame` in modo continuo): un utente reale ha segnalato la finestra "non
  risponde" su Windows in questo scenario su `v0.4.24` — causa root confermata e corretta in
  `v0.4.26` con `RedrawCoalescer` (vedi `CUSTOMIZATIONS.md`, entry 2026-09-20/21). Confermato
  funzionante dall'utente su `v0.4.26`. Resta da riverificare su macOS/Linux, dato che il bug
  era nella logica dell'event loop condivisa, non in codice Windows-specifico.

## Perché macOS specificamente ha bisogno di un umano

macOS ha già una storia di comportamenti che una CI headless non riesce a riprodurre fedelmente —
il gamepad SDL3 si blocca indefinitamente lì e resta disattivato (causa reale non confermata, vedi
`CUSTOMIZATIONS.md`) — probabilmente legati a permessi di sistema (TCC) o alla mancanza di una
vera sessione grafica sui runner GitHub Actions. È plausibile che SDL3 windowing stesso nasconda
problemi simili (creazione finestra, focus, fullscreen) che solo un Mac reale, con una sessione
utente vera, può rivelare.

## Come riprendere questo lavoro

1. Leggere `CUSTOMIZATIONS.md`, le entry dal 2026-09-17 in poi (sezione SDL3 windowing), per il
   dettaglio tecnico di ogni file toccato.
2. Procedere con i test manuali elencati sopra, su hardware reale per ciascuna delle tre
   piattaforme desktop.
3. Aggiornare questo file rimuovendo le voci verificate man mano che vengono confermate.
