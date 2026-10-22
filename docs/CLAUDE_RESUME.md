# Ripresa autonoma per Claude — 22 ottobre 2026

Questo file consente di riprendere senza la chat Codex. Leggere AGENTS.md, questo file e
solo l'intestazione corrente di docs/V8_MIGRATION.md; consultare lo storico solo per problemi
concreti. L'utente ha chiesto che Claude possa continuare direttamente domani, perché la quota
settimanale Codex è quasi esaurita. Non è necessario attendere Codex per riprendere lo sviluppo
su richiesta dell'utente. Evitare doppia implementazione o altri agenti paralleli.

## Stato affidabile

- Ultimo push prima di questo handoff:47a5cd351e1, wiki c9bcc84, con0275–0278.
- Commit completi aggiunti:45d88a513e7 (0279),196ee7a5faa (0280); wiki9fbd176.
- 0279: registro JSON debole prima del link e dopo gli steps; radici temporanee durante il
  link, private cppgc sul record. JSON linkati mai valutati restano forti. Normal82/JITless81.
- 0280: prototipo immutabile ClassBox passa da Global a TracedReference; normal84/JITless83.
  Revisioni, negativi e replay completati; nessuna misura RAM/CPU o raccolta pagina dimostrata.
- 0281 è INCOMPLETA, NON verificata e NON applicata ai sorgenti. Il WIP è versionato in
  docs/handoff/0281-incomplete-realm-registry.patch, fuori da patches/ applicate dalla CI.
- La quota sessione Claude si è esaurita; reset indicato dalla CLI00:10 Europe/Rome.
  Nessun tentativo automatico o cambio modello è stato effettuato.

## CI prima di questo push

Su47a5cd351e1: V839822504067 verde dopo un retry del solo bootstrapJITless (apt30min),
Android39822504026 verde, iOS39822504229 verde. Generale39822504002:12/13verdi,
Steam119978506378 scaduto dopo90min durante mach bootstrap/dipendenze, prima della
compilazione. Build e packaging, inclusoDEB che falliva prima di0278, sono verdi.
Il checkpoint generale NON è pienamente verificato. Il push di salvataggio è richiesto
esplicitamente dall'utente; verificare le nuove run del commit reale, non riusare gli esiti
di47a5 per dichiarare validati0279/0280. Non creare release o bump consumer per questo WIP.

## Come riprendere0281

1. Controllare HEAD, git status, submodule wiki e CI reali. Dare priorità a eventuali errori
   reali delle nuove run; leggere soltanto i log pertinenti. Niente polling continuo.
2. Leggere docs/handoff/0281-realm-design.md e0281-implementation-instructions.txt.
   Sono uno snapshot del design precedente0280: il prerequisito immutable prototype è GIA'
   implementato e non va rifatto. Usare i sorgenti correnti come autorità sui nomi.
3. Esaminare la patch WIP, verificare git apply --check e riapplicarla solo se coerente;
   non considerarla completa. Nove file runtime erano stati modificati, senza completare
   suite di test, patch finale o replay. Non iniziare da una copia target testata precedente.
4. Completare realm cppgc: Context TracedReference, holder nel context, registro debole,
   radici realm corrente e precedenti annidati, RealmSwitch RAII nei callback, iterazione
   con radici temporanee, principals rilasciati esattamente una volta a GC o shutdown.
   Preservare il realm iniziale forte per gli intrinseci catturati. Nessuna nuova libreria:
   rusty_v8/cppgc esistenti coprono il meccanismo.
5. Verificare pubblicazione/rooting durante allocazioni, context→holder→RealmData→context,
   assenza di dangling Realm pointer, ordine finalizer/Runtime, principals e reentrancy.
   Testare oggetto/funzione che trattiene realm, current+saved nested GC, callback CSP/jobs,
   wrapper Window, iterazione e negativi sicuri senza letture di puntatori liberati.
6. Test normali e JITless nel workspace isolato target/v8-incumbent-audit; ricrearlo se manca,
   seguendo manifest e sorgenti roves-js/roves-v8/derive. CARGO_TARGET_DIR assoluto
   C:/Users/simon/3D Objects/roves/target/roves-js. Copia testata identica alla produzione.
   Build completa browser solo in CI. Non duplicare test già verificati su sorgenti identici.
7. Produrre patch0281 reale solo quando completa, replay pristinev0.5.0+patch precedenti,
   aggiornare CUSTOMIZATIONS, README, wiki e handoff; revisionare e committare/pubblicare
   secondo AGENTS e istruzioni correnti dell'utente. Nessun tetto chiamate o contabilità
   consumi richiesto; fermarsi soltanto a quota effettiva, stop o blocco non risolvibile.

## Resto del lavoro (non dichiarare completata la migrazione)

- Cache Function0273: identico corpo può riusare origine del primo chiamante. WIP locale
  target/0273-incomplete-native-traps.patch NON applicare alla cieca; diagnosi nel piano V8.
- WindowProxy navigazione/identità, incumbent e lifecycle worker/termination/shutdown.
- Servo mantiene ancora global e mappe moduli: realm raccoglibile nel backend non prova
  raccolta pagina dopo navigazione. JSON linkato mai valutato resta trattenuto.
- WPT selezionati e validazione browser normal/JITless, scene Pixi/Three, performance e
  release/pin soltanto quando appropriati. Niente promesse di risparmio RAM non misurato.

Il materiale in docs/handoff è archivio WIP/design, NON una patch CI né sorgente pronto.
