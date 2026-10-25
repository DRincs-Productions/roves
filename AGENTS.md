# Servo (vendored, customized)

## Lingua e direzione del progetto

- **In chat rispondi sempre in italiano.** Codice, commit, `CUSTOMIZATIONS.md` e patch restano
  in inglese come oggi.
- **Allontanarsi da Servo upstream non è un problema.** L'obiettivo è avere meno codice nostro:
  delegare il più possibile a librerie mantenute da altri e scrivere solo il codice che serve a
  farle lavorare insieme. Togli il codice non usato e, quando una libreria esistente copre una
  parte del lavoro, preferiscila al codice scritto a mano. Restare compatibili con i futuri
  aggiornamenti di Servo non è un vincolo. Piano e candidati sono in `docs/LIBRERIE_E_CONSOLE.md`.

### Valutazione obbligatoria prima di ogni modifica

Prima di iniziare qualsiasi modifica, valuta se librerie esterne mantenute, incluse quelle
già presenti nel progetto, possono coprire l'implementazione. L'obiettivo è ridurre il codice
gestito da Roves: preferisci riuso e integrazione, conserva solo il codice di collegamento
necessario e rimuovi il codice sostituito. Valuta manutenzione, licenza, compatibilità con le
piattaforme e impatto su CPU, RAM e GPU. Non aggiungere dipendenze senza un uso concreto;
se nessuna libreria è adatta, spiega brevemente il motivo prima di implementare codice nostro.

### Diagnosi del browser con Chrome DevTools (MCP)

Usa Chrome DevTools (MCP), quando disponibile, per ispezionare pagine, console, rete e
prestazioni e per raccogliere il riferimento Chrome nei confronti con Roves. Registra versioni,
scena e condizioni di misura, distinguendo esecuzioni visibili e headless. Annuncia prima
profili o misure costose. Se il MCP non è disponibile o l'accesso è bloccato, dichiaralo
esplicitamente e non presentare misure ottenute con altri strumenti come risultati del MCP.

## V8 migration: resumed with bounded delegation

The user explicitly resumed the migration loop on 2026-10-22. Task 0274 is complete locally
within its authorized cumulative 50-call boundary (25 initial plus 22 extension, 47 actual).
Codex may start subsequent bounded assignments and manage the CI loop without asking for
approval at each completed task, and resolve recoverable technical blocks autonomously.
The user removed per-assignment tool-call ceilings and routine consumption/residual-work
accounting. Continue useful authorized work autonomously; actual subscription quotas remain
a stop condition. Historical bounded-task counts are retained only as historical evidence.
No periodic automation is authorized. A single retry at the reset time reported by Claude `/usage`, plus two minutes,
is authorized for a confirmed five-hour session limit as specified below; no intermediate polling.

Before resuming, read only the current handoff in `docs/V8_MIGRATION.md`; consult historical
sections only for a concrete issue. Consumption evidence and the first bounded comparison
are in `docs/AGENT_WORKFLOW.md`; browser performance history is separate in
`docs/ROVES_PERFORMANCE.md`. Prototype coverage is not a completed production migration.
This file is the shared entry point for coding agents.
Dal 23 ottobre 2026 l'utente ha ripristinato la delega a Claude: Codex dirige e revisiona,
Claude implementa incarichi circoscritti via terminale. L'eccezione del22ottobre (sviluppo
Codex diretto per quota Claude) è terminata. Il file temporaneo docs/CLAUDE_RESUME.md
è stato rimosso su richiesta; il punto di ripresa unico è docs/V8_MIGRATION.md.

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

### Procedura di implementazione: loop autonomo

La procedura è un **loop**, non un singolo incarico seguito da un'attesa o da una richiesta
ordinaria di conferma. Ripetere questi passi finché resta lavoro utile e autorizzato:

1. Codex individua il prossimo problema concreto, valuta il riuso di librerie mantenute e
   prepara un incarico circoscritto con criteri di completamento.
2. Claude implementa e verifica l'incarico, poi consegna e si ferma senza commit o push.
   Codex revisiona diff ed evidenze; se il risultato è incompleto o errato, delega la
   correzione e ripete questa fase fino a ottenere una modifica completa.
3. Aggiornare documentazione e patch applicabili, completare le verifiche necessarie e
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
  errori. Non riversare stream JSON completi nel contesto. Aggiornare documentazione e patch
  una sola volta per implementazione completa, includendo le integrazioni della revisione.
- Non mantenere un ciclo di risvegli del modello per osservare un processo Claude.
  Attendere il risultato dell'incarico o un blocco; nessuna analisi duplicata durante l'attesa.
- Per CI non terminali, fare un controllo aggregato al termine di un'implementazione o a
  un punto di verifica motivato. Se manca altro lavoro utile, lasciare un handoff e fermarsi;
  non fare polling continuo. Nessuna automazione periodica predefinita. Un eventuale
  ricontrollo programmato deve essere esplicitamente richiesto, circoscritto e disattivato
  quando termina il motivo del controllo.
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


This directory is a **vendored copy** of [Servo](https://github.com/servo/servo), currently
tracking upstream tag `v0.5.0` (migrated from the original `v0.4.0` import — see
`CUSTOMIZATIONS.md`'s 2026-09-04 entry for that migration's own write-up) — not a git clone,
not a GitHub fork, not a submodule. It is the rendering engine used by the parent project's
"embedded" distribution target (see `../.github/workflows/embedded.yml`, `SERVO_TAG`), as an
alternative to the Tauri build.

## Why vendored instead of forked/submoduled

Servo's full repository is enormous (~1.3GB unpacked), almost entirely `tests/` (the Web
Platform Tests conformance mirror + perf benchmarks — irrelevant to this project). A GitHub
fork was tried first and abandoned: cloning/maintaining a fork of a repo this size for what
is meant to be a handful of small, surgical UI/behavior tweaks was overkill. Downloading the
tagged source zip and applying targeted patches locally is far cheaper to maintain.

## Local git repo

`servo/.git` is a **standalone, local-only** git repository — no remote, not connected to
`servo/servo` or any fork. It was `git init`-ed here purely so the customizations layered on
top of pristine upstream Servo can be diffed/tracked locally. `servo/.gitignore` extends
upstream's own `.gitignore` to additionally exclude upstream bulk/noise we'll never touch
(`tests/`, `docs/`, `.devcontainer/`, project governance files) — see that file for the full
list. Everything else (`ports/`, `components/`, `python/`, `resources/`, `support/`, build
files, and our own `.github/` and `patches/` — see below) stays trackable.

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
  authority (Linux/macOS builds still aren't possible on this Windows machine), but as of
  2026-09-29 a local Windows `cargo build`/`./mach build` toolchain is set up and confirmed
  working (LLVM/clang/`lld-link.exe`/`libclang.dll`, MSVC Build Tools, real GStreamer,
  `depot_tools` — see "Diagnosing a failure without `gh`/a token on hand" below for the
  remaining CI-only gap). Keep heavy builds online in CI as requested by the user; use local
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

## This directory is meant to be self-contained and separable

Everything Servo-specific — patches, and the CI that tests them — lives inside `servo/`
itself (`patches/`, `.github/workflows/`), not in the parent project's own `.github/` or
repo root. The intent is that `servo/` will eventually have nothing to do with the parent
pixi-vn-react-template project and could be lifted out wholesale (e.g. pushed as its own
`BlackRam-oss/servo` repo) without leaving anything behind. Keep that in mind before adding
anything Servo-related outside this directory.

One consequence: **`servo/.github/workflows/servo-test-build.yml` does not currently run
anywhere.** GitHub Actions only auto-discovers workflows under `.github/workflows/` at an
actual repository root. Today `servo/` is just a gitignored subfolder of the parent repo —
not its own pushed repository — so this workflow is dormant until `servo/` is pushed as a
real top-level GitHub repo. See the note at the top of that file.

**Watch out: the `v0.4.0` git tag no longer points at the pristine Servo import.** The very
first commit in this history (message `v0.4.0`, hash starting `dd340af`) is the true,
unmodified baseline every patch is meant to be diffed against — but the *tag* `v0.4.0` was
later reused for this project's own 4th versioned engine release (see "Cutting a versioned
release" below), a pure version-number coincidence between Servo's baseline and this repo's
independent release numbering, and now points at whatever commit that release was cut from
instead. `git diff v0.4.0 -- <file>` therefore silently gives a **wrong** (too-small) diff
for any file touched by an earlier patch — it happened to still work once, for a file no
prior patch had touched, which is how this went unnoticed. When generating or re-verifying a
patch, either diff against the `dd340af` commit hash directly (not the `v0.4.0` tag), or —
for a *new* patch's own isolated diff — just use a plain `git diff -- <file>` (working tree
vs. `HEAD`), which is unaffected by this and is what patches 0050+ were generated with.

## CRITICAL: keep CUSTOMIZATIONS.md *and* patches/ up to date

**Every time you change a file under this `servo/` directory, in the same turn:**

1. Add or update an entry in [`CUSTOMIZATIONS.md`](./CUSTOMIZATIONS.md) — file touched, what
   changed, why (prose, for humans and for the "reapply on upgrade" workflow below).
2. Regenerate the matching patch file under `patches/servo-v<TAG>/` (one file per logical
   change, numbered `0001-`, `0002-`, ...) — a real, machine-applicable unified diff against
   the pristine upstream file for the current tag. Verify it applies cleanly to a fresh
   pristine copy before moving on (`patch -p1 --dry-run < the.patch` from a clean extraction).

Do this unprompted; don't wait to be asked. **The patch files are not optional documentation
— `.github/workflows/servo-test-build.yml` (once running — see that file for why it's
currently dormant) downloads a pristine copy of this same tag on every run and applies every
`.patch` file under `patches/servo-v<TAG>/` to it.** If a change here isn't reflected as an
up-to-date patch, that CI silently tests something other than what's actually in this
directory.

This two-file setup (prose changelog + real patches) is the entire point of vendoring
instead of forking: when the Servo version gets bumped later, the workflow is "download the
new tag's zip, extract it, read `CUSTOMIZATIONS.md` for context on intent, then try applying
each patch from `patches/` — for any that fail to apply cleanly (upstream code moved or
changed shape), manually re-derive that one change against the new source." Without an
accurate, current changelog and patch set, every future upgrade means re-deriving each
customization from scratch by re-diffing behavior against vanilla Servo — exactly the
tedious work this setup exists to avoid. A stale or incomplete `CUSTOMIZATIONS.md`/patch is
worse than an honest gap: if a change here isn't reflected there, fix it as soon as you
notice, even for changes you didn't make yourself.

## CRITICAL: keep `roves-action` in sync with CI/CD changes here

Alongside this repo, on the same machine, there are sibling checkouts of other
`DRincs-Productions` repos that depend on this engine's build/bundle surface — most
importantly `../roves-action` (`DRincs-Productions/roves-action`, a separate git repo, not a
submodule of this one). `roves-action` is a GitHub Action that runs `mach build` + `mach
bundle` against a pinned checkout of *this* repo on behalf of any third-party game's CI —
its own README says outright that it mirrors this repo's own
`.github/workflows/test.yml`. Its `action.yml` inputs are a near 1:1 forwarding layer over
`mach build`/`mach bundle`'s CLI flags (each tagged `[servo]` or `[roves]` in that file to
say which side owns it).

That mirroring is not automatic — it's a manual sync someone (you) has to maintain. **Any
change here that touches this repo's CI/CD or build/bundle surface must be reflected in
`roves-action` in the same turn**, not deferred. Concretely, this includes:

- A new, removed, renamed, or redefaulted `mach build` or `mach bundle` flag (the command
  lives in `python/servo/post_build_commands.py`; see the `## CRITICAL: keep
  CUSTOMIZATIONS.md` section above — those same changes already need a `CUSTOMIZATIONS.md`
  entry and patch).
- A new `mach bundle` output format/target (e.g. the existing `--deb`, or anything added
  after it) — `roves-action`'s `action.yml` needs a matching input, and its `README.md`
  needs a matching example/row in its input tables.
- Changes to what `.github/workflows/test.yml` actually *does* with those flags (build
  matrix entries, packaging/zipping conventions, defaults) — if `test.yml` is the reference
  implementation `roves-action` mirrors, drift here silently makes that mirror stale.

If `../roves-action` isn't present as a sibling checkout when you're making a change like
this, say so explicitly rather than silently skipping the sync — don't assume the update is
someone else's problem. `roves-api` and `roves-wiki` (also sibling repos) are not part of
this mirroring relationship and don't need this same treatment unless a change here directly
affects what they document or consume.

## Ask whether a new `mach bundle` setting belongs in `roves-packmaster` too

`../roves-packmaster` (a sibling checkout, "Roves Packmaster") is a GUI that wraps `mach bundle`'s
own options as a settings screen a game developer clicks through instead of typing flags —
see that project's own `CLAUDE.md` and `src/lib/settings.ts` for its current settings shape
(portable platforms, installer formats, Steam, content compression).

**Any time you add, remove, or change the meaning of a `mach bundle` flag or default (see
the `CUSTOMIZATIONS.md`/patches section above — those same changes already need an entry
there), ask explicitly whether it should also be reflected in `roves-packmaster`'s settings** —
don't assume it does or doesn't; the answer depends on whether the new option is something
a game developer using the GUI would plausibly want to control, which isn't always obvious
from this side. If `../roves-packmaster` isn't present as a sibling checkout, say so explicitly
rather than silently skipping the question.

## CRITICAL: the wiki must always be kept up to date — for anything, not just features

`CUSTOMIZATIONS.md`/`patches/` (above) capture *what changed relative to upstream Servo*, for
the next version upgrade. That's a different audience from *someone using Roves right now* —
a game developer bundling their game, or a player hitting a problem — who reads
[`README.md`](./README.md) and the [`roves-wiki`](https://github.com/DRincs-Productions/roves-wiki)
docs site instead, and neither of those updates automatically just because
`CUSTOMIZATIONS.md` did.

**This isn't scoped to "new user-facing features" alone — read it as "for anything."** A new
`mach bundle`/`mach build` flag or bundle output is the clearest case, but the same same-turn obligation
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
the next person upgrading the Servo version; a real user has no reason to ever open that file.

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

`.github/workflows/release.yml` builds and publishes a real, versioned GitHub Release —
distinct from `.github/workflows/test.yml`'s rolling "test" release (see that file's own
comment for why it exists separately). It triggers on pushing a tag matching `v*.*.*` (e.g.
`v0.1.0`), builds directly from that tagged checkout of this repo — not a pristine-download-
plus-`patches/` reconstruction the way `test.yml` does, since this repo already tracks the
full patched source (see the top of this file) — and publishes a portable bundle per platform
(Windows/macOS/Linux, no `.msi`/`.dmg`/`.deb` installers) to that Release, named
`roves_shell_<platform>.zip`. The first cut, `v0.1.0`, is the engine shell only: no bundled
UI/game content (`roves-packmaster` content lands in a future release).

### To cut a release

1. Decide the version (semver), e.g. `0.1.0`.
2. Tag and push: `git tag v0.1.0 && git push origin v0.1.0` — this alone triggers
   `release.yml`; nothing else needs to happen by hand.
3. Watch the run: `https://github.com/DRincs-Productions/roves/actions/workflows/release.yml`.
4. Once green, verify what actually got published at
   `https://github.com/DRincs-Productions/roves/releases/tag/v0.1.0` — check all six zips are
   attached (Windows/macOS/Linux, plain and `_steam` variants) and
   the notes rendered as expected. Don't consider the release done on "the workflow went
   green" alone — confirm the artifacts are actually there.
5. Update the pinned shell version in `roves-action` and `roves-packmaster` (see the dedicated
   section below) — **the same turn**, not a follow-up. A release isn't finished just
   because the tag built; the sibling projects that consume this version are the whole
   reason it needed cutting in the first place.

### CRITICAL: after every release, sync the shell version in `roves-action` and `roves-packmaster`

The consumer checkouts can be sibling directories or submodules inside this repository
(`roves-action/`, `roves-packmaster/`, `roves-wiki/`). Locate the actual checkout before
reporting it missing. Their engine references do not update just because a tag is pushed:

- **`roves-action/action.yml`**: bump both fixed literals, the source-checkout `ref:` and
  the prebuilt-shell release URL, plus the README version. There is no `roves-ref` input
  anymore. This changes what consumers receive and needs a verified commit.
- **`roves-packmaster/src/lib/shell-version.txt`**: update the single shared pin. The frontend
  imports it through Vite's raw loader for `TARGET_SHELL_VERSION`, and Rust embeds it with
  `include_str!` for the actual download. Both trim whitespace. Update the README and check
  both paths; do not reintroduce separate literals. A frontend-only bump previously left
  production Rust downloads at v0.4.18 while the UI displayed v0.7.12.

`roves-api` (`../roves-api/src/version.ts`'s `COMPATIBLE_SHELL_VERSION`) is **not** part of
this same per-release obligation — it's a static compatibility note for consumers, not
something that drives build behavior or a live check, so bump it only when a shell change
actually affects `roves-api`'s own compatibility (a `roves:`/`steam:` protocol change), not
mechanically on every tag.

If `../roves-action` or `../roves-packmaster` isn't present as a sibling checkout when cutting a
release, say so explicitly rather than silently skipping this step — the same rule as every
other cross-repo sync obligation in this file.

### If a run fails: delete-and-republish loop

GitHub won't let a tag be re-pushed over itself, and `gh release create` (used by the
workflow's own `create-release` job) errors on an existing release of that name — retrying
means tearing both down and starting clean, not just re-running the failed job:

1. Delete the GitHub Release for that tag, if one was already created — via the web UI
   ("Delete this release") or `gh release delete vX.Y.Z --yes`. (`create-release` also
   self-heals this automatically on the *next* push of the same tag — see its own `gh
   release delete ... || true` step — so this is belt-and-braces, not strictly required
   before re-pushing.)
2. Delete the tag both locally and on the remote: `git tag -d vX.Y.Z && git push origin
   --delete vX.Y.Z`.
3. Fix whatever caused the failure.
4. Re-tag and re-push: `git tag vX.Y.Z && git push origin vX.Y.Z`.
5. Repeat until every one of the 3 matrix jobs is green and the release page shows all 3
   zips.

### Diagnosing a failure without `gh`/a token on hand

Raw step logs need repo-admin auth to download (the Actions API's job-logs endpoint 403s
"Must have admin rights to Repository" for an anonymous/public request) — but `::notice::`/
`::error::` **annotations** are readable anonymously via `GET /repos/DRincs-Productions/
roves/check-runs/{job_id}/annotations`. That's why the smoke-test step re-emits stdout/
stderr/`roves.log` as `::notice::` lines on a launch failure (mirroring `test.yml`'s own
reasoning, see its comments). A failure inside `mach build`/`mach bootstrap` itself (a
compile error, a GStreamer install failure) has no custom annotation, only the generic
"Process completed with exit code N" one — diagnosing those needs either `gh run view --log`
(if `gh`/a token is available) or someone with repo access checking the run's logs directly
at its `html_url`.

### Design notes worth knowing before touching this workflow

- **No `--content-dir`**: this first release is the engine shell only — revisit this once a
  future release adds real `roves-packmaster` content.
- **Portable only, no `--deb`/`--msi`/`--dmg`**: `--package-name`/`--package-version` only
  affect those installer formats (see `post_build_commands.py`), so they're dropped from
  `mach bundle` here too — not worth the extra CI surface for a shell-only build. See
  README.md's "Portable vs. installable packages" section if a future release adds them
  back.
- **Real GStreamer, not `--media-stack dummy`**: unlike `test.yml`, this release ships
  working audio/video. Linux and macOS get it for free (`mach bootstrap` already installs
  GStreamer non-interactively there — apt packages on Linux, `sudo installer -pkg` on macOS
  with `--yes`). **Windows needs a workaround**: `mach bootstrap`'s own GStreamer installer
  wraps `msiexec` in `Start-Process -verb runAs` (a UAC elevation prompt) that hangs forever
  on a non-interactive GH runner — see `test.yml`'s own comment on this exact, already-
  documented finding (that workflow sidesteps it by using `--media-stack dummy` instead of
  solving it). `release.yml` installs the same two MSIs itself, directly via `msiexec /a ...
  TARGETDIR=... /qn` (no elevation prompt), to the exact path `python/servo/platform/
  windows.py`'s `DEPENDENCIES_DIR` would use, then passes `--skip-platform` to `mach
  bootstrap` so it doesn't redundantly (and hang-prone-ly) try to install it again.
- **Every platform builds twice, plain and `--features steam`**: each of the 3 platforms
  gets a second build compiled with `--features steam` and published as a second asset
  (`roves_shell_<platform>_steam.zip`, alongside the existing plain `roves_shell_<platform>.zip`)
  — see the matrix's `steam`/`asset_suffix`/`build_features` fields. `mach bundle` itself
  needs no changes for this: Windows/Linux pick up the Steamworks redistributable
  automatically (`build.rs`'s `copy_steam_lib` already places it flat next to the binary,
  where the existing DLL/`.so`-copy step already looks), and macOS's `_bundle_macos` already
  special-cases `libsteam_api.dylib` correctly (see `CUSTOMIZATIONS.md`'s 2026-08-14 entry).
  This is what lets Roves Packmaster (`roves-packmaster`) offer a real Steam-enabled release without
  ever compiling anything itself — see that project's own `CLAUDE.md`, "Why no Steam plugin
  still" section (now stale as of whichever release first publishes the `_steam` assets;
  revisit that section once Packmaster's own Steam wiring lands).
- **`mach` needs `chmod +x` on Linux/macOS**: this repo's `mach` script is tracked in git as
  mode `100644` (committed from Windows, where the exec bit is meaningless) — a fresh
  Linux/macOS checkout needs it re-marked executable or `./mach bootstrap` fails immediately
  with "Permission denied" (exit 126). `release.yml` does this explicitly; if this ever
  regresses (e.g. a future commit re-adds `mach` without the bit), re-run `git update-index
  --chmod=+x mach`.
- **`mach` no longer needs `tests/wpt/tests/tools/`** (since 2026-10-14, patch 0216): the
  upstream `tidy`/`wpt` packages, `package`/`devenv` command modules and every
  `testing_commands.py` command except `test-unit`/`fmt` were deleted, so `bootstrap`,
  `build`, `bundle` and `test-unit` run from a plain checkout. The old sparse-checkout
  workaround is gone from every workflow and from `roves-action`. Deleted upstream paths are
  listed in `patches/servo-v0.5.0/REMOVED_PATHS.txt`; workflows that rebuild the tree from
  pristine Servo delete them after applying the patches.

## Upgrading to a newer Servo version (rough steps)

**Updated 2026-09-04, after actually doing the v0.4.0 → v0.5.0 migration** (see
`CUSTOMIZATIONS.md`'s entry of that date for the full write-up) — the steps below reflect what
actually worked, not the untested plan that preceded it. The single biggest lesson: **do not
try to sequentially reapply `patches/servo-v<old>/*.patch` against the new tag with
`patch`/`git apply`.** It looks like the obvious approach, but cascades — the moment one patch
fails to apply (upstream moved the code it targets), every later patch touching the same file
fails too, even when its own hunk would have applied fine on its own. `post_build_commands.py`
alone is touched by more than a dozen old patches; one early failure there poisons the rest.

1. Get a full clone of the new tag (`git clone --depth 1 --branch v<NEW> https://github.com/servo/servo.git`
   into a scratch directory, sibling to this repo — do **not** overwrite this directory
   directly) — a real git repo, not just an extracted zip, is what step 3 needs.
2. **Per-file 3-way merge, not sequential patch replay.** For every file `patches/servo-v<old>/`
   touches (`grep -h "^diff --git" patches/servo-v<old>/*.patch | sed ...` to get the list),
   run a 3-way merge with `git merge-file`: base = the pristine old-tag version of the file
   (this repo's own root commit, `git log --oneline | tail -1` finds its hash — **not** the
   `v<old>` tag, which gets reused for this project's own versioned releases and no longer
   points at the pristine import, see the warning above), ours = this repo's current
   fully-patched version of the file, theirs = the new tag's pristine version (`git show
   v<new>:<path>` from the scratch clone). This reconciles upstream's evolution against the
   *entire* accumulated customization for that file in one step, regardless of how many old
   patches touched it — no cascading, and files that merge with zero conflicts need no manual
   attention at all. Files with real conflicts (upstream and this fork's own patches touching
   the exact same region) need hand resolution — read both sides' diffs from base, understand
   each one's intent, reconcile by hand.
3. **Watch for files patches never touched at all.** Two categories, both real: (a) genuinely
   new binary/text-placeholder assets no unified diff can carry (icons, fonts, the OpenHarmony
   symlink-placeholders — CUSTOMIZATIONS.md's "not part of any patch" mentions collect the
   full list as of the last migration) — the 3-way merge above skips these since they have no
   patched history to diff against; carry them over by hand (`git checkout <old-tag-commit> --
   <path>` from this repo's own history) whenever the new tag's own copy differs. (b) Files
   whose *only* difference is something a Windows checkout mangles that `cargo build` doesn't
   care about but `git diff`/`patch` do: the executable bit (NTFS has no concept of it — lost
   on every file touched by a plain recursive copy if no `rsync` is available) and git-tracked
   symlinks (a Windows `cp` dereferences them into a plain-file copy of the target's actual
   bytes, not the symlink's own tiny "relative path" content) both showed up as real diffs
   against pristine that were pure copy-mechanism noise, not customizations — verify with
   `git diff <old> <new> -- <path>` per file that looks suspiciously large-but-boring before
   deciding it needs a patch at all.
4. Re-reason about the default-on experimental prefs (`CUSTOMIZATIONS.md`'s "Default-on
   experimental web platform features" entry — as of the v0.5.0 migration, folded into
   `patches/servo-v0.5.0/0011-storage-and-origin.patch`, not its own numbered patch anymore;
   check that file's own header comment in `CUSTOMIZATIONS.md`'s migration entry for whichever
   patch currently owns `components/config/prefs.rs`) — **don't just mechanically reapply the
   current field list.** It's a snapshot of one Servo version's `EXPERIMENTAL_PREFS`
   (`ports/servoshell/prefs.rs`) and `Preferences` struct (`components/config/prefs.rs`), not
   a fixed policy. The new tag's versions of both will likely differ — new prefs added, some
   removed, some graduated from experimental to stable (already `true` upstream, so nothing to
   do). For every pref that's new in either place compared to the old tag, reason about it the
   same way that entry did: would a real video game plausibly want this (graphics/audio/input/
   storage/UI capability), or is it dev-tooling, testing-only, or unrelated to running a game?
   Default the former to `true` here, leave the latter alone, and update the entry's field
   list and reasoning to match — don't leave it describing the previous version's set.
5. Swap the new, merged tree in for this directory — same exclusion list as before (don't
   touch `.github/`, `CUSTOMIZATIONS.md`, `patches/`, `README.md`, `TODO.md`, `AGENTS.md`, the
   sibling project checkouts, or anything else this repo adds on top of vanilla Servo).
6. Regenerate `patches/servo-v<new>/` from the merged result (`git diff <pristine-new-tag> --
   <files>`, grouped however makes sense — the v0.5.0 migration grouped by
   subsystem/directory instead of by logical change specifically so a file only ever needs
   reconciling in one place on the *next* upgrade too) and verify each one applies cleanly to
   a fresh pristine extraction of the new tag before moving on.
7. Update `SERVO_TAG` in `test.yml`/`android.yml` (this directory's own, plus
   `../.github/workflows/embedded.yml` if that's ever wired to the patched build instead of
   upstream's stock binary — see TODO.md #1) — but only *after* step 6, since those workflows
   look for `patches/servo-v${SERVO_TAG}/*.patch` and will fail outright if that directory
   doesn't exist yet for the new tag.
8. **Verify via CI too, not only a local build** — a local Windows `cargo build`/`./mach build`
   toolchain is set up and confirmed working as of 2026-09-29 (LLVM `lld-link.exe`/`libclang`,
   MSVC Build Tools, real GStreamer, `depot_tools`), so `./mach build --release` locally is a
   real option for fast iteration on this one platform. It still doesn't replace CI: push to a
   branch and either wait for `test.yml`'s own path-triggered run (touching `patches/**`
   triggers it) or watch a manual `workflow_dispatch` run — it exercises the real
   reconstruction path (download pristine + apply `patches/servo-v<new>/` + `mach build` +
   `mach bundle`) end to end, across all 3 platforms, which is a stronger check than anything
   achievable locally on one machine anyway. Don't stop at "it compiled" — a clean compile
   doesn't catch a merge that silently dropped an import your own code needed (see the
   migration entry's own "two real bugs" writeup) if nothing else in the file happened to also
   reference that symbol; only an actual build catches that.
9. Update `CUSTOMIZATIONS.md` with the new baseline version at the top, and this file's own
   intro paragraph (which names the currently-tracked tag).
