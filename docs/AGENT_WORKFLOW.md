# Consumi e coordinamento degli agenti

## Stato e obiettivo

Il 22 ottobre 2026 l'utente ha fermato la migrazione e approvato la priorità **minimo consumo
complessivo**. Nessun rapporto rigido 50/50: token e quote dei due abbonamenti non sono
unità direttamente confrontabili. La migrazione riprenderà soltanto su nuova richiesta.
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
