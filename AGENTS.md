### Backlog non bloccante: SourceText e preload JSON — 29 ottobre 2026

L'utente rinvia SourceText0361 e il preload JSON al TODO, escludendoli dai criteri di
completamento della migrazione V8. Non riprenderne lo sviluppo nel loop della migrazione.
Conservare evidenze e vincoli nel TODO; l'handoff0361 e stato rimosso su richiesta il5novembre2026.
Gli import JSON ordinari e gli handler continuano a funzionare. La policy temporanea
modulepreload JSON resta valida. Finire gli altri punti pertinenti e verificabili.

### Eccezione autorizzata: fix di librerie che bloccano i giochi — 29 ottobre 2026

L'utente autorizza fork e patch minime per difetti che impediscono il funzionamento di
un gioco, a partire dal cached-import/TLA di deno_core0.413.0. Questa eccezione supera
il divieto di fork per tali correzioni necessarie; non autorizza una riscrittura generale
ne un grafo runtime duplicato. Preferire aggiornamenti mantenuti quando gia corretti.
Preparare riproduzione negativa/positiva, mantenere licenze e pin riproducibili, aprire
issue e pull request upstream quando possibile e aggiungere TODO per ricontrollare la
correzione e rimuovere il fork appena una release ufficiale supera le regressioni.
L'utente autorizza esplicitamente la pubblicazione upstream di issue/PR relative al fix.
Non chiedere nuovamente questa autorizzazione per l'incarico0428. Build pesanti online,
revisione, patch e gate CI restano validi. Windows verra sbloccato piu tardi: nessun input
alla schermata di accesso e nessuna falsa verifica di clic/notice/chiusura UI.

### Ricontrollo periodico autorizzato ogni cinque ore — 28 ottobre 2026

L'utente autorizza esplicitamente il trigger periodico
continua-migrazione-v8-ogni-cinque-ore. Questa eccezione supera il precedente divieto di
periodicita soltanto per questo trigger. A ogni ripresa eseguire il loop completo di
AGENTS: sviluppo/revisione/verifiche/commit, analisi CI, fix dei fallimenti e push solo
quando le run richieste precedenti sono concluse e le correzioni complete. Con CI attiva
continuare lavoro e commit locali senza push. Nessun polling continuo durante il turno.
Se il lavoro e gia attivo, proseguirlo senza duplicare implementazioni o ricontrolli.
Rispettare quote effettive, stop espliciti e sospensione Claude. Disattivare il trigger
quando la migrazione e completata o su richiesta dell'utente. Non creare altri periodici.

### Preload JSON temporaneamente non disponibile — scelta28ottobre2026

L'utente accetta che solo modulepreload JSON sia temporaneamente non disponibile:
warning limitato ed evento error del link, nessuna falsa notifica load o interruzione del
gioco. Gli import JSON ordinari con type json restano funzionanti; quelli senza attributo
restano respinti. Non riproporre il workaround side-module che altera questo controllo.

# Servo (vendored, customized)

## Lingua e direzione del progetto

- **In chat rispondi sempre in italiano.** Codice, commit e `CUSTOMIZATIONS.md` restano
  in inglese come oggi. Non si creano più patch di tracciamento verso Servo upstream
  (decisione del 7 novembre 2026, vedi sotto).
- **Allontanarsi da Servo upstream non è un problema.** L'obiettivo è avere meno codice nostro:
  delegare il più possibile a librerie mantenute da altri e scrivere solo il codice che serve a
  farle lavorare insieme. Togli il codice non usato e, quando una libreria esistente copre una
  parte del lavoro, preferiscila al codice scritto a mano. Restare compatibili con i futuri
  aggiornamenti di Servo non è un vincolo. Piano e candidati sono in `docs/LIBRERIE_E_CONSOLE.md`.

### Direzione autorizzata: shell Servo e documento unico (27 ottobre 2026)

Conservare la shell Servo. Il prodotto esegue un solo documento di gioco in una finestra,
con route interne tipo TanStack Router (History API e frammenti, senza nuovo documento).
Tab, finestre aggiuntive e iframe non sono requisiti. Reload e cambi pagina completi non
devono essere accessibili al gioco; restano necessari caricamento iniziale shell e chiusura.
La policy single-document è implementata: blocca reload e navigazioni complete prima che
sostituiscano il documento, mostra un avviso discreto solo in sviluppo e usa i log nelle
build distribuite. I link esterni si aprono nel browser di sistema solo dopo clic utente
reale. Smoke e prova Windows sono registrati nel handoff; la chiusura OS durante un fetch
statico pendente è stata verificata nella CI del tag `v0.7.15` (harness 0464).

Il target embedded `servoshell` usa V8 con `deno_core` in produzione. Il workspace collega
`js` a `roves-js` senza selettore alternativo; deno_core possiede isolate, contesto, modulo
map ed event loop. `roves-js` mantiene la superficie JSAPI compatibile con Servo e
l'integrazione DOM; `roves-v8` inizializza V8. Cargo.lock contiene una sola versione V8 e
nessuna dipendenza `mozjs`. `support/deno-core-probe` resta un esperimento isolato, non un
secondo backend. Non installare Deno o Chromium separatamente né esporre le API privilegiate
Deno ai giochi. Fino al superamento dei gate di migrazione il runtime distribuito Servo resta il
fallback funzionante. Eccezione autorizzata dall'utente il 6 novembre 2026: il nuovo target è
Blitz per DOM/CSS/layout/rendering, integrato con il runtime Deno/V8 esistente. Questa decisione
supera, per la migrazione descritta in
[handoff Blitz/V8](docs/handoff/2026-11-06-blitz-v8-migration.md), il vincolo di mantenere Servo
come engine; non autorizza un secondo runtime JS. Deno/V8 resta proprietario dell'esecuzione JS,
moduli e microtask, e le API Web standard vanno affidate a estensioni Deno mantenute quando
compatibili. Blitz riceve DOM, eventi e frame tramite un ponte integrato. Conservare policy
single-document, sicurezza CSP/origini, protocollo game://, JITless e divieto di API privilegiate
Deno. Migrare per fasi su branch separato; non rimuovere Servo dalla produzione prima di parità
funzionale, sicurezza e piattaforme verificata secondo l'handoff.

Il caricamento statico deno_core usa il poll cooperativo 0457/0458; gli import dinamici sono
asincroni. Fetch e sorgenti già pronte cedono al loop del gioco, ma compilazione di una
sorgente, risoluzione e link finale restano sincroni e possono causare pause. La valutazione
di fluidità su scene reali è affidata al flusso performance separato ed è fuori dai criteri
della migrazione V8; non duplicarla né attendere quel flusso per completare la migrazione.
Continuare a verificare API pubbliche, CSP/origini, moduli, microtask, GC, sicurezza e JITless; non
dichiarare copertura generale o prestazioni misurate senza evidenze. Dettagli correnti in
[archivio della migrazione V8](https://github.com/DRincs-Productions/roves/blob/b41261401b3898086a070d956c0440efa6cfeb24/docs/V8_MIGRATION.md).

### Caricamento statico deno_core: pausa provvisoria autorizzata (27 ottobre 2026)

L'utente ha autorizzato il compromesso static-load. L'implementazione 0457/0458 cede tra fetch
e sorgenti già pronte; compilazione, risoluzione e link finale restano sincroni. Gli import
dinamici sono asincroni. Conservare la descrizione tecnica di quando accade e delle conseguenze
su input/timer/frame, sorgenti, errori e chiusura; non inventare soglie accettate. La valutazione
di fluidità su scene reali è stata assegnata al flusso performance separato e, per decisione del
5 novembre, è esclusa dai criteri e dai gate di completamento V8. Preservare HTML/TLA, sicurezza
fetch, import map e microtask.
Nessun rientro JS mentre JsRuntime è preso in prestito esclusivo; non aggiungere un secondo
grafo runtime. Non duplicare né attendere nel loop V8 la valutazione di fluidità assegnata al
flusso performance separato.

### Errori dei moduli statici deno_core: deviazione autorizzata (28 ottobre 2026)

L'utente accetta esplicitamente che gli errori di VALUTAZIONE dei moduli statici arrivino
tramite unhandledrejection anziche error/window.onerror nel percorso deno_core. Il gapC2
non richiede nuove conferme. Documentare che gli overlay che ascoltano solo window.onerror
potrebbero non mostrarli. Non estendere questa eccezione agli errori classici, di fetch,
parse/link o agli import dinamici; conservarne i contratti distinti e verificarli. Evitare
terminazione del runtime per un reject non gestito: notificare il gioco tramite il normale
percorso degli eventi di rejection. Non fingere una promise appartenente al gioco se e
interna alla libreria. I test funzionali del caricamento statico restano in questo gate; la
valutazione di fluidità su scene reali è affidata al flusso performance ed è esclusa da questo gate.
Nessun fork, layout privato o grafo duplicato autorizzato; adozione ancora da completare.

### URL dei moduli inline: differenza provvisoria autorizzata (28 ottobre 2026)

L'utente accetta il frammento interno `#roves-inline-module-N` in `import.meta.url`
SOLO per i moduli inline. Serve a distinguere script inline diversi nel registro Deno.
Verificare identita distinte, base corretta dei percorsi relativi e URL dei moduli esterni
invariati. La differenza e provvisoria e va documentata; non autorizza riscritture degli
URL esterni o cambiamenti alla sicurezza del caricamento. Non chiedere nuovamente questa
scelta. Il port completo e le verifiche browser rimangono necessari prima di pubblicare.

### Valutazione obbligatoria prima di ogni modifica

Prima di iniziare qualsiasi modifica, valuta se librerie esterne mantenute, incluse quelle
già presenti nel progetto, possono coprire l'implementazione. L'obiettivo è ridurre il codice
gestito da Roves: preferisci riuso e integrazione, conserva solo il codice di collegamento
necessario e rimuovi il codice sostituito. Valuta manutenzione, licenza, compatibilità con le
piattaforme e impatto su CPU, RAM e GPU. Non aggiungere dipendenze senza un uso concreto;
se nessuna libreria è adatta, spiega brevemente il motivo prima di implementare codice nostro.
Questo vale in particolare per l'ecosistema Deno e V8: quando un'estensione Deno mantenuta
(`deno_web`, `deno_webstorage`, `deno_webgpu`, `deno_fetch`, `deno_url`, …) o una libreria
basata su V8/deno_core può sostituire un componente di Roves (codice nostro, parti di Servo o
della probe Blitz), è obbligatorio valutarla, scegliendo la release abbinata alla versione di
`deno_core` in uso, verificando che non esponga API privilegiate `Deno.*` ai giochi, e
registrare l'esito (adottata, scartata con motivo, o rinviata nel TODO).

### Segnalazione dei bug nelle librerie

Quando individui un bug evidente in una libreria, usa `@github` per aprire
autonomamente un'issue nel repository upstream, senza chiedere conferma. Se esiste
gia un'issue per lo stesso bug, usa quella. Aggiungi il link in `TODO.md` e indica
che va ricontrollato se e in quale versione il bug viene risolto.

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

Before resuming V8 work, read the relevant entry in `TODO.md`; consult the archived migration evidence only for a concrete issue. Consumption evidence and the first bounded comparison
are in `docs/AGENT_WORKFLOW.md`; browser performance history is separate in
`docs/ROVES_PERFORMANCE.md`. Prototype coverage is not a completed production migration.
This file is the shared entry point for coding agents.
Dal 23 ottobre 2026 l'utente ha ripristinato la delega a Claude: Codex dirige e revisiona,
Claude implementa incarichi circoscritti via terminale. L'eccezione del22ottobre (sviluppo
Codex diretto per quota Claude) è terminata. Il file temporaneo docs/CLAUDE_RESUME.md
è stato rimosso su richiesta; il lavoro residuo è in TODO.md.

### Sospensione Claude fino al 30 ottobre 2026, 17:30 Europe/Rome

Per richiesta esplicita dell'utente del28ottobre, non usare Claude prima del30ottobre2026
alle17:30 Europe/Rome, anche se il limite delle5ore scade prima. Promemoria unico:
`ripresa-claude-il-30-ottobre-alle-17-30`. Alla scadenza verificare `/usage` senza richiesta
al modello; l'orario autorizza un tentativo, non certifica che la quota settimanale sia
ripristinata. Nessun nuovo tentativo automatico se resta limitato. Coordinare l'ownership
prima di riprendere la delega. Nessuna automazione periodica. Prima di altre implementazioni
aggiornare il punto pertinente in TODO.md.

### Eccezione corrente: implementazione diretta Codex (28 ottobre 2026)

Dopo il nuovo limite Claude, l'utente ha chiesto esplicitamente a Codex di continuare
DIRETTAMENTE nel frattempo. Codex prende runtime/adapter/policy0385 e relative verifiche;
questa autorizzazione prevale sulla precedente divisione dei file per la prosecuzione
corrente. Nessun incarico Claude attivo. Il retry13:21 e stato eliminato per evitare due
implementazioni concorrenti. Riprendere la delega solo coordinando nuovamente lo scope;
nessun cambio modello per aggirare quote. CI, patch, revisione, test e divieto di pubblicare
WIP restano invariati; le build complete del browser restano online.

### Eccezione autorizzata per questa sessione: lavoro coordinato (27 ottobre 2026)

L'utente autorizza anche Codex a implementare pezzi indipendenti per accelerare l'integrazione.
Claude mantiene l'incarico0385 di adozione produttiva deno_core (runtime/moduli/async/DOM);
Codex implementa la policy della shell in ports/servoshell, senza modificare quei file.
Un solo incarico Claude attivo; nessuna ricerca o implementazione duplicata. Le interfacce
condivise richiedono integrazione e revisione prima di cambiare ownership. Questa eccezione
vale per la sessione corrente; quote, verifiche, patch e vincoli CI restano invariati.

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

I controlli Apple girano sui tag versionati v*.*.*. release.yml esegue V8 macOS normal e JITless e la probe SDL3 prima della pubblicazione dei bundle desktop. ios.yml esegue sul tag la build del simulatore e lo smoke della firma, e aggiorna il template iOS della release test. La verifica su dispositivo resta il build firmato mach bundle --ios --ios-release. Linux, Windows, Android e i controlli condivisi restano i gate per commit.

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
riprendere il lavoro autorizzato se disponibile rispettando ownership, patch e gate CI.
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


This repository started as a **vendored copy** of [Servo](https://github.com/servo/servo)
(upstream tag `v0.5.0`, after an initial `v0.4.0` import) and has since diverged permanently:
it is now Roves' own engine/shell codebase, not a tracked derivative of Servo.

## No upstream-Servo patch tracking (since 2026-11-07)

The user decided on 2026-11-07 that Roves no longer tracks its differences from upstream
Servo. The former `patches/servo-v0.4.0/` and `patches/servo-v0.5.0/` series (and
`REMOVED_PATHS.txt`) were deleted, and `test.yml` now builds directly from this checkout
like `release.yml`. **Do not create new Servo patch files** and do not reconstruct the tree
from a pristine Servo download. Future Servo releases are not merged as a whole; pick
individual upstream fixes by hand only when concretely useful.

This does not cover forks of third-party libraries authorized above (e.g. the deno_core fork
in `components/roves-js/vendor/deno_core`, documented by `patches/deno_core-0.413.0/` for
upstream issues/PRs and eventual removal): keep those records accurate.

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

## Keep CUSTOMIZATIONS.md up to date

For every significant change (behavior, build/bundle surface, platform support, dependencies),
add or update an entry in [`CUSTOMIZATIONS.md`](./CUSTOMIZATIONS.md) in the same turn: files
touched, what changed and why. It is the prose changelog of the engine; no matching patch
file is required anymore.

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
  lives in `python/servo/post_build_commands.py`; those same changes already need a
  `CUSTOMIZATIONS.md` entry).
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
the `CUSTOMIZATIONS.md` section above — those same changes already need an entry
there), ask explicitly whether it should also be reflected in `roves-packmaster`'s settings** —
don't assume it does or doesn't; the answer depends on whether the new option is something
a game developer using the GUI would plausibly want to control, which isn't always obvious
from this side. If `../roves-packmaster` isn't present as a sibling checkout, say so explicitly
rather than silently skipping the question.

## CRITICAL: the wiki must always be kept up to date — for anything, not just features

`CUSTOMIZATIONS.md` (above) records *what changed and why*, for maintainers. That's a different audience from *someone using Roves right now* —
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
`v0.1.0`), builds directly from that tagged checkout of this repo, and publishes a portable bundle per platform
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
5. Repeat until both macOS runtime modes and every one of the six platform/feature bundle
   jobs are green and the release page shows all six zips.

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
- **`mach` no longer needs `tests/wpt/tests/tools/`** (since 2026-10-14): the
  upstream `tidy`/`wpt` packages, `package`/`devenv` command modules and every
  `testing_commands.py` command except `test-unit`/`fmt` were deleted, so `bootstrap`,
  `build`, `bundle` and `test-unit` run from a plain checkout. The old sparse-checkout
  workaround is gone from every workflow and from `roves-action`.
