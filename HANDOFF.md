# Handoff — riprendere da un altro PC (2026-09-14)

Nota temporanea di passaggio, non documentazione permanente — cancellabile una volta
ripreso il lavoro. Lo stato "vero" a lungo termine resta in `TODO.md` (punto 8) e
`CUSTOMIZATIONS.md`; questo file serve solo a non perdere il filo passando macchina.

## Cosa è già fatto e pushato (tutto su `main`, niente in sospeso localmente)

- **Motore** (`DRincs-Productions/roves`, ultimo commit `6e7fa3112`): `mach bundle --ios`/
  `--ios-release` reali (`_bundle_ios`/`_sign_and_export_ios_release` in
  `python/servo/post_build_commands.py`), firma via `IOS_SIGNING_CERTIFICATE_P12_PATH`/
  `_PASSWORD`/`IOS_SIGNING_PROVISIONING_PROFILE_PATH`/`IOS_SIGNING_TEAM_ID` (stesso schema
  di `--android-release`). Test in `tests/mobile/test_packaging.py`. Patch
  `patches/servo-v0.5.0/0016-mobile-native-webviews.patch` rigenerata e riverificata contro
  pristine v0.5.0. `TODO.md` punto 8 e `CUSTOMIZATIONS.md` aggiornati con tutto il dettaglio.
- **`roves-action`** (commit `a83e5df`): nuovi input `android-release`/`android-keystore-*` e
  `ios-release`/`ios-certificate-*`/`ios-provisioning-profile-*`/`ios-team-id`.
- **`roves-packmaster`** (commit `7d04316`): sezione iOS completa (`ios.rs`/`ios_signing.rs`
  + card in `configure.tsx`, i18n in tutte e 9 le lingue), parallela a quella Android (che
  era **già completa** prima di questa sessione — non un gap come inizialmente sospettato).
- **`roves-wiki`** (commit `7ab499b`): pagine `mobile.mdx`/`packmaster.mdx` aggiornate.
- **Segreti GitHub configurati** (dall'utente, confermato su entrambi `roves` e
  `roves-action`): `ANDROID_KEYSTORE_BASE64`/`_PASSWORD`, `ANDROID_KEY_ALIAS`/`_PASSWORD`
  (keystore Android reale, autofirmata, valida 30 anni) e `IOS_CI_TEST_P12_BASE64`/
  `_PASSWORD` (certificato iOS self-signed, **solo per testare il meccanismo in CI**, non
  per una firma di distribuzione reale).

## Bug aperto da diagnosticare per primo

Il job **`ios`** di `.github/workflows/ios.yml` (quello che esegue `mach bundle --ios` per
davvero su un runner macOS) fallisce con un generico "Process completed with exit code 1" —
nessun dettaglio nelle annotation pubbliche (i log grezzi richiedono permessi admin sul
repo, non leggibili in forma anonima).

**Ipotesi principale, non ancora verificata**: `python/servo/command_base.py`'s
`common_command_arguments(binary_selection=True, ...)` decoratore (usato da `bundle()`)
probabilmente salta la risoluzione del binario compilato **solo** per Android
(`is_android(self.target)` / `self.target.needs_packaging()`), non per iOS — quindi
`mach bundle --ios` potrebbe fallire perché cerca un `servoshell` mai compilato in CI
(questo job non esegue mai `mach build` prima). Prossimo passo: leggere
`command_base.py` righe ~555-610 (`if binary_selection: ... if self.target.needs_packaging(): ...`)
e capire se serve un branch analogo per iOS, oppure se basta far sì che `--ios` non passi
mai per quel percorso di risoluzione binario (mirror di come `--android` lo evita).

**Per confermare l'ipotesi servirebbe un GitHub PAT** (l'utente ha detto di darne uno ma non
è mai arrivato in chat) per leggere il log reale invece di ragionare a tentativi — chiedere
di nuovo se serve, oppure procedere per tentativi via push + retry (più lento).

I job `android-release-signing`/`ios-release-signing-smoke` (verifica firma) non sono
ancora stati ri-testati con i secret appena configurati — l'ultimo retry (`6e7fa3112`) è
partito ma non ne ho ancora controllato l'esito.

## Decisione ancora in sospeso (chiesta esplicitamente all'utente, risposta: "aspetta")

Per far sì che `roves-action`/Packmaster possano **davvero** usare `--ios`/`--ios-release`
(oggi puntano a un tag motore già pubblicato che non li contiene), serve tagliare una nuova
release del motore (`git tag vX.Y.Z && git push origin vX.Y.Z`, vedi `CLAUDE.md`, "Cutting a
versioned release") e poi aggiornare il tag pinnato in `roves-action/action.yml` e
`roves-packmaster/src/lib/shell-version.ts`'s `TARGET_SHELL_VERSION`. **Non ancora fatto,
in attesa di conferma esplicita** — l'utente ha chiesto di rivedere prima il codice.

## File locali che esistono SOLO su questa macchina (non nel repo, non su un altro PC)

Nella cartella scratchpad di questa sessione (temporanea, potrebbe essere ripulita dal
sistema):
`C:\Users\simon\AppData\Local\Temp\claude\c--Users-simon-3D-Objects-roves\c1e80f05-941c-4417-b914-81516505440e\scratchpad\signing\`

- `android-keystore-secrets.txt` + `roves-release.keystore`/`.keystore.b64` — la keystore
  Android reale. **I valori sono già caricati come secret GitHub, ma il file della keystore
  stessa non è recuperabile da GitHub se lo perdi** (i secret sono write-only). Consigliato
  fare un backup del file `.keystore` (e della password) in un posto sicuro e durevole
  prima che questa cartella temporanea venga ripulita.
- `ios/distribution.csr` + `distribution.key` — la CSR da sottomettere al portale Apple
  Developer (org "DRincs Productions") per ottenere il certificato di distribuzione reale.
  **Non ancora sottomessa.** Se questi file si perdono va rigenerata una nuova CSR (nessun
  danno grave, ma da rifare).
- `ios/ci-test.p12`/`.p12.b64` — il certificato di test self-signed, già caricato come
  secret; il file locale non serve più una volta caricato.

Se possibile, copiare `android-keystore-secrets.txt` e la cartella `ios/` in un posto
persistente (non temp) prima di cambiare macchina.
