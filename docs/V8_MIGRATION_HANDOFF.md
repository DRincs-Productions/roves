# V8 migration - handoff (2026-10-04, fine giornata)

Punto di ripresa operativo sintetico. L'utente autorizza a proseguire autonomamente: implementare,
testare localmente, avviare e attendere CI, correggere e continuare senza fermarsi ai checkpoint.
Aggiornare questo file a ogni checkpoint e condensare le note chiuse; il piano dettagliato e i
criteri finali sono in [`V8_MIGRATION.md`](./V8_MIGRATION.md).

## Stato corrente

**CP37 completo**, commit sorgente `2e053f7c216` e gitlink wiki `81642b8bb10`. `DomObject::native_object_id()` espone l'identità stabile engine-neutral; `dom_struct` genera `PartialEq` tramite tale API. Test reflector 2/2, macro `servo-dom-struct` 3/3, rustfmt e patch 0101 reverse-check superati. CI verde: V8 37226916624 (6/6), Servo 37226916650 (13/13), Android 37226916659 e iOS 37226916639.

**CP38 implementato e pushato**, commit `b8f0acd3d3d`; wiki `roves-wiki` aggiornata e pushata al commit `ca2ec3b`. `components/script_bindings/root.rs` usa `NativeObjectId` per equality/hash di `Dom<T>` e `DomRoot<T>`. Test locali: test nuovo root identity/hash, reflector 2/2, `servo-dom-struct` 3/3, compile-only `servo-script-bindings`, rustfmt e `git diff --check` verdi. Patch 0102 reverse-checka e il validator CI ha applicato la serie pristine con successo. Build wiki Next.js: 82 pagine generate.

**CI CP38:** V8 `37231672237` 6/6, Android `37231672206`, iOS `37231672276` verdi. Servo
`37231672272` 12/13: l'unico fallimento (Windows portable, exit 35) era un flake TLS del runner
(`CRYPT_E_REVOCATION_OFFLINE`, letto dal log con il token read-only fornito il 2026-10-05). Fix:
tutti i `curl` di `test.yml` ora usano `--retry-all-errors` (vedi `CUSTOMIZATIONS.md` del
2026-10-05); il push di quel fix riverifica l'intera matrice Servo.

**CP39 (2026-10-05):** prima migrazione dall'audit dei chiamanti di `Reflector::get_jsobject`
(categorie in `V8_MIGRATION.md`). `OpaqueNode`/`UntrustedNodeAddress` ora usano l'indirizzo del
`Node` nativo invece del `JSObject`; `from_untrusted_node_address` non usa più
`private_from_object`. Patch 0103, `cargo check -p servo-script` locale pulito. CI in verifica.

**CP40 (2026-10-05):** gli 8 ingressi manuali nel realm di un oggetto DOM passano ora tutti per
`crate::realms::enter_auto_realm` (patch 0104); `get_jsobject()` scende da 56 a 45 chiamanti.
`cargo check -p servo-script` locale pulito.

**CP41 (2026-10-05):** gli 8 `ObjectValue(x.reflector().get_jsobject().get())` passano per il
nuovo `script_bindings::reflector::dom_object_value` (patch 0105); 37 chiamanti aperti di
`get_jsobject()` restano. Il helper non fa wrapping cross-compartment (come il codice sostituito):
questione aperta annotata in `V8_MIGRATION.md`. `cargo check -p servo-script` locale pulito.

**CP42 (2026-10-05):** gating WebGL2 dei canvas via `is_webgl2_enabled_for_global(&GlobalScope)`
e `Window.event` via `ToJSValConvertible for Reflector` (patch 0106); 34 chiamanti aperti.

**CP43 (2026-10-05):** il tracking delle rejection in `script_runtime.rs` usa `Promise::result`
e `promise_obj` (patch 0107): l'API Promise di SpiderMonkey resta solo in `promise.rs`; 31
chiamanti aperti.

**CP44 (2026-10-05):** gli ultimi due `ObjectValue(..get_jsobject()..)` su più righe (cursor
IndexedDB, rendering context del paint worklet) usano `dom_object_value` (patch 0108); 29
chiamanti aperti, quasi tutti API engine-specifiche (windowproxy, debugger, structured clone,
definizione interfacce, `JS_DefineProperty`, compile).

**CI CP39–CP44 verde** al commit `540320d7df2`: Servo `37349391499` (serie patch fino a 0108,
build e bundle), Android `37349391487`, iOS `37349391463`; matrice V8 `37348750612` verde al
commit CP41 `e067fc2800f` (i commit successivi non toccano i path del workflow V8).

**CP45 (2026-10-06):** `codegen/v8_coverage.py` misura il backend V8 su tutto il WebIDL di Servo:
**13/486** definizioni generabili. Ha fatto emergere e correggere un crash su
`[Exposed=(Window,Worker)]`. Blocchi principali: ereditarietà 279, costruttori 55,
`LegacyNoInterfaceObject` 32, `Pref` 25+. Test generatore 29/29 (patch 0109); la CI V8 stampa il
report. Prossimo obiettivo Phase 4: supportare l'ereditarietà delle interfacce nel generatore e nel
runtime `roves-v8`. Prima verificare cosa offre già il runtime: i flag di materializzazione
condivisi indicano un supporto parziale alle catene di prototipi.

**CP46 (2026-10-06):** il generatore V8 supporta l'ereditarietà delle interfacce (patch 0110):
un unico tipo nativo `T` per albero, `XNative: ParentNative`, `XBinding::install(runtime,
&parent_binding)`, moduli fratelli via `v8_module_name`, `run_v8.py` con file WebIDL antenati come
contesto. Il test runtime ha trovato e corretto una non conformità: mancava
`Object.getPrototypeOf(Child) === Parent`, ora impostato da `expose_interface`. Test: generatore
32/32; `roves-v8` pilot e JIT-less 67+5+2 ciascuno, default 54+2; check `servo-script` col pilot
pulito. La copertura resta 13/486 perché quasi tutte le gerarchie partono da `EventTarget`, che
ha un costruttore. **Prossimo: costruttori WebIDL** (208 definizioni bloccate) nel runtime
(`define_interface` oggi lancia "Illegal constructor") e nel generatore.

**CP47 (2026-10-06):** costruttori WebIDL (patch 0111). Runtime:
`define_constructible_interface` (argomenti convertiti come per le operazioni, finalizzazione
esattamente una volta, TypeError senza `new`, `length`, sottoclassi JS); refactor
`convert_webidl_arguments` e `arm_native_finalizer`. Attenzione: in `rusty_v8` solo il `Weak`
originale possiede il finalizer, i clone sono semplici osservatori. Generatore: `XNative::Constructor`
chiamato in forma qualificata; `[NewObject]` accettato; `[Throws]` e overload rifiutati. Test:
generatore 34/34, `roves-v8` 69+5+2 (pilot e JIT-less), default 55+2, check `servo-script` pulito.
**Prossimo:** `[Throws]` uniforme per costruttori, metodi e attributi (serve un tipo errore
engine-neutral nel runtime, che lanci TypeError/RangeError; DOMException più avanti). Poi i membri
`[LegacyUnforgeable]` copiati nei discendenti (`Event.isTrusted`): sono la stessa definizione,
non un vero shadowing, e oggi bloccano 40 interfacce. Poi `[Pref]`.

**CP48 (2026-10-06):** `[Throws]` su costruttori e operazioni (patch 0112). `roves_v8::WebIdlError`
(TypeError, RangeError, DomException) è engine-neutral; `define_fallible_webidl_method`;
`NativeConstructor` restituisce `Result`. DOMException: `new DOMException(message, name)` se il
realm lo definisce, altrimenti un `Error` con `name` impostato (lacuna documentata finché il
runtime non installa l'interfaccia). Test generatore 35/35, `roves-v8` 70+5+2 (pilot e
JIT-less), default 55+2, check `servo-script` pulito. Copertura **16/486**. **Prossimo:** i
membri `[LegacyUnforgeable]` copiati dal parser nei discendenti (`Event.isTrusted`, 40
interfacce), `[GetterThrows]`/`[SetterThrows]` sugli attributi, `[Pref]`.

**CP49 (2026-10-06):** `[LegacyUnforgeable]` readonly (patch 0113): proprietà proprie non
configurabili dell'istanza. Il runtime le reinstalla nei discendenti perché
`FunctionTemplate::inherit` **non** propaga gli accessor `set_accessor_property` dell'instance
template; il generatore salta le copie del parser. `[Pure]`/`[Constant]` ignorati. Test
generatore 38/38, `roves-v8` 71+5+2 (pilot e JIT-less), default 55+2, check pulito. Copertura
16/486. **Prossimo, il blocco strutturale:** valori di tipo interfaccia (wrapper DOM come
attributi/argomenti/ritorni), da costruire sulla cache identità di CP33/CP35. Poi costanti,
dizionari, callback, sequence, union, `any`, `[Pref]`.

**CP50 (2026-10-06):** costanti WebIDL (`define_constant`, congelate su interface object e
prototype) e attributi readonly per tutti gli interi/float (patch 0114). Test generatore 39/39,
`roves-v8` 72+5+2, default 55+2, check pulito. Copertura **19/486**.

**CP51 (2026-10-06), Phase 3:** prototipo di ownership sull'heap unificato V8 (cppgc), patch 0115.
API engine-neutral: `Trace`/`Tracer` (come `JSTraceable`), `GcMember<T>` (come `Dom<T>`),
`GcRoot<T>` (come `DomRoot`), `JsRef` (come `Heap<JSVal>`); `allocate_traced`,
`create_traced_instance` (con `Object::wrap`), `traced_native`, `js_ref`, `eval_handle`. Un unico
tipo cppgc `GcBox` con `UnsafeCell<Box<dyn Any>>`, puntato dall'internal field 0, quindi i
callback esistenti funzionano invariati. I test dimostrano che i cicli nativo↔nativo e
nativo↔JS (listener che chiude sul proprio nodo) vengono raccolti, e restano vivi finché
raggiungibili. **Attenzione:** il GC di test a livello isolate scansiona lo stack in modo
conservativo; `force_full_gc_for_testing` ora aggiunge una raccolta precisa (`NoHeapPointers`).
Suite `roves-v8` 75+5+2 stabili (6 ripetizioni + release). **Prossimo:** binding generati e cache
di identità su istanze tracciate, poi la mappatura su `#[dom_struct]`/derive `JSTraceable` di
Servo.

**CP52 (2026-10-06):** un solo wrapper per nativo tracciato (patch 0116). `GcBox` traccia il
proprio wrapper (come `ScriptWrappable` di Blink): identità `===` ed expando preservati finché il
nativo vive, anche se JS lo abbandona; la coppia viene raccolta insieme. I binding generati
hanno `wrap_traced`. Test: generatore 39/39, `roves-v8` 77+5+2, default 59+2, check pulito.
**Prossimo:** mappare `#[dom_struct]`/`JSTraceable` di Servo su `Trace`, poi far passare un
tipo DOM reale (`Screen` o `ValidityState`) per i wrapper tracciati.

**CP53 (2026-10-06):** valori di tipo interfaccia (patch 0117). `Value::Native(NativeRef)`;
registro delle interfacce in uno slot dell'isolate. I ritorni riusano l'unico wrapper del nativo,
oppure lo creano con l'interfaccia concreta. Gli argomenti vengono verificati col tag cppgc e la
catena di ereditarietà del registro, non con il prototype chain falsificabile. **Bug trovato:**
`Object::unwrap` su un oggetto non API wrapper legge memoria arbitraria: ora c'è sempre prima
`is_api_wrapper()`. Test generatore 41/41, `roves-v8` 78+5+2, default 59+2, check pulito.
Copertura **33/486**. **Prossimo:** `[Pref]` (69+18), `[LegacyNoInterfaceObject]` (33),
overload (23), event handler, `any`, dizionari e sequence.

**CP54 (2026-10-06):** esposizione (patch 0118). Trait engine-neutral `roves_v8::Exposure`
(pref Servo, secure context) e `ExposeAll`. I binding hanno `install_with`. A livello di
interfaccia, `[Pref]`/`[SecureContext]`/`[LegacyNoInterfaceObject]` nascondono l'oggetto
interfaccia (`hide_interface_object`); a livello di membro saltano la registrazione. Test
generatore 42/42, `roves-v8` 79+5+2, default 59+2, check pulito. Copertura **66/486**.
**Prossimo:** overload (34), esposizione non-Window (30), attributi event handler, `any`,
dizionari, sequence, union.

**CP55 (2026-10-06):** overload distinguibili per numero di argomenti (patch 0119):
`define_overloaded_webidl_method`, nomi `X`/`X_` come in Servo. Gli overload che vanno
distinti per tipo (30) restano rifiutati. Test generatore 44/44, `roves-v8` 80+5+2, default 59+2,
check pulito. Copertura 67/486.

**CP56 (2026-10-06):** `[CEReactions]` (patch 0120). Trait engine-neutral
`roves_v8::CeReactions::with_ce_reactions`, implementato dal tipo nativo. Il generatore avvolge
le closure di getter, setter, operazioni e overload; la coda viene rilasciata anche se il membro
lancia. Test generatore 45/45, `roves-v8` 81+5+2, default 59+2, check pulito. Copertura 68/486;
i rifiuti "unsupported member" sono scesi da 71 a 35. **Prossimo:** sbloccare gli antenati
`Node`/`Element` (`[Abstract]`, attributi event handler, `any`, setter mutabili non supportati),
poi l'esposizione non-Window (30).

**CP57 (2026-10-06):** `[Abstract]` accettato (patch 0121); copertura **92/486**. Blocchi della
catena principale: dizionari (`EventInit`), sequence (`composedPath`), callback
(`addEventListener`), `[PutForwards]` (`classList`), overload per tipo. **Prossimo passo
architetturale:** un contesto di script engine-neutral (`ScriptContext`) passato ai nativi che
ne hanno bisogno, come il `cx` di Servo, per chiamare funzioni JS e creare oggetti/array.
Sblocca `any`, callback, event handler, dizionari e sequence.

**CP58 (2026-10-06):** `ScriptContext` (patch 0122). I nativi contestuali ricevono
`cx: &mut roves_v8::ScriptContext` (call, value, handle, js_ref); supportati argomenti
`any`/`object`/callback e ritorni `any`/`object`; le eccezioni dei callback vengono rilanciate
invariate (`WebIdlError::Js`). **Fix di soundness:** con la re-entrancy i setter non possono più
ricevere `&mut`: tutti i setter sono `&self` (interior mutability, come il DOM di Servo).
Test generatore 47/47, `roves-v8` 82+5+2, default 59+2, check pulito. Copertura 96/486.
**Prossimo:** attributi contestuali (event handler, attributi `any`), poi dizionari e sequence.

**CP59 (2026-10-06):** attributi contestuali (patch 0123): `define_contextual_attribute`,
conversione `LegacyCallback` per `[LegacyTreatNonObjectAsNull]` (event handler). I nativi
salvano i valori JS come `JsRef` tracciati. Test generatore 48/48, `roves-v8` 83+5+2, default
59+2, check pulito. Copertura 99/486. **Prossimo:** dizionari e sequence (costruttori di
`Event`/`EventTarget`, `composedPath`), poi callback interface (`EventListener`).

**CP60 (2026-10-06):** tipi WebIDL strutturati e sequence (patch 0124). `WebIdlType` ad albero e
`define_typed_webidl_method`; `convert_webidl_value` estratto (senza cambi di comportamento) e
`convert_typed_value` ricorsivo; le sequence usano il protocollo `@@iterator`. Test generatore
49/49, `roves-v8` 84+5+2 senza warning, default 59+2, check pulito. Copertura 100/486.
**Prossimo:** dizionari (`EventInit`, `GetRootNodeOptions`) sul percorso strutturato, poi
callback interface (`EventListener`) e union.

**CP61 (2026-10-06):** dizionari (patch 0125). `WebIdlType::Dictionary` con la conversione WebIDL
completa (ordine dei membri, default compreso `{}` annidato, membri required); gli struct Rust
sono generati per binding; c'è `define_typed_constructible_interface`. Test generatore 50/50,
`roves-v8` 85+5+2, default 59+2, check pulito. Copertura **143/486**. **Prossimo:** callback
interface (`EventListener`) e union, poi overload per tipo.

**CP62 (2026-10-06):** union, callback interface e costruttori tracciati (patch 0126). Algoritmo
union WebIDL, `call_user_object_operation`. Ora `new X()` crea platform object tracciati
(`TracedNative`, i binding costruibili richiedono `T: Trace`). **Ora generano `EventTarget`,
`Event`, `UIEvent`, `MouseEvent`, `KeyboardEvent`.** Test generatore 51/51, `roves-v8` 86+5+2,
default 59+2, check pulito. Copertura **157/486**. **Prossimo:** blocchi di `Node`/`Element`
(attributi `[SetterThrows]`/`[PutForwards]`, setter di stringhe nullable, variadici, overload
per tipo), poi l'esposizione non-Window.

**CP63 (2026-10-06):** attributi che lanciano, setter di tutti i tipi numerici e `[PutForwards]`
(patch 0127) sul percorso degli accessor contestuali; nuovo `ScriptContext::set_property`.
**Bug corretto:** `convert_webidl_integer` calcolava il modulo in f64, e ogni `long long` negativo
diventava 0; ora usa aritmetica `i128` esatta. Test generatore 52/52, `roves-v8` 88+5+2, default
60+2, check pulito. Copertura **170/486**; **`Node` ora genera**. **Prossimo:** variadici,
overload per tipo, `[HTMLConstructor]`, `[Unscopable]`, poi l'esposizione non-Window.

**CP64 (2026-10-06):** argomenti variadici (patch 0128): `WebIdlArgument::variadic`; gli argomenti
rimanenti, convertiti, finiscono in un'unica sequence. Test generatore 53/53, `roves-v8` 89+5+2,
default 60+2, check pulito. Copertura 171/486.

**CP65 (2026-10-06):** operazioni statiche, `[Unscopable]`, `toJSON` di default e `[NewObject]`
(patch 0129). Test generatore 54/54, `roves-v8` 90+5+2, default 60+2, check pulito. Copertura
**190/486**. **Prossimo:** esposizione non-Window (31: Worker e worklet), `Promise`, overload
per tipo, `[Serializable]`/`[Transferable]`.

**CP66 (2026-10-06):** `[Exposed]` per worker e worklet (patch 0130). `Exposure::global_name` ed
`exposed_in` (`*`, gruppo `Worker`); i binding nascondono oggetti interfaccia e membri sui global
non esposti. Test generatore 54/54, `roves-v8` 90+5+2, default 60+2, check pulito. Copertura
**208/486**. **Prossimo:** `Promise`, overload per tipo, iterable/maplike/setlike,
`[Serializable]`/`[Transferable]`, `[Global]` (`Window` e i global dei worker).

**CP67 (2026-10-06):** promise e Bindings.conf (patch 0131). Le operazioni che restituiscono
promise rifiutano invece di lanciare. Su `ScriptContext` ci sono `new_promise`, `resolve_promise`,
`reject_promise` e `rejected_promise`; per l'asincrono c'è `Runtime::settle_promise`. Quali
nativi ricevono `cx` lo decidono, come in Servo, le liste `'cx'`/`'realm'` di `Bindings.conf`
(`--bindings-conf` per le fixture). Test generatore 56/56, `roves-v8` 91+5+2, default 60+2, check
pulito. Copertura 220/486. CI verde su CP49–CP58 (`3e86880a3a4`).

**CP68 (2026-10-06):** i default (di argomenti e di membri di dizionario) sono convertiti a runtime
attraverso il tipo. Supportati anche `[LegacyWindowAlias]` (`add_interface_alias`), `[Func]`
(`Exposure::func_enabled`) e `[Serializable]`/`[Transferable]` (patch 0132). Test generatore 57/57,
`roves-v8` 92+5+2, default 60+2, check pulito. Copertura **242/486**. **Prossimo:** overload per
tipo, BufferSource (`ArrayBufferView`/typed array), iterable/maplike/setlike, `[Global]`, named
property (`LegacyUnenumerableNamedProperties`), `[HTMLConstructor]`/`[LegacyFactoryFunction]`.

**CP69 (2026-10-06):** buffer source (patch 0133). `WebIdlType::Buffer(BufferKind)`; accesso ai
byte in place con `ScriptContext::with_buffer_bytes`, creazione con `new_array_buffer`/
`new_typed_array`; supportati gli attributi union readonly. Test generatore 58/58, `roves-v8`
93+5+2, default 60+2, check pulito. Copertura **255/486**.

**CP70 (2026-10-06):** risoluzione completa degli overload (patch 0134): per conteggio, poi per
il tipo dell'argomento distintivo (`define_typed_overloaded_webidl_method`, `select_overload`,
selezione union riusata). Test generatore 58/58, `roves-v8` 94+5+2 senza warning, default 60+2,
check pulito. Copertura **259/486**.

**CP71 (2026-10-06):** attributi tipizzati (patch 0135): attributi union mutabili (`fillStyle`),
union con handle e buffer, con setter convertito dal tipo strutturato (`define_typed_attribute`).
Test generatore 59/59, `roves-v8` 95+5+2, default 60+2, check pulito. Copertura 261/486.

**CP72 (2026-10-06):** getter indicizzati e value iterable (patch 0136): `IndexedGetter` nei trait
nativi, interceptor registrato di nuovo nei discendenti; per `iterable<V>` i metodi di iterazione
vengono da `%Array.prototype%`. **Ora generano `Element`, `NodeList`, `DOMTokenList`.** Test
generatore 60/60, `roves-v8` 96+5+2, default 60+2, check pulito. Copertura **263/486**.
**Prossimo:** named property (`HTMLCollection`), pair iterable, `[Global]` (Window),
`[LegacyFactoryFunction]`, `[HTMLConstructor]`.

**CP73 (2026-10-06):** getter nominali con `[LegacyUnenumerableNamedProperties]` (patch 0137):
regola di visibilità (le proprietà reali vincono) e assegnazione rifiutata (ignorata in sloppy
mode, TypeError in strict). Nota: `NON_MASKING` di V8 non invoca il setter interceptor, quindi la
regola è implementata a mano. Test generatore 60/60, `roves-v8` 97+5+2, default 60+2. Copertura
**268/486**; `HTMLCollection` ora genera.

**CP74 (2026-10-06):** `[LegacyLenientThis]` (accessor senza signature, downcast senza panic) e
`[HTMLConstructor]` (patch 0138). Quest'ultimo per ora rende l'interfaccia non costruibile: i
custom element sono una lacuna documentata. Test generatore 61/61, `roves-v8` 98+5+2. Copertura
**349/486**; ora generano `HTMLElement` e quasi tutte le interfacce degli elementi.
**Prossimo:** ritorni union/enum (`getContext`, `canPlayType`), default `null` su callback e
interfacce nullable (`Document`), `[LegacyFactoryFunction]` (`Image`, `Audio`), membri ridefiniti
(`innerText`), `[Global]` (`Window`).

**CP75 (2026-10-06):** chiamate native qualificate `<T as XNative>::M(native, ..)` (patch 0139):
i membri ridefiniti nei discendenti (`innerText`) ora funzionano; i getter ereditati usano il
trait dell'antenato. Test generatore 61/61, `roves-v8` 99+5+2. Copertura **357/486**.

**CP76 (2026-10-06):** lacune di tipo (patch 0140). Enum (assegnazioni non valide ignorate),
risultati union e `ByteString`, `[Clamp]`/`[EnforceRange]` (`WebIdlType::Integer`), attributi
promise e callback interface, default `= null`. Test generatore 62/62, `roves-v8` 101+5+2,
default 61+2. Copertura **422/486 (87%)**. **Rimasti:** `[Global]` (`Window`, global dei worker),
pair iterable, `[LegacyFactoryFunction]`, namespace, costruttori sovraccarichi, callback
interface come interfacce, `[LegacyOverrideBuiltIns]`, `[LegacyUnforgeable]` sull'interfaccia
(`Location`).

**CP77 (2026-10-06):** `[BinaryName]` (`v8_native_name`), `[WebGLHandlesContextLoss]` come hint,
`[Func]` sui membri (patch 0141). **Ora generano WebGL 1/2, Canvas 2D, `HTMLCanvasElement`,
`AudioContext`, `Navigator`.** Copertura **425/486**.

**CP78 (2026-10-06):** namespace WebIDL (patch 0142). `define_namespace` crea un oggetto ordinario
con i descrittori dei membri copiati. Aggiunti `define_static_attribute` e
`define_static_overloaded_webidl_method`. **`console` e `CSS` generano.** Test: generatore 64/64,
`roves-v8` 102+5+2. Copertura **428/486**.

**CP79 (2026-10-06):** costruttori sovraccarichi (`define_overloaded_constructible_interface`,
patch 0143). **`Path2D`, `ImageData`, `OfflineAudioContext`, `MediaStream` generano.** Test:
generatore 65/65, `roves-v8` 103+5+2. Copertura **432/486**.

**CP80 (2026-10-06):** `[LegacyFactoryFunction]` (`define_legacy_factory_function`, patch 0144).
**`HTMLImageElement`, `HTMLAudioElement` e `HTMLOptionElement` generano.** Test: generatore
66/66, `roves-v8` 104+5+2. Copertura **435/486**.

**CP81 (2026-10-06):** attributi mutabili di tipo interfaccia (percorso typed, patch 0145).
Semplificazione del pilot: con `[*Throws]` entrambi gli accessor sono fallibili. **Generano
`Animation`, `AudioBufferSourceNode`, `HTMLInputElement`, `HTMLTableElement`, `MediaSession`,
`TreeWalker`, `VTTCue`.** Test: generatore 67/67, `roves-v8` 105+5+2. Copertura **442/486 (91%)**.

**CP82 (2026-10-06):** attributi statici di interfaccia e `[LegacyLenientSetter]`
(`mark_lenient_setter`, patch 0146). **Generano `Notification`, `PerformanceObserver`,
`ShadowRoot`.** Test: generatore 68/68, `roves-v8` 106+5+2. Copertura **445/486**.

**CP83 (2026-10-06):** pair iterable (`define_pair_iterable`, patch 0147). Gli iteratori sono
oggetti con un prototipo per interfaccia, e il loro stato sta in simboli privati V8. **Genera
`FormData`.** Test: generatore 69/69, `roves-v8` 107+5+2. Copertura **447/486**. Prossimo passo:
`record<K, V>` (`Headers`, `URLSearchParams`, `RequestInit`).

**CP84 (2026-10-06):** `record<K, V>` (`WebIdlType::Record`, `Value::Record`, patch 0148). In
Rust i record sono `Vec<(K, V)>`. **Generano `Headers`, `URLSearchParams`, `Request`,
`Response`.** Test: generatore 69/69, `roves-v8` 108+5+2. Copertura **453/486 (93%)**.

**CP85 (2026-10-06):** callback interface (`define_callback_interface`, solo costanti),
`[ExceptionClass]` (`make_exception_class`), `[Replaceable]` (`mark_replaceable`), costruttori
`[SecureContext]`, attributi `[EnforceRange]`, `[PutForwards]` nullable (patch 0149). Test:
generatore 70/70, `roves-v8` 109+5+2. Copertura **462/486 (95%)**. **Rimasti (24):**
- `[Global]` (`Window` e global dei worker/worklet/debugger; `GlobalScope` `[Inline]`);
- getter named enumerabili e `[LegacyOverrideBuiltIns]` (`Document`, `HTMLDocument`,
  `DOMStringMap`, `Storage`);
- maplike/setlike (`CustomStateSet`, `FontFaceSet`, `CSSFontFeatureValuesMap`);
- `[LegacyUnforgeable]` sull'interfaccia (`Location`);
- `undefined` nelle union (`URLPattern`).

**CP86 (2026-10-06):** maplike/setlike (`define_maplike`, `WebIdlLikeNatives`, patch 0150). I
natives usano i nomi di Servo (`get_index`, `size`, `has`, `get`, `set`/`add`, `delete`,
`clear`). **Generano `CustomStateSet`, `FontFaceSet`, `CSSFontFeatureValuesMap`.** Test:
generatore 71/71, `roves-v8` 110+5+2. Copertura **468/486 (96%)**.

**CP87 (2026-10-06):** `undefined` come membro di union (`WebIdlType::Undefined`, variante
`Undefined(())`, patch 0151). **Genera `URLPattern`.** Copertura **469/486**.

**CP88 (2026-10-06):** proprietà named complete (`define_named_properties`) con enumerazione,
setter e deleter, `[LegacyOverrideBuiltIns]`, più setter indicizzati
(`define_indexed_properties`) (patch 0152). Prima i setter indicizzati venivano ignorati in
silenzio. **Generano `Document`, `HTMLDocument`, `XMLDocument`, `DOMStringMap`, `Storage`.**
Test: generatore 72/72, `roves-v8` 111+5+2. Copertura **474/486 (97.5%)**. **Rimasti:**
`[Global]` (10) e `[LegacyUnforgeable]` sull'interfaccia (`Location`,
`DissimilarOriginLocation`).

**CP89 (2026-10-06):** `[LegacyUnforgeable]` sull'interfaccia (`make_unforgeable`,
`member_target`, `valueOf` intrinseco) e i `[CrossOrigin*]` come hint (patch 0153). **Corretto
un bug latente:** gli stringifier generavano un metodo JS `__stringifier` invece di `toString`.
**Generano `Location` e `DissimilarOriginLocation`.** Test: generatore 73/73, `roves-v8`
112+5+2. Copertura **476/486 (97.9%)**. **Resta solo `[Global]`** (vedi il piano sotto).

**CP90 (2026-10-06):** realm `[Global]` (`make_global`, `install_global`, `global_ref`;
`Interface` è ora un handle `Rc`) e `[Inline]` (patch 0154). Il native del global sta sul
**proxy**, l'unico oggetto con il campo interno. **Copertura del generatore 486/486 (100%).**
Attenzione: misura solo le *forme* dei binding. La produzione usa ancora SpiderMonkey. Prossima
fase:
- implementare i natives sui tipi DOM di Servo (`#[dom_struct]`/`JSTraceable` → `Trace`);
- integrare realm, script, event loop e moduli;
- poi il cutover.

**CP91 (2026-10-06, Fase 3):** nuovo crate proc-macro `roves-v8-derive` con `#[derive(Trace)]`
(patch 0155). Usa gli stessi attributi di `JSTraceable` (`#[no_trace]`, `#[custom_trace]` →
`roves_v8::CustomTrace`), e `roves-v8` implementa `Trace` per i container standard. **Nota CI:**
la V8 CI compila `roves-v8` fuori dal workspace. Per questo il derive ha versioni esplicite, il
workflow lo copia accanto, e `components/roves-v8/Cargo.lock` lo include. Test GC con mutation
check. Prossimo passo: le controparti V8 di `Dom<T>`/`MutNullableDom`/`Heap<JSVal>`/`DomRefCell`,
così che `#[dom_struct]` possa derivare `Trace`.

**CP92 (2026-10-06):** le operazioni che restituiscono Promise rifiutano, invece di lanciare,
quando falliscono la risoluzione degli overload o la conversione degli argomenti
(`mark_promise_operation`, `reject_on_failure`, patch 0156). Vale su tutti i percorsi, compresi
quelli statici e sovraccarichi.

**Audit `get_jsobject` (Phase 3):** La scope chain degli event handler
(`eventtarget.rs`) è API di compilazione dell'engine: va con la categoria (d). Restano (c)
(global per definizione interfacce, debugger, windowproxy) e (d) (Promise, structured clone,
estensioni WebGL, compile). Il toolchain locale Windows è completo: verificare ogni modifica in
locale prima della CI.

## Cutover (decisione 2026-10-08) — stato

L'utente ha approvato il cutover a V8 in produzione, e valuta di separarsi da Servo per sempre.
Il meccanismo è `components/roves-js`: l'API mozjs reimplementata su V8. Il cutover consiste nel
far puntare la riga `js = { package = "mozjs", ... }` del `Cargo.toml` radice a `roves-js`. La
strategia completa è in `docs/V8_MIGRATION.md` ("Cutover strategy"). Metrica:
`python support/v8_cutover_check.py <crate>` (baseline CP93: 22.530 errori su
`servo-script-bindings`).

- **CP93:** scaffold `roves-js` e misura.
- **CP94:** il modello dei valori.
  - Celle cppgc per i GC thing; `JSVal` con lo stesso punboxing di SpiderMonkey.
  - Stack delle radici letto al GC dal root set persistente: `rooted!`, `Handle`,
    `MutableHandle`, `Heap<T>`. Nel trace, i puntatori grezzi delle celle tornano a essere
    `Member` tramite `UnsafePtr`.
  - Marking incrementale/concorrente disattivato (in `roves_v8::initialize_engine`).
  - Test: 5.
- **Prossimi passi.** Le operazioni dell'API usata dal DOM:
  1. stringhe e conversioni (`jsstr_to_string`, `ToJSValConvertible`/`FromJSValConvertible`);
  2. oggetti e proprietà (`JS_NewObject`, `JS_GetProperty`/`SetProperty`/`DefineProperty`);
  3. eccezioni, realm, Promise, typed array.

  Poi il passaggio dei binding generati al backend V8. Rimisurare con lo script a ogni blocco.

## Piano per la prossima fase (dopo CP92): il DOM di Servo sull'heap V8

La copertura del generatore è al 100%: ora il lavoro è il cutover del DOM reale. Analisi del
codice attuale:

- **Il modello di Servo.** In `script_bindings/root.rs`, `Dom<T>` è un `NonNull<T>`. Il suo
  `JSTraceable` traccia il **reflector** del bersaglio (`trace_reflector`): l'oggetto Rust è
  posseduto dal suo `JSObject` e il grafo GC è quello di SpiderMonkey.
- **Il modello V8 (pronto in `roves-v8`).** Il native vive in un `GcBox` cppgc e traccia il
  proprio wrapper. Gli archi native→native sono `GcMember` e native→JS sono `JsRef`.
  `#[derive(Trace)]` (CP91) usa la stessa grammatica di `JSTraceable`.
- **Il ponte proposto.** Il `Reflector` di ogni oggetto DOM (primo campo di ogni
  `#[dom_struct]`) tiene, sotto `engine-v8`, un `Member` al proprio `GcBox`. In questo modo
  `Dom<T>::trace` diventa "traccia il membro del reflector del bersaglio": stessa forma di oggi
  (`trace_reflector`), ma su cppgc, senza cambiare i ~600 call site che usano `Dom<T>`.
  `MutNullableDom`/`DomRefCell` restano wrapper, con `Trace` a passthrough (`RefCell` è già
  fatto, alla maniera di `borrow_for_gc_trace`). `Heap<JSVal>` diventa `JsRef`.
- **Vincolo.** V8 e mozjs non si linkano nello stesso binario (CP32). Quindi
  `script_bindings`/`script` vanno compilati con un cfg `engine-v8` alternativo, non
  affiancato. Ordine suggerito: `Reflector` → `Dom`/`DomRoot`/`MutNullableDom` →
  `#[dom_struct]` che deriva `Trace` sotto `engine-v8` → `reflect_dom_object` su
  `allocate_traced` → realm Window (`install_global`) → `cargo check` del crate `script` con
  `engine-v8` come primo obiettivo misurabile (contare gli errori come metrica, come per la
  copertura del generatore).

## Avvio rapido

1. Leggere per intero [`V8_MIGRATION.md`](./V8_MIGRATION.md), poi l'inventario [`V8_MIGRATION_PHASE0_INVENTORY.md`](./V8_MIGRATION_PHASE0_INVENTORY.md).
2. Per ogni modifica Servo, aggiornare `CUSTOMIZATIONS.md` e aggiungere una patch applicabile in `patches/servo-v0.5.0/`; verificare la serie pristine. Mantenere aggiornati README e wiki nello stesso checkpoint; verificare la build wiki.
3. Eseguire test locali, commit/push secondo `AGENTS.md`, attendere tutte le CI rilevanti e correggere ogni failure. Aggiornare questo handoff con esiti e nuovo prossimo passo prima di proseguire.

## Direzione tecnica e limiti

Le patch 0097–0102 hanno validato cache weak wrapper isolata e identità DOM engine-neutral fino a `Dom`/`DomRoot`. Restano da sostituire in produzione rooting/tracing SpiderMonkey, wrapper DOM e runtime; il pilot non è una migrazione production. CP32 ha dimostrato che collegare V8 e mozjs nello stesso eseguibile fallisce per simboli C++ duplicati: procedere come cutover a singolo engine, non come architettura duale.

Per contesto storico consultare `CUSTOMIZATIONS.md` e la cronologia Git; evitare di duplicare qui i resoconti di ogni checkpoint già chiuso.

CP34 completo (`1b3b87a0d7e`): `Reflector` assegna un `NativeObjectId` engine-neutral monotono e
univoco nel processo, così la futura cache wrapper può usare un'identità nativa che non dipende da
indirizzi riutilizzabili. Il test mirato passa, così come il check `servo-script-bindings` con pilot
V8/JIT; rustfmt e `git diff --check` passano. Patch 0098 applica nella serie pristine. CI verde:
V8 37217091986, Android 37217091942, iOS 37217092008, Servo 37217091945 (13/13 job). Wiki build
82 pagine al commit `69d7578`. README/wiki aggiornate. Rooting/tracing e produzione restano
SpiderMonkey. Passo successivo dopo CP36: continuare la sostituzione del rooting/handle JS e tracing
per categorie di call site, senza esporre tipi V8 dalle API DOM pubbliche.

CP31 completo: il pilot genera gli argomenti enum WebIDL, converte i valori stringa, rifiuta valori
fuori dall'enum prima del callback e applica i default opzionali. Verifiche locali: 26 test
generatori; pilot normale/JIT-less ciascuno con 64 unit + 3 integrazioni + 2 doctest; default 52
unit + 2 doctest; check integrato `servo-script`. Patch 0095 valida nell'intera serie pristine;
wiki build 82 pagine al commit 01270c6. CI verde: V8 37201595906; Servo 37202006567 (patch
validation, SDL3, Steam, sei bundle); Android 37202073963; iOS 37202073968. Produzione resta
SpiderMonkey. I passi successivi di conversione WebIDL sono proseguiti nei checkpoint seguenti; il prossimo lavoro attivo e registrato nel blocco CP33 qui sopra.

CP32 e completo: il test generato dal vero `ValidityState.webidl` verifica 11 attributi, descrittori,
`instanceof` e i dieci flag nativi. CP32 resta pilot-only; la CI V8, Servo (patch validation, SDL3,
Steam e sei bundle), Android e iOS e verde. Un test eseguibile nello stesso crate Servo fallisce al
link per simboli C++ duplicati tra mozjs e V8; il `cargo check` integrato passa. Il runtime test
resta nel binario isolato V8 fino alla rimozione di SpiderMonkey dal binario Servo.

CP33 implementazione completata localmente: `Runtime::create_instance_with_identity` usa una cache `HashMap` di weak wrapper,
con token per impedire a un vecchio finalizer di rimuovere una voce sostitutiva. Testa riuso via
`===`, factory lazy, cleanup esatto dopo GC e ricreazione. Suite locali normali/JIT-less: 66 unit +
4 integrazioni + 2 doctest; default 54 unit + 2 doctest. Patch 0097 e validazione pristine passano;
26 test generatori e `cargo check` integrato di `servo-script-bindings` passano. Tutta la CI è verde:
V8 37213254328, Android 37213254318, iOS 37213254280 e Servo 37213254289 (13/13 job: overlay,
SDL3, Steam, tutti i sei bundle, layout/paint-api e smoke test Linux/macOS/Windows). Wiki build 82
pagine verde al commit `43e941d`; doc CP33 pubblicata in `roves-wiki`.

Prossimo lavoro: audit e migrazione per categorie dell'identità DOM, rooting e tracing. L'audit
concreto conferma che `Reflector` contiene `Heap<*mut JSObject>`, `root.rs` basa `StableTraceObject`
su `js::gc::Traceable`, e `trace.rs` usa `CallObjectTracer`; i 644 riferimenti non sono tutti
equivalenti, perché molti richiedono direttamente API SpiderMonkey. Definire prima un contratto
engine-neutral per identità nativa e wrapper/root posseduti dal runtime di scripting, poi migrare
gruppi di call site mantenendo la compilabilità. Nessun tipo `v8::*` deve arrivare nelle API DOM
pubbliche o shell. Produzione continua a collegare SpiderMonkey finché la sostituzione end-to-end
non è pronta.
CP26 (`b03e711f304`) è completo e verde. Verifiche locali: 21 test Python; pilot normale e JIT-less ciascuno con 59 unit + 3 integrazioni + 2 doctest; default 52 unit + 2 doctest; check integrato `servo-script`. Patch 0090 applica sulla serie pristine. CI verdi: V8 ([37144196093](https://github.com/DRincs-Productions/roves/actions/runs/37144196093)), Servo (patch validation, SDL3, Steam e tutti i bundle: [37144196126](https://github.com/DRincs-Productions/roves/actions/runs/37144196126)), Android ([37144196111](https://github.com/DRincs-Productions/roves/actions/runs/37144196111)), iOS ([37144196107](https://github.com/DRincs-Productions/roves/actions/runs/37144196107)). Wiki aggiornata e build Next.js verde al commit `92ac3ac`.

CP27 (`19bb9c8cd2e`) è completo e verde. ByteString operation arguments mappati a `Vec<u8>`; ToString valida ogni code unit <=255, per le forme required, nullable e optional. Verifiche locali: 22 test generatori; pilot normale/JIT-less ciascuno con 60 unit + 3 integrazioni + 2 doctest; default 52 unit + 2 doctest; check `servo-script`. Patch 0091 applica alla snapshot CP26 e al pristine nella CI. CI verdi: V8 ([37147151024](https://github.com/DRincs-Productions/roves/actions/runs/37147151024)), Servo (patch validation, SDL3, Steam e tutti i bundle: [37147151005](https://github.com/DRincs-Productions/roves/actions/runs/37147151005)), Android ([37147150994](https://github.com/DRincs-Productions/roves/actions/runs/37147150994)), iOS ([37147151023](https://github.com/DRincs-Productions/roves/actions/runs/37147151023)). Wiki build Next.js verde al commit `d36c050`.

CP28 (36da94054a7) complete and green: optional boolean/numeric defaults, including float and nullable types. Typed IDLValue/IDLNullValue; tests verify false/zero and infinity handling. String and non-primitive defaults still fail closed. Local: 23 Python tests; pilot normal/JIT-less each 61 unit + 3 integration + 2 doctests; default 52 unit + 2 doctests; integrated servo-script check; patch 0092 applies to CP27; wiki build 82 pages. CI green: V8 37151335289, Servo 37151335249 (six bundles, Steam, SDL3, patch validation), Android 37151335284, iOS 37151335248. Wiki commit 56203c4.
CP29 (9762ac25439) complete and green: optional WebIDL string defaults for DOMString (UTF-16 vector), USVString (escaped scalar-valid Rust string) and ASCII ByteString (byte vector). Runtime covers literal backslash, supplementary Unicode, null and explicit input. Local green: 24 generator tests; pilot normal/JIT-less each 62 unit + 3 integration + 2 doctests; default 52 unit + 2 doctests; integrated servo-script check. Patch 0093 applies to the CP28 snapshot. Wiki build generates 82 pages; commit 462ed36 is published. CI green: V8 37189196623; Servo patch validation, SDL3, Steam and six bundles 37189196591; Android 37189196574; iOS 37189196578.

CP30 (`b79c3445897`) complete and green: optional nullable DOMString/USVString/ByteString defaults declared as null. Local green: 25 generator tests; normal and JIT-less each 63 unit + 3 integration + 2 doctests; default 52 unit + 2 doctests; integrated servo-script check. Patch 0094 pristine-series validation green; wiki 82-page production build at 1fcf64f. CI green: V8 37192646577, Servo patch validation/SDL3/Steam/six bundles 37192646570, Android 37192646472, iOS 37192646529.

Il checkpoint 25 è pubblicato con commit `6525db014e0`; patch 0089 applica pulitamente. Aggiunge argomenti required nullable per boolean, tipi numerici, `DOMString` e `USVString`: `null`/`undefined` diventano IDL null (`None`), altrimenti si applica la conversione del tipo interno. Verifiche locali complete: 20 test Python; pilot normale/JIT-less ciascuno con 58 unit + 3 integrazioni + 2 doctest; default 52 unit + 2 doctest; check integrato `servo-script`. CI verde: V8 ([37141033528](https://github.com/DRincs-Productions/roves/actions/runs/37141033528)), Servo con sei bundle/Steam/SDL3/patch validation ([37141033571](https://github.com/DRincs-Productions/roves/actions/runs/37141033571)), Android ([37141033507](https://github.com/DRincs-Productions/roves/actions/runs/37141033507)), iOS ([37141033514](https://github.com/DRincs-Productions/roves/actions/runs/37141033514)). Produzione resta SpiderMonkey.

Il checkpoint 24 e pubblicato con commit `02b695f74d0`; patch 0088 applica pulitamente. Aggiunge argomenti obbligatori DOMString/USVString con preservazione UTF-16 o scalar-value, coercizione ToString e propagazione degli errori. Verifiche locali complete: 19 test Python; pilot normale/JIT-less ciascuno con 57 unit + 3 integrazioni + 2 doctest; default 52 unit + 2 doctest; check integrato `servo-script`. Tutte le CI sono verdi: V8 6/6 ([37137152487](https://github.com/DRincs-Productions/roves/actions/runs/37137152487)), Servo bundle/test/Steam/patch validation ([37137152516](https://github.com/DRincs-Productions/roves/actions/runs/37137152516)), Android ([37137152568](https://github.com/DRincs-Productions/roves/actions/runs/37137152568)), iOS ([37137152538](https://github.com/DRincs-Productions/roves/actions/runs/37137152538)). Produzione ancora SpiderMonkey.

Il checkpoint 23 e stato pubblicato con commit `09ffe10fdcf`; patch 0087 e la CI sono verdi. Aggiunge conversioni degli argomenti numerici WebIDL interi e `float`/`unrestricted float`. Verifica locale: 18 test Python, pilot normale/JIT-less, default e cargo check integrato; run V8 37132532003, Servo 37132531959, Android 37132531996, iOS 37132532177.

Il checkpoint 22 e pubblicato con commit `323649b84f7` e patch 0086; aggiunge argomenti richiesti `double`, `unrestricted double` e `unsigned long`. Omissione e conversioni da Symbol falliscono prima del callback. Passano 18 test Python, runtime normale/JIT-less/default e check integrato; CI verde: V8 37117775121, Servo 37117775170, Android 37117775250, iOS 37117775238. Produzione resta SpiderMonkey.

Verifica CP23: `cargo test -p roves-v8 --features webidl-pilot` e la variante `webidl-pilot,jitless` passano con 56 unit + 3 integrazioni + 2 doctest ciascuna; il default passa con 52 unit + 2 doctest. `AWS_LC_SYS_NO_ASM=1 cargo check -p servo-script --features v8-bindings-pilot,js_jit --offline --locked` passa. Patch 0087 si applica pulitamente.
Il checkpoint 21 corregge la distinzione tra tipi floating-point WebIDL ristretti (`float`/`double`, nativi `FiniteF32/F64`) e `unrestricted float/double`, che conservano NaN e infinito. I tipi finiti non possono essere costruiti con valori non finiti; gli attributi mutabili `double` ora generano TypeError prima di cambiare stato se ToNumber produce NaN o infinito. L-adapter Screen usa il contratto finito sotto feature pilot. Passano 16 test Python, runtime normale/JIT-less/default e `cargo check` integrato di `servo-script`; patch 0085 e CI verdi: V8 6/6 (37108919478), sei bundle Servo con Steam e patch validation (37108919488), Android (37108919483), iOS (37108919477). Wiki non aggiornabile perché checkout `roves-wiki` assente.

Il checkpoint 20 estende i ritorni numerici delle operazioni a `byte`, `octet`, `short`, `unsigned short`, `long`, `long long`, `unsigned long long` e `float`, inclusi `float?` e `long long?`. I numeri WebIDL si mappano al tipo Rust corrispondente e a JavaScript Number; `Option<T>::None` diventa `null`. Passano 16 test Python, 56 unit + 3 integrazioni + 2 doctest in pilot normale e JIT-less, 52 unit + 2 doctest default e `cargo check` integrato. Patch 0084 si applica pulitamente. CI verde: V8 6/6 (37061193384), sei bundle Servo più Steam e patch validation (37061193356), Android (37061193392), iOS (37061193362). Il checkout `roves-wiki` manca, quindi non è stato possibile aggiornare la wiki.

Il checkpoint 18 supporta i ritorni `DOMString`, `DOMString?`, `USVString` e `USVString?` per le operazioni senza argomenti. `DOMString` conserva i code unit UTF-16, `USVString` usa stringhe Unicode scalari, e i risultati nullable preservano `null`. Passano 14 test Python, i runtime normali/jitless, il default e il `cargo check` integrato. Patch 0082 e CI sono verdi: matrice V8 6/6 (37041208311), sei bundle Servo con Steam e patch validation (37041208274), Android (37041208279), iOS (37041208300).

Il checkpoint 17 aggiunge parametri WebIDL obbligatori di tipo `boolean` alle operazioni a firma singola supportate. La truthiness JavaScript segue la conversione IDL; argomenti omessi diventano `false`, gli argomenti aggiuntivi sono ignorati. Parametri opzionali, nullable, variadici e altri tipi restano rifiutati. Passano 13 test Python, i runtime pilot normale/jitless, il runtime default e il `cargo check` integrato. Patch 0081 e CI sono verdi: matrice V8 6/6 (37036084532), sei bundle Servo con Steam e patch validation (37036084693), Android (37036084595), iOS (37036084598).

Il checkpoint 16 aggiunge metodi WebIDL senza argomenti e senza overload, con ritorni `undefined`, `boolean`, `double` e `unsigned long`. Il generatore usa i callback nativi gi? presenti e mantiene il controllo del receiver. Overload, parametri e tipi di ritorno non ancora supportati falliscono esplicitamente. Passano localmente 11 test Python, i runtime normali/jitless e il `cargo check` integrato dei binding. Patch 0080 e CI sono verdi: V8 37029579196, Servo 37029580090, Android 37029579200, iOS 37029579293.

Il checkpoint 15 aggiunge attributi WebIDL nullable `boolean?`, `double?` e `unsigned long?`, con `Option<T>` sul lato Rust, conversione diretta di `null` e le normali regole WebIDL per gli altri valori. La conversione precede la mutazione nativa; errori da Symbol preservano lo stato. Passano localmente 10 test Python, 56 unit test + 3 integrazioni + 2 doctest in pilot normale e jitless, 52 unit + 2 doctest senza pilot, e il `cargo check` del crate `servo-script-bindings` con pilot e `js/jit`. Patch 0079 e CI sono verdi: overlay e sei bundle, Steam inclusi (37024609945), matrice V8 6/6 (37024610041), Android (37024609737), iOS (37024609529).

Il checkpoint 14 aggiunge `USVString` e `USVString?` al generatore opt-in: i valori diventano `String`/`Option<String>` dopo la conversione scalar-value (i surrogate isolati sono sostituiti da U+FFFD); `null` nullable resta `null`. Passano localmente 56 unit test, 3 integrazioni e 2 doctest con pilot normale e jitless; 52 unit test + 2 doctest senza pilot; 9 test Python. La patch 0078 e la CI sono verdi: matrice V8 6/6 (run 37016952782), Servo completo con sei bundle, Steam e overlay (37016951496), Android (37016951470), iOS (37016951542). La produzione resta SpiderMonkey.

Il checkpoint 13 aggiunge `DOMString?` mutabile, preservando i code unit UTF-16 e `null`. Patch 0077 e CI sono verdi: run Servo 37009800175, Android 37009800200 e iOS 37009800118. La matrice V8 6/6 del commit sorgente 0077 era verde nel run 37004723223. Il test Steam attende ora il marker della pagina invece di un ritardo fisso.

Il checkpoint 12 aggiunge setter mutabili `boolean`, `double` e `unsigned long`; il checkpoint 11
aggiunge setter `DOMString`. Sul commit `de49cb9`, patch validation, Android, iOS e i bundle
salvo Linux deb sono passati; Linux portable ha completato il bootstrap, mentre Linux deb e Steam
hanno raggiunto il timeout di 45 minuti scaricando dipendenze apt. Il mirror Azure non ha risolto
il problema. Il commit `dbc6b9d` aumenta a 90 minuti i timeout del bootstrap Linux; la run Servo
37000099322 e ancora in corso, mentre iOS e Android sono passati.

Il README dichiara gia che le release usano ancora SpiderMonkey e che `roves-v8` e un runtime
isolato in fase di validazione. Il checkout adiacente `roves-wiki` non e presente, quindi la
wiki non e stata aggiornata. `roves-action` non e presente; questa modifica CI non cambia flag,
default o convenzioni di bundle dell'action.

Il checkpoint 7 avvia l'integrazione vera con `components/script_bindings`: un backend opt-in
usa il parser WebIDL esistente e genera `ValidityState` dalla definizione Servo reale, senza
toccare l'output predefinito del generatore SpiderMonkey. Il checkpoint 8 aggiunge anche
`Screen.webidl`, con tipi `double` e `unsigned long`. `v8-bindings-pilot` compila le binding
generate come percorso aggiuntivo; non seleziona V8 in produzione. Il checkpoint 9 collega la
binding `Screen` al tipo DOM nativo Servo con un adapter opt-in; non installa ancora l'oggetto in
un realm V8 e non sostituisce il reflector SpiderMonkey. Il checkpoint 10 aggiunge conversione
UTF-16 lossless e la binding WebIDL `DOMString`, compresi i code unit di surrogate isolate. Sono
passati localmente sei test Python; 53 unit test runtime, tre integrazioni WebIDL e due doctest
sia in modalità normale sia JIT-less; e 52 unit test più due doctest senza feature pilot. Build
Windows complete di `servo-script-bindings` e `servo-script` con i feature pilot erano riuscite
nei checkpoint precedenti. Per compilare `servo-script` è servito `AWS_LC_SYS_NO_ASM=1` perché
NASM non è installato localmente. La matrice V8 6/6 e l'overlay fino a 0072 sono verdi. Per il
commit e521bf0, `validate-servo-patches` (inclusa 0073) è passato; il workflow completo Servo è
ancora in esecuzione. La matrice V8, Android e iOS del checkpoint precedente è verde. Il job Steam della CI Servo per 31c2ae6 ? scaduto due volte nel bootstrap apt: il mirror standard scaricava lentamente e ha installato solo 272 pacchetti in 45 minuti. Il job ora usa il mirror Azure, che ha installato circa 500 pacchetti in 18 minuti in un altro job Linux, pi? la cache uv e il timeout di 45 minuti.

`codegen.py` è stato esteso solo con la classe opt-in `CGV8BindingRoot`; il percorso SpiderMonkey
predefinito è rimasto senza modifiche di output (1.441 file generati confrontati prima dell'ultimo
controllo di esposizione). Il pilot accetta solo interfacce esposte a Window, attributi di istanza
readonly boolean/`double`/`unsigned long`/`DOMString` senza attributi estesi, e senza
ereditarietà/costruttore. Tutte le altre forme falliscono esplicitamente. Patch overlay: 0071
aggiunge la prima interfaccia, 0072 aggiunge i numerici, 0073 collega l'adapter nativo Screen,
0074 aggiunge le stringhe UTF-16 lossless.
Il file `CLAUDE.md` è stato rinominato in `AGENTS.md`, entry point condiviso per tutti gli agenti.

Checkpoint 10 (`0074`, commit `9412dfe`) e checkpoint 11 (mutabili DOMString, in sviluppo locale)
sono dettagliati sopra e nel piano. La CI del commit 9412dfe è verde per matrice V8 (6/6), Android,
iOS e validazione overlay; i bundle macOS hanno mostrato che serve aggiungere Homebrew `ld.lld`,
ma lasciare il clang Apple selezionato: Homebrew `clang++` causa un panic in bindgen/mozangle.
Il workflow ora espone solo il binario della formula `lld`. Dopo la
patch 0075, proseguire con le conversioni WebIDL e poi wrapper/realm solo quando l'ownership è
validata. Il pilot non è il runtime di produzione; il piano resta aperto fino a tutti i criteri di
`V8_MIGRATION.md`.

## Stato reale, in breve

- **Fase 1 (isolate/context/eval)**: completa.
- **Fase 2 (conversione valori, Promise, moduli ES)**: completa.
- **Fase 3 (ownership/GC)**: primitivi isolati validati in `roves-v8` (`create_wrapped`/`get_wrapped`, finalizzazione garantita e stress test dei cicli fino a 200 oggetti). La sostituzione in produzione di rooting/tracing SpiderMonkey, l'identita dei wrapper DOM e il collegamento al GC DOM restano aperti.
- **Fase 4 (binding WebIDL/DOM)**: pilot incrementato fino al CP32, con generazione WebIDL opt-in in `components/script_bindings/codegen.py`, adapter del DOM `Screen` e runtime verificato in `components/roves-v8`. Copre attributi e operazioni con conversioni primitive e stringhe, metodi e fixture runtime. Nessun percorso di produzione è ancora passato da SpiderMonkey a V8; restano wrapper DOM, interfacce, semantiche WebIDL e integrazione runtime. La fase è aperta.

**Perché tutto è isolato in `roves-v8` e non in produzione:** ispezionando una build reale
(`target/debug/build/servo-script-bindings-*/out/Bindings/`) si sono trovati **522 file di
binding generati**, ~700 righe anche per il più semplice (`ConsoleBinding.rs`). I moduli di
supporto scritti a mano che quei binding usano (`components/script_bindings/interface.rs`,
`proxyhandler.rs`, `finalize.rs`) sono **mutuamente interdipendenti** attorno al modello a
oggetti `JSClass` di SpiderMonkey — non si può sostituirne uno solo in isolamento (es. un
finalizzatore V8 non ha senso se l'oggetto è ancora creato con `JSClass`). Per questo si è
scelto di costruire e validare i primitivi di ownership (create_wrapped/finalizzazione
garantita/identità di interfaccia/proprietà/metodi) dentro una crate isolata **prima** di
toccare quei file di produzione — l'utente ha confermato esplicitamente questa scelta durante la
sessione.

## Bug reali trovati durante il lavoro (utile saperli prima di scrivere altro codice V8)

Tutti documentati per esteso in `CUSTOMIZATIONS.md` e nei commenti doc di `lib.rs`, qui solo
l'elenco rapido:

1. Le scope di questa versione della crate `v8` sono `!Unpin` e richiedono le macro
   `v8::scope!`/`v8::tc_scope!`, non `HandleScope::new`/`TryCatch::new` diretti.
2. Un solo isolate V8 può essere "entered" per thread alla volta (serve `Locker` per condividerne
   uno tra thread).
3. Il `Runtime` deve avere **un contesto persistente condiviso**, non uno nuovo per chiamata —
   altrimenti dati registrati in una chiamata (es. una funzione nativa) sono invisibili a una
   chiamata successiva.
4. Il tag per `set_aligned_pointer_in_internal_field` deve essere un valore piccolo (0 funziona,
   un valore arbitrario come `0xC0DE` fa abortire l'intero processo).
5. `FunctionCallbackArguments::data()` restituisce un `Local<Value>` diretto, non un `Option`.
6. **`PropertyCallbackArguments` non ha modo di recuperare il vero destinatario (`this`)** — solo
   `.holder()`, che per un accessor su un prototipo condiviso restituisce il prototipo stesso, non
   l'istanza. Ogni lettura tornava silenziosamente `Undefined`. Fix: installare l'accessor
   sull'**instance template**, non sul prototype template.
7. Sorpresa positiva: nonostante il fix sopra, `FunctionTemplate::inherit` propaga comunque gli
   accessor su instance template lungo la gerarchia — l'ereditarietà funziona gratis.
8. **`get_function()` materializza il prototipo *subito*, in modo permanente.** Chiamarlo
   dentro `define_interface` prima che tutte le `define_method`/`define_property` fossero
   registrate rompeva silenziosamente i metodi aggiunti dopo ("X is not a function"). Fix:
   l'esposizione del costruttore globale è rimandata alla prima `create_instance` per quella
   interfaccia.
9. Un valore `u16` arbitrario come tag di campo interno fa abortire V8 (vedi punto 4) — stesso
   bug categoria, voce separata perché è stato il primo trovato.

## Note operative per chi continua

- **Workflow di verifica**: per ogni modifica a `components/roves-v8/`, genera sempre una patch
  nuova in `patches/servo-v0.5.0/` (vedi `../AGENTS.md`, sezione "keep CUSTOMIZATIONS.md and
  patches up to date") — **anche per file nuovi**, non solo modifiche: è stato un errore reale
  fatto e corretto in questa sessione (vedi CUSTOMIZATIONS.md, entry "V8 migration Phase 1" per
  il dettaglio del fix).
- **Build locale funzionante**: questa macchina ha ora un toolchain Windows completo e verificato
  (LLVM/`lld-link.exe`/`libclang`, MSVC Build Tools, `depot_tools`, GStreamer reale) — vedi la
  memoria `project_roves_local_build_toolchain.md`. `cargo test -p roves-v8` gira in pochi
  secondi; non serve aspettare la CI per iterare.
- **CI**: `test.yml` si riattiva solo toccando `patches/**`/`test-page/**`/se stesso — un commit
  vuoto non basta. L'API GitHub anonima ha un rate limit (60/ora) facilmente superabile se si fa
  polling troppo stretto durante l'attesa della CI — spaziare le richieste.
- **Quirk noto**: lo strumento Edit può silenziosamente convertire un intero file da LF a CRLF
  — verificare con `file <path>` prima di committare (vedi memoria
  `feedback_edit_tool_crlf_quirk.md`).
- **Autonomia**: continuare senza pause o approvazioni intermedie. Scegliere autonomamente i passi tecnici; chiedere una decisione soltanto quando nessun percorso sicuro e utile puo avanzare senza input dell'utente.

## Continuation checkpoint — 2026-10-01

At that stage, work continued with isolated prototyping and indexed read interception before the
production generator jump. Added checkpoint 6 in `roves-v8`, patch
0069, six tests (47 total), passing locally with and without `--features jitless`.
`FunctionTemplate::inherit` does not propagate indexed interceptors: explicitly register on
children. This is not a complete collection binding: query/enumeration/descriptors,
assignment/deletion, named handlers and wrapper-valued results still need implementation.
Registration must precede creation of any interface/descendant instance; only direct late
registration is guarded. Production rooting replacement and wrapper reuse also remain open;
Phase 3 being described as complete above refers only to tested ownership primitives.
See CUSTOMIZATIONS.md and the Phase 4 status note for the precise scope.

## Autonomous continuation - ownership safety

The user requires continued implementation, local testing, CI and fixes until all migration
criteria are met. Shared instructions are now AGENTS.md. Ownership hardening in patch 0070
fixes get_wrapped's runtime lifetime, guards ancestor materialization, owns setter callback
allocations, and sweeps completed finalizers. 49 unit tests and 2 compile-fail doctests.
A separate v8.yml matrix runs normal/JIT-less tests on all desktop platforms. Production
WebIDL migration remains the next implementation step, starting from a small real interface;
do not claim that prototype ownership coverage completes Phase 3 in production.
