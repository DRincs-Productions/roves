### Preload JSON temporaneamente non disponibile — scelta28ottobre2026

L'utente accetta che solo modulepreload JSON sia temporaneamente non disponibile:
warning limitato ed evento error del link, nessuna falsa notifica load o interruzione del
gioco. Gli import JSON ordinari con type json restano funzionanti; quelli senza attributo
restano respinti. Non riproporre il workaround side-module che altera questo controllo.

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

### Sospensione Claude fino al 30 ottobre 2026, 17:30 Europe/Rome

Per richiesta esplicita dell'utente del28ottobre, non usare Claude prima del30ottobre2026
alle17:30 Europe/Rome, anche se il limite delle5ore scade prima. Promemoria unico:
`ripresa-claude-il-30-ottobre-alle-17-30`. Alla scadenza verificare `/usage` senza richiesta
al modello; l'orario autorizza un tentativo, non certifica che la quota settimanale sia
ripristinata. Nessun nuovo tentativo automatico se resta limitato. Coordinare l'ownership
prima di riprendere la delega. Nessuna automazione periodica. Prima di altre implementazioni
consegnare il handoff dettagliato in docs/V8_MIGRATION.md chiesto dall'utente.

### URL dei moduli inline: differenza provvisoria autorizzata (28 ottobre 2026)

L'utente accetta il frammento interno `#roves-inline-module-N` in `import.meta.url`
SOLO per i moduli inline. Serve a distinguere script inline diversi nel registro Deno.
Verificare identita distinte, base corretta dei percorsi relativi e URL dei moduli esterni
invariati. La differenza e provvisoria e va documentata; non autorizza riscritture degli
URL esterni o cambiamenti alla sicurezza del caricamento. Non chiedere nuovamente questa
scelta. Il port completo e le verifiche browser rimangono necessari prima di pubblicare.

### Eccezione corrente: implementazione diretta Codex (28 ottobre 2026)

Dopo il nuovo limite Claude, l'utente ha chiesto esplicitamente a Codex di continuare
DIRETTAMENTE nel frattempo. Codex prende runtime/adapter/policy0385 e relative verifiche;
questa autorizzazione prevale sulla precedente divisione dei file per la prosecuzione
corrente. Nessun incarico Claude attivo. Il retry13:21 e stato eliminato per evitare due
implementazioni concorrenti. Riprendere la delega solo coordinando nuovamente lo scope;
nessun cambio modello per aggirare quote. CI, patch, revisione, test e divieto di pubblicare
WIP restano invariati; le build complete del browser restano online.

### Errori dei moduli statici deno_core: deviazione autorizzata (28 ottobre 2026)

L'utente accetta esplicitamente che gli errori di VALUTAZIONE dei moduli statici arrivino
tramite unhandledrejection anziche error/window.onerror nel percorso deno_core. Il gapC2
non richiede nuove conferme. Documentare che gli overlay che ascoltano solo window.onerror
potrebbero non mostrarli. Non estendere questa eccezione agli errori classici, di fetch,
parse/link o agli import dinamici; conservarne i contratti distinti e verificarli. Evitare
terminazione del runtime per un reject non gestito: notificare il gioco tramite il normale
percorso degli eventi di rejection. Non fingere una promise appartenente al gioco se e
interna alla libreria. La pausa statica provvisoria e i suoi test restano un tema separato.
Nessun fork, layout privato o grafo duplicato autorizzato; adozione ancora da completare.

### Eccezione autorizzata per questa sessione: lavoro coordinato (27 ottobre 2026)

L'utente autorizza anche Codex a implementare pezzi indipendenti per accelerare l'integrazione.
Claude mantiene l'incarico0385 di adozione produttiva deno_core (runtime/moduli/async/DOM);
Codex implementa la policy della shell in ports/servoshell, senza modificare quei file.
Un solo incarico Claude attivo; nessuna ricerca o implementazione duplicata. Le interfacce
condivise richiedono integrazione e revisione prima di cambiare ownership. Questa eccezione
vale per la sessione corrente; quote, verifiche, patch e vincoli CI restano invariati.

## Direzione autorizzata — documento unico e servizi sostituibili (27 ottobre 2026)

Conservare la shell Servo: una finestra, un documento, route interne tipo TanStack Router.
History API/back/forward e frammenti non sostituiscono il documento. Niente requisito di
tab, nuove finestre o iframe; reload e cambi pagina completi da gioco sono fuori perimetro.
Policy da implementare nel motore: rifiuto prima di cambiare documento/stato, risultato API
coerente e warning almeno nei log/console con operazione/motivo. Limitare ripetizioni; niente
crash o alert modali per notificare il rifiuto. Avviso visibile eventualmente non modale.
Decisioni confermate: avviso discreto non modale in sviluppo, solo log nel distribuito;
link esterni nel browser di sistema solo su clic reale, automatici bloccati con warning.
Prima prova isolata deno_core a documento unico, poi policy/diagnostica col runtime scelto.
Caricamento iniziale shell/chiusura rimangono. Non presentare la policy come già implementata.

Servizi Servo possono essere sostituiti e rimossi. Rivalutare deno_core con un solo contesto:
il NO-GO0378 per frame multipli/WindowProxy valeva per il perimetro precedente e non blocca
questa nuova prova. Verificare DOM/eventi, sicurezza, microtask/moduli/GC e JITless; una sola
implementazione del servizio, API mantenute, niente fork nostro, privilegi Deno ai giochi o
installazioni separate. Nessun risparmio o adozione dichiarati senza evidenza. Non rimuovere
fixture esistenti prima di definire le prove sostitutive. Handoff0380 descrive comportamento
e verifiche attesi. Codex dirige/revisiona, Claude implementa; quote e regole CI invariati.

### Caricamento statico deno_core: pausa provvisoria autorizzata (27 ottobre 2026)

L'ultima istruzione dell'utente autorizza a procedere subito: durante il caricamento e
l'istanziamento dei moduli statici input e timer del gioco possono fermarsi temporaneamente.
Gli import dinamici restano asincroni. Questa decisione sostituisce la precedente scelta2.
E un compromesso PROVVISORIO DA RIVEDERE nei test, non una prestazione accettabile gia
misurata: documentare quando accade (avvio e script module aggiunti a gioco avviato), durata,
sorgenti locali/rete, input/timer/frame e gestione errori/chiusura. Confrontare caricamenti
freddi/caldi e rete lenta controllata; non inventare durata o soglie accettate dall'utente.
Preservare valutazione HTML/TLA, sicurezza fetch, import map e microtask. Nessun rientro JS
mentre JsRuntime e preso in prestito esclusivo; nessun fork o grafo duplicato permanente.
Usare API pubbliche Deno/Tokio. La bozza salvata0385 puo essere ripresa e revisionata;
nessun backend parziale va pubblicato. Se i test mostrano pause problematiche riaprire
la decisione e rivalutare un'API pubblica interamente asincrona.

## Eccezione Apple autorizzata il 26 ottobre 2026

Per la prosecuzione V8 l'utente esclude macOS/iOS dal blocco di pubblicazione. Non sono
verifiche superate: documentarle come escluse/mancanti, senza disattivare workflow o
ritentare per la sola piattaforma Apple. Tutti i job non Apple necessari devono restare
verdi e terminali prima del push. Questa eccezione prevale sulla regola generale di batch
interamente verde solo per macOS/iOS; il loop di sviluppo e gli altri limiti restano validi.

## Loop operativo autonomo

La procedura in AGENTS.md è ciclica: definire un incarico, implementare e verificare,
revisionare fino al completamento, aggiornare MD/patch e committare localmente, controllare
il batch CI e pubblicare solo quando consentito. Dopo ogni incarico o push si passa al
prossimo lavoro utile senza conferme ordinarie. Con CI attiva si sviluppa e si committa,
ma non si pusha; eventuali fallimenti vanno corretti nel prossimo batch. Restano validi
stop utente, quote effettive, blocchi non risolvibili e assenza di lavoro utile; nessun
polling continuo. I dettagli e la divisione Codex/Claude sono in AGENTS.md.

## Ripresa dopo limite Claude — regola aggiornata il 25 ottobre 2026

Quando Claude segnala una quota esaurita, salvare l'incarico e verificare la quota con
`/usage` nella CLI, senza richieste al modello. Per il limite della sessione di 5 ore,
pianificare un solo tentativo all'orario di reset indicato più 2 minuti, rispettando data
e fuso orario della CLI. Questa regola sostituisce le 5 ore dal messaggio di blocco;
le vecchie scadenze riportate nello storico non sono istruzioni correnti.
Se il reset è assente o ambiguo, non inventare una scadenza. Nessun polling intermedio
o duplicato: aggiornare il ricontrollo esistente. Un orario esplicito dell'utente prevale.
Disattivare dopo il tentativo; se lo stesso limite persiste, fermarsi senza retry concatenati.
Restano esclusi retry automatici per quote settimanali/mensili, stop utente e cambi modello.
Restano in vigore revisione, verifiche e vincoli CI di AGENTS.md.

## Prosecuzione durante la CI

Con run attive e nessuna fallita, continuare il lavoro utile autorizzato, revisionare e
committare in locale senza push. Non terminare il ciclo soltanto perché la CI è in corso.
Controllare il batch al completamento delle implementazioni; pubblicare secondo AGENTS.md.

## Tentativo18:45 eseguito — 23 ottobre 2026

Claude disponibile:0315 consegnato con test positivi/negativi e replay. Codex ha revisionato
le prove e il diff; nessuna ripetizione senza motivo. Ricontrollo one-shot eliminato dopo
l'esecuzione. La precedente quota è storica, non un blocco corrente. Prossimo incarico:
deriva patch preesistente rilevata dal replay completo, mantenendo un solo Claude attivo.

## Delega ripristinata — 23 ottobre 2026

L'utente ha ripristinato Codex direzione/revisione e Claude implementazione via terminale.
Un solo incarico Claude attivo, nessuna implementazione duplicata, nessun tetto numerico o
contabilità ordinaria. L'eccezione Codex diretto del22ottobre è terminata. Handoff unico:
docs/V8_MIGRATION.md; il file temporaneo CLAUDE_RESUME.md è stato rimosso su richiesta.
Il lavoro remoto di Claude è stato acquisito con fast-forward fino a8e4fb2e6cf4 (wiki b86bb38).
La CI generale/Android/iOS è verde; V8 fallisce nei due smoke Linux con main non definita
in gc-lifecycle.html. Correzione mirata delegata; pubblicare dopo revisione e batch concluso. La prima invocazione0315 si è
fermata per quota effettiva (reset CLI18:40 Europe/Rome23ottobre) prima di verifiche/replay.
Bozza archiviata sotto docs/handoff, sorgenti ripristinati. Il blocco iniziale è storico: l’utente ha poi richiesto il tentativo delle18:45 e autorizzato
la regola5ore descritta in cima. Nessun cambio modello/sviluppo
Codex sostitutivo per aggirare il limite.

# Regola corrente — aggiornamento 22 ottobre 2026

L'utente ha eliminato i tetti di25/50 chiamate e l'obbligo di registrare consumo e lavoro
residuo per incarico. Codex continua autonomamente con un solo Claude attivo, incarichi
circoscritti, revisione e vincoli CI; si ferma soltanto per quota effettiva, stop esplicito
o blocco non risolvibile. Nessuna nuova contabilità di chiamate/turni/token. Le misure e
le soglie nelle sezioni seguenti sono storiche, non regole correnti.

# Consumi e coordinamento degli agenti

## Stato e obiettivo

Il 22 ottobre 2026 l'utente ha fermato la migrazione e approvato la priorità **minimo consumo
complessivo**. Nessun rapporto rigido 50/50: token e quote dei due abbonamenti non sono
unità direttamente confrontabili. La ripresa è stata autorizzata; questo paragrafo conserva il contesto storico.
L'automazione `migrazione-v8-ci-e-delega-claude` è stata eliminata; nessun nuovo monitor previsto.

## Evidenze della sessione del 22 ottobre

Fonte: record finali `type=result` dei cinque log locali `target/claude-*.log` indicati sotto.
I token cache letti sono cumulativi fra richieste: non rappresentano altrettanti token unici.
Il costo è quello indicativo a listino riportato dal client, non una fattura o la conversione
in crediti dell'abbonamento. Nei record stream l'output parziale sottostima il totale:
per questi conteggi è stato usato il record finale, senza sommare copie dello stesso messaggio.

| Incarico / log | Turni | Token output | Token cache letti | USD indicativi |
| --- | ---: | ---: | ---: | ---: |
| 0269 / claude-0269-oct22.log | 85 | 51.745 | 6.776.451 | 3,6162 |
| 0270 / claude-0270-module-private.log | 88 | 56.962 | 9.927.231 | 4,6834 |
| 0271 / claude-0271-host-referrer.log | 100 | 84.055 | 14.672.382 | 6,5647 |
| 0272 / claude-0272-traced-owner.log | 124 | 73.194 | 17.996.233 | 6,8699 |
| 0273 / claude-0273-native-traps.log | 49 | 32.904 | 4.493.603 | 2,6278 |
| **Totale** | **446** | **298.860** | **53.865.900** | **24,3620** |

Sono stati inoltre scritti nella cache 951.098 token. Il contesto iniziale dei cinque incarichi
andava da 44.496 a 45.802 token; alcuni contesti successivi superavano 240.000 token.
Ogni incarico ha prodotto circa 157.000–280.000 caratteri di output degli strumenti;
alcuni singoli output superavano 30.000 caratteri. Tutti i record indicano claude-opus-5-5.
Questi dati non spiegano da soli la conversione nelle percentuali di quota dell'account.

Non è disponibile un conteggio affidabile attribuibile a Codex per la stessa finestra.
Non è quindi possibile quantificare il costo totale dei due servizi o dimostrare un
risparmio percentuale rispetto a uno sviluppo equivalente svolto soltanto da Codex.

## Diagnosi

Non è stato dimostrato un loop infinito di richieste Claude: i cinque processi sono finiti,
l'ultimo per limite di sessione. Il ciclo di sviluppo era però aperto a incarichi successivi
senza budget per incarico. Le richieste di implementazione includevano ricerca, test positivi
e negativi, replay delle patch e documentazione: erano più ampie di quanto suggerisse la
qualifica di incarichi circoscritti. Il contesto crescente veniva riprocessato molte volte.

Codex ha aggiunto controlli CI/processo frequenti, letture estese di output e analisi di
soluzioni già delegate. I riferimenti browser indipendenti erano verifiche pertinenti, ma mancava una separazione
esplicita delle responsabilità e una regola contro le ripetizioni della ricerca e dei test.
Una ricerca ricorsiva non limitata ha attraversato il workspace ed è stata interrotta.
L'automazione ogni 20 minuti ha inoltre generato risvegli durante una quota già esaurita.
Non è verificabile da questi log quanti crediti Codex abbia consumato ciascuna di tali attività.

La responsabilità del coordinamento e della frequenza dei controlli è di Codex: delegare
l'implementazione non produce risparmio se il coordinatore continua a fare lo stesso lavoro.

## Procedura approvata

Le regole operative vincolanti sono in [AGENTS.md](../AGENTS.md): un delegato, soglia iniziale
25 tool call, nessun rilancio automatico oltre budget, niente ricerca duplicata, letture
mirate, evidenze dei test riutilizzate, niente polling continuo e stop al limite di quota.
La soglia riguarda le chiamate agli strumenti, non i turni o i token e non è un limite CLI
verificato. Non usare un tetto USD destinato alle API come promessa di controllo dei crediti.

Per il primo incarico dopo una futura ripresa:

1. Mantenere i modelli invariati e fissare un solo risultato, pochi file e le verifiche minime
   necessarie. Indicare la soglia nel prompt prima di avviare Claude.
2. Registrare contesto iniziale, modello, chiamate tool univoche, turni, token nuovi,
   creazione/lettura cache, output e verifiche; conservare i log fuori dal contesto del modello.
3. Codex revisiona diff ed evidenze; ripete verifiche solo per un motivo esplicito.
4. Confrontare con incarichi simili per complessità e copertura. Quote account eventualmente
   rilevate prima/dopo sono soltanto indicative se nel frattempo c'è altra attività.
5. Modificare soglia o strategia dopo il confronto, senza promettere anticipatamente il 50%.

Non sono stati modificati codice, dipendenze, API, flag, build o politiche di release.
Non servono patch di sorgenti Servo, sincronizzazioni di roves-action/Packmaster o nuovi test
per questo aggiornamento delle istruzioni e del handoff.

## Prima ripresa con soglia — incarico 0274

L'utente ha chiesto di riprendere. Claude ha usato lo stesso modello (`claude-opus-5-5`) e
si è fermato a 25 tool_use univoci; 26 turni. Contesto iniziale 46.186 token. Record finale:
36 input token, 96.697 token cache creati, 1.399.981 cache letti, 34.467 output (18.420 thinking).
Costo indicativo riportato 1,7430562 USD, non crediti dell'abbonamento. Fonte unica:
`target/claude-0274-owned-resolve.log`. Codex ha atteso il processo tramite un'orchestrazione
strumentale, senza nuovi turni del modello ogni minuto o analisi concorrenti della soluzione.

L'incarico non è completo: test isolati 67 normal/66 JIT-less passano, ma mancano replay
completo, integrazione/verifica frontend e chiusura della documentazione. Un errore iniziale
della fixture (callee letto da vp[1] anziché vp[0]) ha richiesto una correzione prima dei test
finali. Il WIP è salvato separatamente e i sorgenti pubblicabili sono ripristinati. Nessun
commit/push; nessun rilancio oltre soglia. Questo verifica il rispetto del limite di processo,
non un risparmio percentuale a parità di lavoro completato: il compito è differente e incompleto.
Non aumentare automaticamente il budget; la decisione successiva spetta all'utente.

## Estensione autorizzata — completamento 0274

L'utente ha autorizzato esplicitamente una sola estensione dello stesso incarico 0274: stesso
modello (`claude-opus-5-5`), 25 tool_use univoci aggiuntivi, massimo cumulativo 50; il budget
non è stato azzerato. L'estensione ha usato 21 chiamate (cumulativo 46/50), inclusa una
chiamata fallita per un errore di quoting della shell, e ha completato l'incarico in locale:
verifica API frontend senza correzioni, regressione browser con marker esatti, test del runner
in WSL, replay da pristine e documentazione. Test roves-js riusati tramite confronto della copia
isolata, non rieseguiti. Token, turni e costo di questa estensione vanno aggiunti da Codex dal
record finale; nessun dato stimato qui. Nessun commit/push, nessun nuovo incarico, nessun polling.

### Esito verificato dell'estensione 0274

Conteggio dal log finale `target/claude-0274-completion.log`: 22 tool_use univoci (non 21 come
nel riepilogo del delegato), 23 turni; contesto iniziale 46.019 token. Token: 48 input nuovi,
91.386 cache creati, 1.839.535 cache letti, 28.331 output (12.154 thinking); costo indicativo
1,665807 USD. Totale incarico con la prima parte: 47 chiamate su 50 autorizzate, 49 turni,
188.083 token cache creati, 3.239.516 cache letti, 62.798 output e 3,4088632 USD indicativi.
Non sono crediti d'abbonamento e non dimostrano un risparmio percentuale su lavori equivalenti.

La chiusura riusa i test runtime 67/66 perché le copie coincidono; nuovi contratti runner 9/9.
Codex ha revisionato il diff e confrontato i sei file del replay e la copia runtime, senza
rieseguire i test già dimostrati. Il replay di 57 patch e il dry-run/apply 0274 coincidono;
frontend e browser reale restano responsabilità della CI. Il limite cumulativo è rispettato.
L'utente ha autorizzato il ciclo con nuovi incarichi circoscritti; questo non autorizza
rilanci illimitati di un incarico incompleto o tentativi dopo un limite di quota.

### Incarico 0275: registro debole dei moduli sorgente

21 tool_use univoci, 22 turni; modello invariato claude-opus-5-5. Contesto iniziale 46.535 token;
44 input nuovi, 130.679 cache creati, 2.262.903 cache letti, 56.063 output (31.020 thinking).
Costo indicativo 2,6194486 USD. Fonte: record finale target/claude-0275-weak-source-registry.log.
Il compito è completo localmente entro 25 chiamate: 71 test normali/70 JIT-less passati,
negativi mirati e replay di 48 patch precedenti più 0275. Codex ha confrontato i quattro file
ricostruiti e la copia runtime con i sorgenti, senza ripetere i test. JSON e realm restano forti:
non è un risultato sulla memoria delle pagine. Nessuna percentuale di risparmio attribuita.


### 0276: incomplete permission-blocked assignment (2026-10-22)
Claude Opus 5.5, 17 unique tool calls, 18 turns; no production source edits or tests.
The noninteractive acceptEdits session denied shell write commands (including git apply
--check); six denied attempts are recorded in the final result. Stop rather than repeatedly
trying equivalent commands or restarting to reset the budget. Scratch draft remains in
target/task-0276; clarify worker permissions and remaining/extended budget before resuming.
Usage: input34, cache creation83914, cache read1124421, output36756 (thinking16925).
Indicative provider list-price USD1.6314522; this is not account credit consumption or a
comparison with equivalent Codex work. Full log: target/claude-0276-weak-defined-functions.log.


### Authorized autonomous continuation (2026-10-22)
The user approved one Codex-authorized justified extension per assignment: 25 initial
unique tool calls, maximum50 cumulative across resumed sessions. Stop at50 or actual
subscription quota; do not reset counters, switch models or retry equivalent denied actions.
Diagnose worker permissions before resuming; never bypass automatic review rejections.
Task0276 resumes from17 calls with at most33 additional calls, including required checks.
Codex directs/reviews and Claude implements. CI gates, online heavy builds, no duplicate
checks and no periodic automation remain in force. Historical stops above remain evidence.


### 0276 completion with explicitly allowed session tools
Resumed Opus5.5 with explicit allowedTools (not bypassPermissions):18 unique tool calls,
35 cumulative including17 previous calls; zero permission denials. Input38, cache creation
76720, cache read1411455, output17619 (thinking5611). Normal72/JITless71 pass, negative
strong-root regression fails as required; replay51 prior patches plus0276 matches sources.
Codex reviewed the source diff and independently compared six production/audit/replay files;
no duplicate tests. Log target/claude-0276-resume.log. JIT-info lookup now enters V8;
performance impact unmeasured, realms/JSON/Servo maps still retain objects.
