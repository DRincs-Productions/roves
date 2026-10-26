# 0363 — prova V8 della facade per scope (candidato respinto)

Nessuna modifica di produzione e nessun helper sperimentale nel runtime pubblicabile.
La prova riproducibile è nel diff `0363-scope-facade-proof.patch` di questa cartella:
applicarlo solo in una copia isolata, eseguire il test
`scope_facades_match_source_text_and_lookups_but_rebind_bare_call_receivers`, poi ripristinare
la copia. Non è una patch di produzione da applicare alla serie Servo.

## Esiti reali

V8/rusty_v8 152, Windows: test mirato normale e JITless verdi; log in target/task-0363/.
Il primo compile ha individuato la firma setter ReturnValue<Boolean>, corretta e ritestata.
ObjectTemplate con named getter/query/setter/deleter è accettato come context extension.
V8 traccia il Proxy nei dati degli interceptor. Test: sourceText uguale al compilatore V8
senza scope e a Chrome155, lookup/scritture/delete/accessor ereditati, ordine scope,
unscopables, typeof mancante, corpo strict/sloppy, sequenza trap equivalente e vita dopo GC.

Controprova determinante: una chiamata `who()` trovata nello scope riceve come this la facade,
non il Proxy reale. Nel compilatore attuale `who() === rovesDocument` è vero, nella facade
falso. Quindi i metodi DOM non qualificati potrebbero ricevere il receiver sbagliato.
Il test asserisce questo risultato per dimostrare il rifiuto del candidato: non promuoverlo
come regressione di produzione che fissi per sempre un comportamento errato.

## Conseguenza e prossimo passo

Facade NON adottata. Nessun binding dei metodi automatico: cambierebbe identità delle
funzioni, chiamate con call/apply e assegnazioni. Restano esclusi override globale di
toString, layout privati V8 e nomi interpolati nella sintassi senza semantica corretta.
Lacuna sourceText0361 ancora aperta.

Studiare un perimetro minimo di reflector DOM reali (Document/form) come API object V8
con interceptor, invece di una facade diversa dall'oggetto visibile a JS. Riusare gli hook
ProxyHandler esistenti senza duplicare un motore di proprietà; WindowProxy e proxy con
getPrototypeOf dinamico restano fuori. Prima un design preciso e prove di receiver,
identità, descrittori, GC e conformità, senza modifiche ampie non dimostrate.

Le verifiche usano i workspace isolati esistenti con cwd corretto; circa0.86GiB liberi
su C dopo la prova. Niente build pesanti locali o nuove copie di cache.
