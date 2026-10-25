# Prossimo incarico0351 — ownership dei listener DOM

Questo è un piano da verificare, non un’implementazione. Prima leggere il solo stato corrente
in docs/V8_MIGRATION.md e Git/CI reali.0349 corregge il crash origin-check;0350 è una
regressione aggiuntiva senza modifica produzione. Non sovrapporre incarichi attivi.

## Problema dimostrato e riuso

Lo snapshot0345 raggiunge il frame dal RootSet tramite CallbackObject::root del listenerclick
iniettato. CallbackObject e Promise erano già autoradicati in Servo0.5 pristino. Non togliere
le root globalmente, non svuotare listener solo per far sparire il frame: gli oggetti possono
essere ancora raggiungibili e i comportamenti di dispatch/realm vanno preservati.

Riusare Heap/JSTraceable, CallbackContainer e RootedTraceableBox esistenti, sul collector V8
attuale. Non introdurre un collettore Rust, contatori manuali di ownershipRc o nuove dipendenze
senza uso concreto. Il glue da studiare è lo storage DOM distinto dai wrapper nativi radicati,
come0348 per FontFaceSet, ma con il contesto callback originale conservato.

## File e vincoli concreti

- components/script_bindings/callback.rs: CallbackObject conserva callbackHeap, incumbent e
  permanent_js_root; CallbackContainer::new cattura implicitamente il contesto corrente.
- components/script_bindings/codegen/codegen.py: CGCallback.getConstructors circa8539 e
  CGCallbackFunctionImpl circa8634 generano le factory. Un wrapper ricostruito NON deve
  ricatturare l’incumbent del momento del dispatch al posto di quello della registrazione.
- components/script/dom/event/eventtarget.rs: EventListenerType::Additive e CompiledEventListener;
  add/remove_event_listener circa877/935; duplicati confrontano identitàcallback e capture.
- Il ramo signal di add_event_listener clona laRc in RemovableDomEventListener/AbortAlgorithm:
  cambiare solo EventTarget lascerebbe una seconda root permanente. Includere questo owner
  nel perimetro o documentare/provare perché un caso distinto resta deliberatamente fuori.

Candidato: record DOM tracciato con reflector callback e incumbent catturato; factory native
che ricrea unaRc radicata usando quei metadata. Valutare l’API minima per la factory nel
codegen, senza downcast/layoutRc non mantenuti. Proteggere l’intero record anche durante
ricostruzione e dispatch (RootedTraceableBox se necessario), inclusi incumbent e callback,
non solo il JSfunction object. LeRc native esterne continuano a usare il comportamento
radicato attuale. Non generalizzare a tutti gli handler inline senza un incarico separato.

## Prove richieste prima di pubblicare

1. Listener di un nodo detached che cattura il nodo: ciclo raccoglibile quando irraggiungibile;
   controprova con riferimento esterno vivo. Native wrapper esterno non deve diventare dangling.
2. Add/remove, duplicati/capture/once, handleEvent object e modifiche della lista durante dispatch.
3. AbortSignal: cancellazione e rilascio delle capture, nessun ramo extra che conserva laRc.
4. Callback creato in un realm e registrato da un altro: preservare l’incumbent originario anche
   se il dispatch parte più tardi da un terzo contesto. Preparare un riferimento Chrome mirato
   per un comportamento osservabile (es. source di postMessage) prima di fissare le attese.
5. Verifica del codice generato deterministica (PYTHONHASHSEED=0), native normal/JITless per
   modifiche runtime effettive; buildServo e snapshot frame-retention online. Usare strumenti
   mantenuti per il grafo, non parser/BFS nostri. Non promettere retention risolta senza prova.

Se non c’è una soluzione circoscritta sicura, consegnare diagnosi verificabile e un solo passo
successivo, senza pubblicare mezze implementazioni. Aggiornare CUSTOMIZATIONS/patch applicabile,
README/wiki/handoff con esiti e limiti; non ricreare CLAUDE_RESUME.md, niente buildpesanti locali.
