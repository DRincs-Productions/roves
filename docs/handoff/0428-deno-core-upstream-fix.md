# 0428 — fix temporaneo deno_core autorizzato

## Direzione e provenienza

L'utente ha autorizzato il29ottobre fork e patch per difetti che impediscono il funzionamento
di giochi, issue/PR upstream e TODO di ricontrollo. La precedente domanda0425 e risolta:
non richiederla di nuovo. Windows sara sbloccato piu tardi; nessuna prova UI/clic ottenuta.

Il vecchio denoland/deno_core e archiviato dopo il trasferimento nel monorepo denoland/deno,
attivo, sotto libs/core. La crate0.413.0 indica quel repository e source revision
fe7ed012d92c642fd8e5607ac5a45d517538b5c5. Non scambiare l'archivio vecchio per abbandono.

- Fork attivo: https://github.com/BlackRam-oss/deno
- Branch: codex/fix-cached-static-tla
- Commit upstream:49a03aaf1ae59c864539c88f510ef0582073826e
- Issue:https://github.com/denoland/deno/issues/37097
- PR in bozza:https://github.com/denoland/deno/pull/37098

Issue/PR sono state create su autorizzazione esplicita. Il template Deno richiede disclosure
AI: inclusa. Nuovo test Rust formattato secondo configupstream (80/2/2024), corpo eseguito
come embeddingteststandalone; ./xfmt,/xlint e suite completa monorepo NON eseguiti localmente.
Nessuna firmaCLA o merge automatico effettuati. PR collegata a questa chat.

## Implementazione Roves

Snapshot della sola crate pubblicata in components/roves-js/vendor/deno_core, licenza MIT
preservata e LICENSE.md ufficiale inclusa. Solo modifica semantica: escludere grafi async
dal fast path ModuleStatus::Evaluated, lasciando a Deno la vera promise di valutazione.
Il vecchio adapter Roves basato su mod_evaluate resta: niente shim import interno0424,
grafo aggiunto, parser JS o API privilegiata. V8 resta150.4.0, deno_v80.5.0, un solo runtime.

Cargo root e Cargo standalone roves-js applicano lo stesso snapshot con patch.crates-io.
Il patch dichiarato nel membro viene ignorato da Cargo quando governa la workspace root,
che possiede il proprio identico override; nel test standalone vale quello del pacchetto.
Non usare pathdependency diretta: Cargo la aggiunge automaticamente ai membri e risolve
feature/devdeps del pacchetto upstream. L'override standard mantiene lo snapshot fuori
dai membri. File Cargo.lock root/standalone aggiornati solo per il source di deno_core.

Packaging: esclusi esempi/benchmark (compreso wasm binario non usato dalla libreria), target
Cargo relativi e marcatori della cache registry; sorgenti libreria/defaultfeatures/deps
preservati. Provenienza, SHA256crate e criteri di rimozione in vendor/deno_core/ROVES_PATCHES.md.
Snapshot esterno mantenuto upstream, non nuovo codice Roves da contare come risparmio.
Non serve installare DenoCLI o Chromium. Nessuna cache/registry locale modificata.

## Prove

- Suite reale Roves sulla crate distribuita:117normal/116JITless,2/3ignored preesistenti.
- Matrice root/descendant resolve/reject, pending prima del resolver, stessa eccezione,
  namespace/cache identici, fetch/esecuzione una volta; ownership e teardown117/116 compresi.
- Testimportmap verifica root HTML invariata e risoluzione normale degli import del gioco.
- Corpo test PR upstream eseguito standalone contro lo snapshot: passa.
- Metadata CI-style standalone copiafisica: --locked, una crate Deno locale, un V8150.4.0,
  un solo membro workspace; qualche sorgente transitorio di piattaforme nonhost e stato
  scaricato per completare metadata, non nuovo buildbrowser.
- Fixture browser ora23check: nuova matrice con veri moduli statici radice/discendente,
  resolver successo/rifiuto e cache. Chrome156 MCP passa il nuovo check (929x861), non e
  una prova del fork nel browser Servo. NamespaceinlineC3 resta nel test separato.
- Node syntax pagina e tre .mjs: passa. Runner/gate Linux10test verdi.
- Build completo browser SOLO online, da confermare dopo il push del codice completo.

Log target/task-0428/{native-normal,native-jitless,upstream-body,runner-gates}.log.
Nativeworkspace usa override allo snapshotreale: nessun prototipo0424. Non ripristinare
vecchi backupCargo o proposed-tests.rs intero; i test nuovi sono gia portati sull'HEAD.

## 0429 — gate build indispensabile

support/v8_cutover_check.py prima restituiva0anche se Cargo falliva. In --build ora propaga
lo status Cargo, impedendo al workflow di provare un binario vecchio rimasto in cache.
La modalita --check resta diagnostica a conteggi. TestPython passa con4casi; negativa sul
vecchio helper produce2fallimenti, nessun cargo reale invocato. Ripristino manifest/lock
verificato anche suerrore. Testaggiunto alla CI runtime. Non dire che le vecchie CI fossero
certamente stale: e un rischio del gate corretto, non un falsoverde osservato in quei run.

## Pubblicazione e seguito

Conservare TODOutente preesistente (blankline non staged): solo nuova sezione fork e staged.
Aggiornare patch Servo applicabili con snapshottesto e license, replay da copia pulita,
documenti e linkwiki/action. Prima push verificare batch f240 richiesto terminale positivo;
poi identificare nuove run una volta e continuare loop senza pollingcontinuo. Apple esclusa.
TODO ricontrolla issue/PR a ogni upgrade e prima release stabile; rimuovere snapshot e i due
override soltanto con crateufficiale corretta e regressioni native/browser/sicurezza verdi.
SourceText0361, pausa provvisoria, retention complessiva e prove UI restano distinti aperti.

## Replay finale

Patch0428 e0429 applicate dopo dry-run al replay upstream esistente;126 file uguali
con LF normalizzati, TODO confrontato con indice preservando la riga utente non staged.
Root Cargo.lock resta metadata del progetto, fuori overlay Servo come nei checkpoint precedenti.
Una whitespace preesistente nel sorgente upstream unique_arena.rs:50 e conservata nello snapshot.
