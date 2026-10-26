# 0365 — primo gate API object/interceptor verificato

Nessuna modifica di produzione. Il diff riproducibile in `0365-api-reflector-proof.patch`
aggiunge un modulo temporaneo api_reflector_proof ai test. Applicarlo solo a una copia
isolata, eseguire il filtro api_reflector_proof e poi ripristinare la copia. tests.rs nel
runtime pubblicabile è stato ripristinato a HEAD: niente helper prototipo inutilizzato.

## Risultati eseguiti

V8/rusty_v8 152 Windows:2testnormali e2JITless verdi, logtarget/task-0365/normal-review.log
 e jitless.log. Le firme indexed getter/query/enumerator compilano; nessun nuovo pacchetto.
- Object normale e APIobject senza interceptor: Reflect.preventExtensions true, Object
  restituisce l'oggetto, isExtensible false.
- APIobject named-only e named+indexed: Reflect false, Object TypeError, extensible true.
  V8 fornisce già il rifiuto senza un nostro hook. Dopo il tentativo le proprietà funzionano.
- Estensioni scope con gli oggetti reali: baremethod this identifica document/form/element,
  sourceText uguale a quello wrapped senza scope, strict/sloppy e durata dopo GC verificati.
- Identità roves-js JSObject avanti/indietro verificata per APIobject generico senza ClassBox.

Riferimento Chrome155 Windows visibile/about:blank: Document permette preventExtensions,
form/select rifiutano. Il caso named-only con questi interceptor differisce quindi da Chrome
Document; non dichiarare quel dettaglio conforme o risolto. La compatibilità con il trap
attuale di Servo va valutata nel gate reale, senza decisioni su degradazioni ipotetiche.

## Correzione della fixture durante la revisione

La prima prova riservava1internalfield senza inizializzare il ClassBox, poi convertiva
l'oggetto attraverso from_v8: il campoundefined era interpretato come puntatore e causava
abort. La fixture generica corretta usa0campi interni. ClassBox/proxytraps reali restano
esplicitamente FUORI da questo gate: niente modifica al bridge per mascherare un input
nativo non valido e niente affermazione di prova dei reflector Servo effettivi.

## Gate successivo

Usare `docs/handoff/0364-real-dom-api-reflector-design.md` per una prova con gli hook DOM
veri: ClassBox inizializzato prima dell'esposizione, tracing/private/reservedslots e finalize,
get/query/set/delete/descriptor/definer/enumerator named+indexed, expando/readonly/chiavi,
receiver e scope. Riusare gli hook esistenti come unica implementazione delle proprietà.
WindowProxy e oggetti con prototipo dinamico o call/construct restano fuori. Nessun cambio
codegen prima che il gate differenziale Proxy/APIobject reale dimostri i contratti necessari.
