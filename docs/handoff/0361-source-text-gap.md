# 0361 — lacuna sourceText e punto di ripresa

## Rinviato al TODO per scelta utente29ottobre2026

Fuori dai criteri della migrazione V8; non e un blocco alla sua conclusione.
Conservare le prove sotto per un eventuale futuro intervento di compatibilita.

## Riesame storico 29 ottobre 2026 (analisi0422)

Il punto autorevole Git/CI/quota e docs/V8_MIGRATION.md. Blocchi disco/CI/quote sotto
sono storico del26ottobre: non applicarli al checkout attuale. Al riesame sono disponibili
circa8.95GiB suC:, non81MiB. Non ripetere cleanup o ricostruzioni sulla base di quelle note.

Deno possiede il grafo moduli, ma gli handler HTML chiamano ancora script_impl::CompileFunction
(components/roves-js/src/script_impl.rs): compile_function senza extension per validare il
corpo; catena non vuota -> outer with + inner function anonima; set_name finale. Il port
Deno e le mappe inline0419 NON sostituiscono questo percorso e non chiudono0361.
La prova facade0363 e gia eseguita: normal-review.log registra testverde che dimostra il
receiver errato della chiamata nuda, NON una soluzione pronta. Non ripeterla come candidato
nuovo e non pubblicare il test sperimentale come prova di conformita risolta.

Resta necessario uno studio circoscritto delle API pubbliche V8 e dei reflector reali,
con identita del receiver e tutti i contratti preservati prima di modificare produzione.
Nessuna nuova via dimostrata da questo riesame; nessun workaround toString/nome sintattico,
fork o duplicazione Deno introdotto. Nessuna implementazione parziale lasciata nei sorgenti.


Nessuna modifica di produzione. I test sperimentali sono fuori dai sorgenti pubblicabili:
`target/task-0361/incomplete-source-text-tests.patch`,4041byte,
SHA256167cf52d7c3a555c6a1f10b5bb7d143e0d33c1b58ffd027dbbae149f67e4ff86.
Proposta di uguaglianza separata in `target/task-0361/proposed-source-text.diff`.

## Evidenza

Chrome155 Windows visibile/about:blank: un attributo onclick col corpo `this.rovesHits += 1;`
ha nome onclick, length1 e sourceText `function onclick(event) {\nthis.rovesHits += 1;\n}`.
V8 con CompileFunction senza scope restituisce lo stesso testo. Con scopeProxy:
`function (event) {this.rovesHits += 1;\n}`. Nome/lunghezza/body/this/scope sono corretti,
nessun prefisso with esterno esposto. Test sperimentale normale iniziale e JITless passano;
JITless finale dopo rimozione della proposta ignorata passa. Relink normale finale bloccato
LNK1201/PDB dopo disco pieno, non interpretare come risultato del test. Sorgente tests.rs
ripristinato esattamente a HEAD dopo salvataggio del diff: nessun WIP da pubblicare.

HTML definisce il sourceText esatto, incluse le LF, nel passo3.9 di:
https://html.spec.whatwg.org/multipage/webappapis.html#getting-the-current-value-of-the-event-handler
Quindi la differenza è una lacuna di conformità APERTA, non un formato accettato.
Non inserire semplicemente il nome nella function expression: creerebbe un binding locale
che può nascondere proprietà dell'elemento/documento. Vietati override globale di toString,
layout privato V8, parser/formatter JS ad hoc. Prossimo passo circoscritto: cercare una via
nell'API pubblica V8 per funzioni wrapped con lookup attraverso proxy; eventuale facade
con interceptor va provata prima per lookup, scritture, receiver, unscopables e durata GC.
Solo con prova mantenuta e regressioni0356/0357 intatte aprire un fix di produzione.

## Esito0363 — facade con interceptor

La via facade è stata scritta come prova nativa (vedi la sezione Claude0363 in
`docs/V8_MIGRATION.md`; esecuzione Codex pendente). Lookup, scritture, delete, receiver degli
accessor, unscopables e sourceText sono attesi corretti; la chiamata nuda di un metodo dello
scope riceve però la facade come `this`, quindi la facade NON è una correzione di produzione.
Resta solo la via dei reflector come oggetti API V8 con interceptor al posto dei Proxy JS.

## Blocco risorse

C: ha raggiunto0byte liberi. Sono stati rimossi solo9 artefatti exe/pdb generati in questa
sessione (circa500MiB), più il PDB poi corrotto dal fallimento; nessun sorgente/log/dipendenza.
Spazio rimasto circa81MiB: insufficiente per nuovi build, anche leggeri. Il tentativo Cargo
con --manifest-path dalla root leggeva .cargo/config.toml (lld) e tentava un'altra cacheV8:
NON ripeterlo. Eseguire dall'isolated workspace stesso con configurazione coerente; servono
prima spazio adeguato e recupero del PDB normale. Non pulire cache generiche alla cieca.

## CI e pubblicazione

Pubblicato37815319987/wiki1fa4cad. Locali0357–0360 completi e revisionati, non pushare.
V8retry40366152633:9successi, browserMac121735908007 cancellato ancora senza runner;
iOSretry40366152391:2successi, ios121736581176 cancellato ancora senza runner. Annotazioni
confermano capacità hostedmacOSARM64, nessun runner acquisito. NON concatenare altro retry
identico. Generale40366152303 all'ultimo checkpoint11successi2attivi; Androidverde.
Linuxnormal/JITless0356 verificano GCinline/additivo; normalframe rilasciaelemento/doc/probe.
Completare verifiche Mac/iOS mancanti quando la capacità cambia; nessun nuovo push ora.
Nessuna automazione periodica o polling. Nessun Claude attivo.
