# Roves

## Lingua e direzione del progetto

- **In chat rispondi sempre in italiano.** Codice, commit e `CUSTOMIZATIONS.md` restano
  in inglese. Non si creano file patch di tracciamento, né verso upstream né per le librerie
  vendorizzate: il sorgente si modifica direttamente (vedi "Independent source" sotto).
- **Meno codice nostro.** Delegare il più possibile a librerie mantenute da altri e scrivere
  solo il codice che serve a farle lavorare insieme. Togliere il codice non usato e, quando una
  libreria esistente copre una parte del lavoro, preferirla al codice scritto a mano.

### Prodotto attuale (26 novembre 2026)

Il prodotto desktop è il player CEF: `support/cef-windows-player` (Windows x64),
`support/cef-macos-gate` (macOS arm64) e `support/cef-linux-gate` (Linux x64), pubblicati
da `release.yml`. Chromium (CEF 154) esegue il gioco con il proprio V8; welding/grafting
(fork temporaneo in `support/vendor/wgpu-weld`) consegnano il frame a wgpu; SDL3 gestisce
finestra, eventi e input. Android e iOS restano sul motore Servo al tag `v0.7.15`. Il motore
Servo/V8, la probe Blitz/Deno e Netrender sono stati rimossi da `main`: non reintrodurli.

Il player esegue un solo documento di gioco per finestra, con route interne (History API e
frammenti, senza nuovo documento). Tab, finestre aggiuntive e iframe non sono requisiti.
Reload e navigazioni complete sono bloccati prima di sostituire il documento; i link esterni
si aprono nel browser di sistema solo dopo un clic utente reale. Conservare questa policy,
l'origine loopback dedicata, la sicurezza CSP/origini e l'assenza di API privilegiate per la
pagina (oltre alla host API `/__roves/` e al bridge MCP opzionale di sviluppo).

### Eccezione autorizzata: fix di librerie che bloccano i giochi — 29 ottobre 2026

L'utente autorizza fork e patch minime per difetti di librerie che impediscono il
funzionamento di un gioco. Non autorizza una riscrittura generale. Preferire aggiornamenti
mantenuti quando già corretti. Preparare riproduzione negativa/positiva, mantenere licenze e
pin riproducibili, aprire issue e pull request upstream quando possibile (autorizzato senza
chiedere di nuovo) e aggiungere in TODO il ricontrollo per rimuovere il fork appena una
release ufficiale corregge il difetto. Build pesanti online, revisione, record dei fork
(`ROVES_PATCHES.md`) e gate CI restano validi.

### Valutazione obbligatoria prima di ogni modifica

Prima di iniziare qualsiasi modifica, valuta se librerie esterne mantenute, incluse quelle
già presenti nel progetto, possono coprire l'implementazione. L'obiettivo è ridurre il codice
gestito da Roves: preferisci riuso e integrazione, conserva solo il codice di collegamento
necessario e rimuovi il codice sostituito. Valuta manutenzione, licenza, compatibilità con le
piattaforme e impatto su CPU, RAM e GPU. Non aggiungere dipendenze senza un uso concreto;
se nessuna libreria è adatta, spiega brevemente il motivo prima di implementare codice nostro.
Questo vale in particolare per l'ecosistema dei player: cef-rs, wgpu-weld/grafting, wgpu,
SDL3, steamworks-rs e le API già offerte da Chromium. Registrare l'esito (adottata, scartata
con motivo, o rinviata nel TODO).

### Segnalazione dei bug nelle librerie

Quando individui un bug evidente in una libreria, usa `@github` per aprire
autonomamente un'issue nel repository upstream, senza chiedere conferma. Se esiste
gia un'issue per lo stesso bug, usa quella. Aggiungi il link in `TODO.md` e indica
che va ricontrollato se e in quale versione il bug viene risolto.

### Confronto obbligatorio con Chromium

Prima di ogni implementazione importante o intervento sulle prestazioni, consultare il
codice e la documentazione pertinenti di Chromium alla revisione usata da CEF (online, ad esempio
source.chromium.org o GitHub `chromium/chromium` a quel commit).
Identificare come Chromium risolve lo stesso problema e annotare revisione, file consultati,
scelte rilevanti e compromessi nel piano o nel documento performance prima di scrivere codice.
Tradurre le idee nel contesto Rust di Roves, valutando prima le API e le librerie mantenute
di CEF, wgpu-weld, wgpu e SDL3. Motivare le differenze quando il modello di Chromium non
si adatta a Roves. Per gli interventi di fluidità, confrontare build release di Roves e Chrome
sullo stesso hardware, con la stessa scena, risoluzione e frequenza di aggiornamento. Usare
gli intervalli tra frame mostrati (P95/P99), la latenza dell'input e il costo CPU/GPU per
verificare il progresso verso prestazioni paragonabili.

Il sottomodulo `support/chromium-reference` è stato rimosso il 25 novembre 2026 su richiesta
dell'utente; le citazioni esistenti (revisione `f2700724f59f`) restano verificabili online.
Non reintrodurre checkout di Chromium nel repository, nella CI o nei pacchetti.

### Diagnosi del browser con Chrome DevTools (MCP)

Usa Chrome DevTools (MCP), quando disponibile, per ispezionare pagine, console, rete e
prestazioni e per raccogliere il riferimento Chrome nei confronti con Roves. Registra versioni,
scena e condizioni di misura, distinguendo esecuzioni visibili e headless. Annuncia prima
profili o misure costose. Se il MCP non è disponibile o l'accesso è bloccato, dichiaralo
esplicitamente e non presentare misure ottenute con altri strumenti come risultati del MCP.

## Coordinamento con Claude e pubblicazione durante la CI

Codex dirige il lavoro: studia il problema, valuta il riuso di librerie mantenute e definisce
passo per passo un incarico concreto per Claude Code, disponibile tramite terminale.
Il prompt deve indicare obiettivo, file interessati, vincoli, documentazione da aggiornare e
criteri di completamento. Claude implementa l'incarico e poi si ferma, senza commit, push o
avvio autonomo di nuovi incarichi. Codex revisiona il risultato, completa le verifiche previste,
gestisce commit e push e analizza la CI. Eventuali correzioni vengono delegate a Claude con
un nuovo incarico preciso, ripetendo il ciclo. Non considerare concluso un incarico lasciato
a metà: ogni implementazione deve essere coerente e teoricamente completa e funzionante
prima di passare al controllo della CI e alla decisione di pubblicare.

### Gate CI Apple aggiornato (5 novembre 2026)

Dal player CEF (v0.9.0) release.yml chiama sul tag i tre workflow dei player, incluso il gate macOS arm64 (Metal, pacchetto standard e Steam), prima di pubblicare. Android e iOS restano sul motore Servo al tag v0.7.15 e non fanno parte di questo rilascio. Dal 26 novembre 2026 (scelta dell'utente) i workflow macOS e Linux non partono più a ogni push: girano solo alla creazione di un tag di rilascio (release.yml) o a mano. La CI ordinaria sui commit è solo Windows (lo sviluppo si verifica su Windows, dove l'utente può testare); non riattivare trigger push per macOS/Linux. Ogni release contiene comunque tutte e tre le piattaforme e si pubblica solo se i test Windows, macOS e Linux del tag sono tutti verdi.

### Procedura di implementazione: loop autonomo

La procedura è un **loop**, non un singolo incarico seguito da un'attesa o da una richiesta
ordinaria di conferma. Ripetere questi passi finché resta lavoro utile e autorizzato:

1. Codex individua il prossimo problema concreto, valuta il riuso di librerie mantenute e
   prepara un incarico circoscritto con criteri di completamento.
2. Claude implementa e verifica l'incarico, poi consegna e si ferma senza commit o push.
   Codex revisiona diff ed evidenze; se il risultato è incompleto o errato, delega la
   correzione e ripete questa fase fino a ottenere una modifica completa.
3. Aggiornare documentazione (e i record dei fork di librerie, se toccati), completare le verifiche necessarie e
   creare il commit locale. Controllare insieme le run del precedente push.
4. Se una run è rossa, analizzarla e completare le correzioni da includere nel prossimo
   push; attendere anche la conclusione delle altre run. Se nessuna è rossa ma alcune
   sono ancora attive, conservare i commit locali e tornare al passo 1 senza push.
5. Quando il batch precedente è interamente verde, pubblicare il lavoro completo e
   revisionato, identificare le nuove run con un controllo aggregato e tornare subito
   al passo 1 mentre la CI procede.

Non fermarsi al termine di un incarico, di un commit o di un push se esiste altro lavoro
utile autorizzato. Il loop non autorizza polling continuo, push durante run precedenti
attive o pubblicazione di WIP. Fermarsi per stop esplicito dell'utente, quota effettiva,
blocco non risolvibile autonomamente o assenza di altro lavoro utile; lasciare un handoff
accurato. Per quota e ripresa applicare la regola `/usage` e reset più 2 minuti sotto.
Le eventuali eccezioni esplicite dell'utente alla delega restano valide nel loro ambito.

### Ripresa Codex vicino al limite delle cinque ore

Quando Codex sta implementando direttamente ed e vicino al limite della finestra di cinque
ore, verificare i limiti reali con get_usage_limits, senza stimare il reset dalla durata
della chat. Considerare vicino il limite quando resta al massimo il10% della finestra5ore.
Salvare prima un handoff aggiornato con Git reale, WIP, verifiche e prossimo passo concreto.
Programmare nella stessa conversazione un SOLO tentativo al reset restituito dal servizio
piu2minuti, tramite scheduler dell'app; riusare un ricontrollo equivalente gia esistente.
L'utente autorizza questa ripresa una tantum: non occorre una nuova conferma ogni volta.

Il promemoria deve leggere AGENTS e il solo handoff corrente, controllare Git e quota, poi
riprendere il lavoro autorizzato se disponibile rispettando ownership e gate CI.
Disattivarlo dopo il tentativo. Se lo stesso limite persiste, salvare l'esito e fermarsi:
nessuna catena di retry, polling, cambio modello o consumo di reset dell'abbonamento.
Se reset/quota non sono disponibili o ambigui, non inventare un orario; se lo scheduler
non e disponibile, dichiararlo senza promettere la ripresa. Il tentativo non supera uno
stop esplicito, limiti settimanali o altre restrizioni dell'utente (incluso lo stop Claude).
Nessuna automazione periodica. Una nuova finestra esaurita dopo una ripresa riuscita e un
nuovo evento, da valutare con dati aggiornati.

### Budget e separazione dei ruoli

La priorità è il minimo consumo complessivo, non una divisione rigida 50/50 dei token.
Quote degli abbonamenti, token nuovi, cache e costi indicativi sono grandezze diverse.
Non promettere un risparmio percentuale senza misure confrontabili.

- Un solo incarico Claude attivo. Codex prepara obiettivo, pochi file pertinenti, vincoli,
  verifiche e risultato atteso; poi revisiona il risultato. Non sviluppare o ricercare in
  parallelo la stessa soluzione già delegata. Eventuale lavoro indipendente deve essere
  necessario al risultato corrente, circoscritto e privo di sovrapposizioni.
- Claude implementa e svolge le verifiche concordate, consegnando diff, esiti sintetici e
  percorsi dei log. Codex controlla queste evidenze e il diff; ripete un test soltanto se
  cambia il codice, manca una verifica necessaria o esiste un dubbio concreto da risolvere.
- Nessun tetto numerico di chiamate per incarico e nessuna fermata per raggiungimento
  di 25 o 50 chiamate. Proseguire autonomamente con incarichi circoscritti fino al risultato
  completo; non richiedere estensioni di budget e non registrare consumo o lavoro residuo
  come contabilità ordinaria. Dopo un diniego dei permessi analizzare la configurazione
  prima di riprovare: niente tentativi equivalenti ripetuti né aggiramento dei rifiuti
  della revisione automatica. Risolvere autonomamente i blocchi tecnici recuperabili.
- Spezzare prima della delega gli incarichi che richiedono più risultati indipendenti;
  mantenere insieme implementazione e verifiche indispensabili alla sua correttezza.
  Una modifica incompleta resta WIP separato e non va pubblicata. Non ridurre le verifiche
  necessarie per accelerare la consegna.
- Leggere istruzioni e handoff correnti una sola volta per incarico; usare ricerche e
  intervalli mirati per gli altri file. Evitare letture integrali di cronologia, changelog,
  patch, output API e log. Usare rg con percorsi limitati, escludendo target, .git e node_modules
  salvo una ricerca esplicita su un artefatto preciso.
- Salvare output estesi nei log e riportare esito, conteggi ed estratti pertinenti degli
  errori. Non riversare stream JSON completi nel contesto. Aggiornare la documentazione
  una sola volta per implementazione completa, includendo le integrazioni della revisione.
- Non mantenere un ciclo di risvegli del modello per osservare un processo Claude.
  Attendere il risultato dell'incarico o un blocco; nessuna analisi duplicata durante l'attesa.
- Per CI non terminali, fare un controllo aggregato al termine di un'implementazione o a
  un punto di verifica motivato. Se manca altro lavoro utile, lasciare un handoff e fermarsi;
  non fare polling continuo. Nessuna automazione periodica predefinita. Un eventuale
  ricontrollo programmato deve essere esplicitamente richiesto, circoscritto e disattivato
  quando termina il motivo del controllo.
- Nel loop attivo richiesto dall'utente, se la CI e ancora in corso e non resta altro lavoro
  utile autorizzato, impostare un timer una tantum pari alla durata di esecuzione dell'ultima
  CI completata con successo piu 5 minuti. Alla scadenza fare un solo controllo aggregato
  delle run del commit e proseguire: scaricare gli artifact necessari e testarli, correggere
  eventuali fallimenti tramite Claude e ripetere il loop. Non fare polling intermedio. Se non
  e disponibile un timer, dichiararlo e lasciare il checkpoint con link alla run.
- Per controllare GitHub Actions, preferire il plugin `@github` disponibile in Codex:
  leggere in una singola richiesta aggregata le run associate allo SHA e i riepiloghi dei job,
  riportando solo nome, stato ed esito. Scaricare i log soltanto per i job falliti o quando
  un dubbio concreto richiede i dettagli; non riversare risposte JSON estese nel contesto e
  non ripetere controlli senza un nuovo punto di verifica. Usare CLI/API alternative solo
  se il plugin non espone i dati necessari o non è disponibile, senza chiedere all'utente
  credenziali che non servono.
- Al raggiungimento di una quota effettiva, salvare il punto di ripresa e sospendere il
  lavoro. Quando Claude segnala una quota esaurita, aprire la CLI e usare `/usage` per
  verificare quota e prossimo reset, senza inviare richieste al modello. Per il limite
  della sessione di 5 ore, pianificare un unico tentativo all'orario di reset riportato
  da `/usage` più 2 minuti, nella stessa chat tramite lo scheduler dell'app. Interpretare
  data e fuso orario mostrati dalla CLI; non calcolare 5 ore dal messaggio di blocco.
  Se il reset non è disponibile o è ambiguo, salvare il blocco senza inventare un orario. Usare un unico ricontrollo con scadenza; riusare quello esistente, senza duplicati
  o controlli intermedi. Un orario esplicito dell'utente prevale; il tentativo23ottobre18:45 è già concluso,
  non riutilizzare quell'orario per nuovi eventi di quota.
  Al risveglio riprendere l'incarico salvato se Claude è disponibile, mantenendo revisione,
  verifiche e vincoli CI. Se lo stesso limite persiste, registrare l'esito e fermarsi senza
  concatenare altri tentativi per quel blocco. Disattivare il ricontrollo dopo il tentativo.
  Una nuova sessione di lavoro esaurita in seguito costituisce un nuovo evento5ore.
  Questa autorizzazione non copre quote settimanali/mensili, stop espliciti o cambi modello
  per aggirare limiti. Se lo scheduler non è disponibile, dichiararlo senza promettere
  la ripresa. Non raccogliere nuovi conteggi di tool call, turni o token per incarico;
  conservare le evidenze storiche senza presentarle come obblighi correnti.

Dopo ogni push, identificare le run CI associate al commit pubblicato con un'unica lettura
aggregata. Se esiste altro lavoro utile e autorizzato, avviare il prossimo incarico circoscritto
secondo i limiti sopra; non usare l'attesa per aprire ricerche senza limite.
**La CI in corso non è un motivo per interrompere lo sviluppo.** Se nessuna run è fallita,
proseguire autonomamente con il prossimo lavoro utile e autorizzato, completare e revisionare
le modifiche e creare i relativi commit locali. Non fermarsi al solo resoconto del push o
per attendere la CI; il vincolo durante l'attesa riguarda il push. Non pubblicare i commit
accumulati finché il batch precedente non lo consente secondo le regole sotto.
Al completamento di ciascuna implementazione, controllare tutte le run del push precedente:

- **Anche una sola CI rossa:** analizzare subito log e causa, delegare e completare la
  correzione affinché il prossimo push la includa. Prima di pubblicare, attendere anche la
  conclusione delle altre run del push precedente, raccogliendo e correggendo gli ulteriori
  errori. Non attendere che una run rossa diventi verde senza una correzione o un retry motivato.
- **Nessuna rossa, ma run ancora in corso o in coda:** non fare push. Continuare con altre
  implementazioni complete e conservare i commit in locale; ricontrollare al loro completamento.
- **Tutte le CI verdi:** si può pubblicare il lavoro completo e revisionato accumulato,
  quindi continuare con il prossimo incarico mentre gira la nuova CI.
- **Run cancellate, saltate o stato non accessibile:** non considerarle verdi. Analizzare
  il motivo e stabilire quali verifiche mancano; completarle o ripeterle prima di trattare
  il checkpoint come verificato. Non usare un nuovo push per interrompere run ancora attive.

Se non rimane altro lavoro utile, registrare le run pendenti e fermarsi senza polling;
non considerare verificato il checkpoint e non pubblicare altri commit finché il batch non
consente il push. La prosecuzione autonoma non supera quote effettive o stop espliciti.
Un'indicazione esplicita
dell'utente di fermarsi prevale sulla prosecuzione autonoma. Queste regole governano il
momento del push anche quando le autorizzazioni permanenti sotto consentono di pubblicare
senza chiedere conferma. Le build pesanti vanno eseguite online in CI, come richiesto
dall'utente; mantenere locali soltanto le verifiche leggere appropriate.

## Independent source, no generated tracking diffs

This repository started as a vendored copy of [Servo](https://github.com/servo/servo) and has
since become Roves' own codebase; the Servo engine itself was removed from `main` on
2026-11-23. **Edit the source directly. Do not create or regenerate patch files** for
upstream projects, for vendored libraries, or as a record of a change.

Temporary forks of third-party libraries (today `support/vendor/wgpu-weld`) are also edited in
place. Keep their `ROVES_PATCHES.md` accurate as prose: provenance (published version,
checksum or upstream commit), what each change does and why, the regression tests, the
upstream issues/PRs, and the removal criteria. Exact diffs come from comparing with the
published crate or from git history.

## Standing authorization: autonomous CI/push/release loop

The user has given standing permission to operate autonomously across this whole ecosystem —
this repo, `roves-action`, `roves-packmaster`/Packmaster, `roves-vscode`, `roves-wiki`, `roves-api`,
and any game repo used to test them (e.g. `pixi-vn-react-template`) — without stopping to ask
for confirmation at each step:

- **Push without asking for confirmation, subject to the CI publication rules above.**
  Commit directly to the active branch (usually `main`); publish complete, reviewed changes
  only when the preceding CI batch permits it. Applies to every sibling repo in this ecosystem.
- **Run whatever CI/CD is needed to verify a fix.** Push branches/tags, trigger runs, and
  watch them to completion as part of normal iteration — CI remains the cross-platform
  authority (Linux/macOS builds aren't possible on this Windows machine; a local Windows
  Rust toolchain exists for light checks). Keep heavy builds online in CI as requested by the user; use local
  lightweight checks where appropriate.
- **Iterate the fix → push → watch CI → fix loop independently** rather than stopping to
  report every intermediate failure — the user prefers being handed the end result (a green
  CI run, a cut release, a working artifact/APK link) over a play-by-play. Report honestly
  when asked directly whether something is finished; don't claim done prematurely.

This doesn't extend to genuinely destructive or hard-to-reverse actions outside the normal
build/test/release loop (force-pushing over someone else's work, deleting a branch/release
nobody asked to delete, rewriting published history) — those still warrant checking first,
same as with any other user.

## GitHub PAT usage (when the user provides one for CI diagnosis)

The user periodically shares a fine-grained GitHub PAT so Actions run logs/annotations can be
read directly via the API instead of asking them to paste a screenshot of every CI failure.
Strict rules for it, every time one is provided, in any repo in this ecosystem:

- **Read-only use.** It's scoped for reading the Actions API (run status, jobs, annotations,
  releases) — not for write operations. A write call (e.g. `POST .../rerun-failed-jobs`)
  will 403; that's expected, not a bug to work around. To retrigger a workflow, use a real
  `git push` instead (git push already works via a separate, independent credential store —
  it never needed this token) — an empty commit (`git commit --allow-empty`) is a legitimate
  way to do this when there's no real code change to make, e.g. retrying after a transient
  CI/infra flake or a release/pin-bump race condition.
- **Never persist it.** Don't write it to any file, don't commit it, don't put it in a script
  saved to disk — export it as an env var inline in the same command that uses it.
- **Re-export it every time.** Shell environment variables don't persist across separate tool
  invocations in this environment — `export GITHUB_TOKEN=...` has to be repeated in every
  command that needs it, not set once and assumed to still be there.
- **It expires.** When a call starts failing with 401/"Bad credentials", say so explicitly
  and ask for a fresh one — don't guess or keep retrying with a dead token.

## Keep CUSTOMIZATIONS.md up to date

For every significant change (behavior, build/bundle surface, platform support, dependencies),
add or update an entry in [`CUSTOMIZATIONS.md`](./CUSTOMIZATIONS.md) in the same turn: files
touched, what changed and why. It is the prose changelog of the engine; no matching patch
file is required or wanted.

## CRITICAL: keep `roves-action`, `roves-cli` and Packmaster in sync with the player

The desktop packages are built by `release.yml` and consumed by
[`roves-cli`](https://github.com/DRincs-Productions/roves-cli) (`roves bundle`, the reference
packager), `roves-action` and `roves-packmaster` (both call roves-cli or mirror its rules).
They are sibling checkouts or submodules of this repository. **Any change here to the
published package surface must be reflected in them in the same turn**, not deferred:

- asset names, variants (standard, Steam, MCP) or the zip layout (executable name, `game/`
  location, icon files, `mcp/` contents, provenance and `SHA256SUMS.txt`);
- the project-icon contract (`support/cef-windows-player/packaging/project_icon.py`, mirrored
  by roves-cli `src/icon.ts`, roves-action and Packmaster);
- player command-line switches or package markers a consumer relies on.

If a consumer checkout isn't present, say so explicitly rather than silently skipping it.
`roves-api` changes only when the host API or the page markers change.

## Ask whether a new packaging option belongs in `roves-packmaster` too

Packmaster is a GUI over the packaging options (platforms, Steam, MCP, icons). Any time you
add, remove, or change the meaning of a roves-cli/player packaging option, **ask explicitly
whether it should also appear in Packmaster's settings** (`src/lib/settings.ts`): the answer
depends on whether a game developer using the GUI would plausibly want to control it.

## CRITICAL: the wiki must always be kept up to date — for anything, not just features

`CUSTOMIZATIONS.md` (above) records *what changed and why*, for maintainers. That's a different audience from *someone using Roves right now* —
a game developer bundling their game, or a player hitting a problem — who reads
[`README.md`](./README.md) and the [`roves-wiki`](https://github.com/DRincs-Productions/roves-wiki)
docs site instead, and neither of those updates automatically just because
`CUSTOMIZATIONS.md` did.

**This isn't scoped to "new user-facing features" alone — read it as "for anything."** A new
packaging option or package output is the clearest case, but the same same-turn obligation
applies to: a behavior change to something already documented (a
default changing, a flag being renamed/removed, a fixed bug that makes an existing wiki page
inaccurate), a new component or distribution path appearing (Packmaster, `roves-action`, or
whatever comes next), or a restructuring of the wiki's own information architecture (tabs,
categories, page moves) when the current structure stops fitting what actually exists. If
you're unsure whether a change is "wiki-worthy," the safer default is to check the wiki
anyway rather than assume it doesn't apply — a stale wiki page is actively worse than a
missing one, since it reads as current and misleads whoever trusts it.

**In the same turn as the change:**

1. Add at least a brief mention to `README.md` — even one sentence and a link out is enough;
   it doesn't need to duplicate the wiki's full explanation, just enough that someone skimming
   the README doesn't miss that the feature exists.
2. Add or update the matching page (or section of an existing page) under `roves-wiki`'s
   `content/docs/` — this is where the real explanation, rationale, and worked examples live.
   `roves-wiki` is a sibling checkout (see the `roves-action` section above for what "sibling
   checkout present" means in practice) — same rule applies: if it isn't present, say so
   explicitly rather than silently skipping this.

Do this unprompted, the same way `CUSTOMIZATIONS.md` gets updated unprompted — don't wait to
be asked, and don't treat "I already wrote the `CUSTOMIZATIONS.md` entry" as covering this
too. A change that's only documented in `CUSTOMIZATIONS.md` is invisible to everyone except
maintainers; a real user has no reason to ever open that file.

## CRITICAL: pushing a fix to `main` is not enough — cut a release and bump the pin too

`roves-action` and `roves-packmaster` never build against this repo's `main` branch — both pin a
specific, already-released version tag (`roves-action/action.yml`'s `ref:`/download URL,
`roves-packmaster/src/lib/shell-version.txt`, shared by its frontend and Rust backend). A fix pushed to `main`
is invisible to both of them, and to anyone building through them, until a new tag is cut *and*
that pin is bumped to point at it — merging to `main` alone changes nothing anyone downstream
actually consumes.

This bit on 2026-09-14: an Android WebView bug fix (immersive mode, a 404 response body) landed
on `main`, `roves-action`'s own CI was green, but a real device test of the APK `roves-action`
produced showed no change at all — because `roves-action` was still pinned to `v0.4.18`, cut
*before* the fix, and its `advanced-mode`/`android`/`ios` paths check out the engine at that
pinned tag, not at `main`'s tip. The fix was real and correct; it just never reached the
artifact being tested. Cutting `v0.4.19` and re-pinning `roves-action` to it was the actual fix.

**So: any time a change here is meant to reach a real user through `roves-action` or
`roves-packmaster` (not just a `test.yml`/`android.yml`/`ios.yml` CI smoke test on `main`) —
not only for Android/iOS, this applies just as much to a desktop-only fix — cut a new release
(below) and bump both pins in the same effort, don't treat "it's on `main` and CI is green" as
done. If you're not sure whether a given change is significant enough to warrant a release yet,
ask rather than silently leaving `main` ahead of the last cut tag indefinitely.**

## Cutting a versioned release (`v<major>.<minor>.<patch>` tags)

`.github/workflows/release.yml` publishes the CEF player release. On a `v*.*.*` tag it calls
the three player workflows (`cef-windows-player.yml` Windows with `release: true`,
`cef-macos-gate.yml`, `cef-linux-gate.yml`), which build and verify the standard, Steam and
MCP packages end to end; its `publish` job zips them, checks every zip and creates the
GitHub Release with ten assets: `roves_shell_{windows,macos,linux}.zip`, the same three with
`_steam` and with `_mcp` (development only), and `SHA256SUMS.txt`. The player workflows test
the packages with the visual-novel-template sample game; the published zips are the shell
only (user decision 2026-11-26: `publish` strips `game/`, the sample icon and provenance via
`.github/release/shell_only_windows.py` and `zip -d`). `@drincs/roves-cli` (`roves-cli/`),
used by roves-action and by Packmaster as a sidecar, adds the developer's build as `game/`. A push to a `release-check/<name>` branch
runs the same pipeline as a dry run (zips uploaded as the `roves-release-dry-run` artifact,
nothing published).

### To cut a release

1. Cut from `main` only, after every project of the ecosystem is merged on its own `main`.
2. Optional dry run: `git push origin HEAD:release-check/vX.Y.Z`, then delete that branch.
3. `git tag vX.Y.Z && git push origin vX.Y.Z`; watch
   `https://github.com/DRincs-Productions/roves/actions/workflows/release.yml`.
4. Once green, confirm at `https://github.com/DRincs-Productions/roves/releases/tag/vX.Y.Z`
   that all ten assets are attached (the workflow's last step also checks it). Download
   them and try them; "the workflow went green" alone is not done.
5. In the same effort, bump the pins in `roves-action` and `roves-packmaster` (below).

### CRITICAL: after every release, sync the shell version in `roves-action` and `roves-packmaster`

The consumer checkouts can be sibling directories or submodules inside this repository
(`roves-action/`, `roves-packmaster/`, `roves-wiki/`). Locate the actual checkout before
reporting it missing. Their engine references do not update just because a tag is pushed:

- **`roves-action`**: the `--shell-version` literal in `action.yml` (DESKTOP PLAYER PIN) and
  the README. Its CLI pin (`ROVES_CLI_VERSION`) moves with roves-cli releases.
- **`roves-packmaster/src/lib/shell-version.txt`**: the single shared pin (frontend through
  Vite's raw loader, Rust through `include_str!`). Update the README and check both paths.
  Its roves-cli sidecar pin is `src/lib/roves-cli-version.txt`.

A change of the package layout (file names, `game/` location, icon files) is a code change
in both consumers, not a pin bump. `roves-api`'s `COMPATIBLE_SHELL_VERSION` is bumped only
when the host API or the page markers change. If a consumer checkout is missing, say so.

### If a run fails: delete-and-republish loop

1. `gh release delete vX.Y.Z --yes` if a release was created (the `publish` job also runs
   `gh release delete ... || true` before creating it).
2. `git tag -d vX.Y.Z && git push origin --delete vX.Y.Z`.
3. Fix the cause on `main` (through the normal CI rules), re-tag the new commit and push.
4. macOS arm64 runners are sometimes cancelled for capacity: a cancelled run is not green;
   re-push the tag rather than publishing without it.

### Diagnosing a failure without `gh`/a token on hand

Raw step logs need repo-admin auth, but `::notice::`/`::error::` annotations are readable
anonymously via `GET /repos/DRincs-Productions/roves/check-runs/{job_id}/annotations`; the
player steps re-emit their key log lines as annotations. With the read-only PAT, the logs
artifacts (`cef-*-logs`, `cef-windows-player-evidence`) can be downloaded through the
artifacts API.
