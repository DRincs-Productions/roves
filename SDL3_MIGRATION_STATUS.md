# SDL3 windowing migration status

Questo file è il registro operativo del branch `sdl3-windowing`. Va aggiornato nello stesso
commit di ogni avanzamento significativo, così stato del codice, test e CI restano allineati.

## Vincolo mobile

La migrazione SDL3 è limitata al desktop. I backend Android e OpenHarmony (`egl/android`,
`egl/ohos` e dipendenze native collegate) devono restare supportati e non vanno sostituiti con
il ciclo finestra desktop. Ogni modifica condivisa deve essere verificata anche rispetto ai target
mobile.

## Obiettivo di completamento

Il port è concluso quando `ports/servoshell` usa SDL3 per finestra, event loop e input senza
dipendenze runtime dirette da `winit`/`egui-winit`, la matrice Linux/macOS/Windows è verde e le
funzioni desktop elencate sotto hanno copertura automatica o una verifica hardware documentata.

## Fatto

- [x] Finestra e ciclo eventi desktop basati su SDL3.
- [x] Creazione contesto GL tramite raw-window-handle SDL3.
- [x] Traduzione base tastiera, mouse, wheel, resize, focus, redraw e close.
- [x] Gamepad SDL3 su Linux, Windows e macOS.
- [x] Rendering egui tramite bridge SDL3 locale (`SdlEguiGlow`).
- [x] Smoke test Steam e build/package su Linux, macOS e Windows.
- [x] Arresto bounded dello smoke test macOS e cancellazione CI delle run obsolete.
- [x] Gate rapido `support/check_sdl3_windowing_contracts.py` prima delle build costose.
- [x] Baseline verde completa: GitHub Actions run `35401939171`.
- [x] Eliminati i residui compilati di `winit` e `egui-winit` da `ports/servoshell`.
- [x] Portati i controlli tastiera WebXR ai tipi evento Servo/SDL3.
- [x] Rimossi helper e moduli winit irraggiungibili.

## In corso

- [ ] Verificare in CI la compilazione senza le feature winit transitive di egui.

## Da fare

- [ ] Completare input egui: testo, clipboard, modifier, pointer e consumo eventi.
- [ ] Implementare testo SDL3 e IME: composizione, candidati e rettangolo editor.
- [x] Portare touch SDL3 con ID stabili e coordinate normalizzate → pixel.
- [ ] Portare gesture multi-touch/pinch.
- [x] Portare il file drop SDL3 e la navigazione al file URL.
- [ ] Completare DPI/display-change, tema, cursori, icona e finestra trasparente.
- [ ] Implementare un adapter AccessKit indipendente da winit.
- [ ] Aggiungere unit test Rust per mapping tasti/eventi e test Xvfb per il ciclo finestra.
- [ ] Eseguire la checklist hardware di `SDL3_WINDOWING_TESTING.md` sui tre sistemi.
- [ ] Rendere verde la matrice finale e annotare qui run e commit conclusivi.

## Regole di verifica per ogni checkpoint

1. Aggiornare codice, patch Servo corrispondenti, `CUSTOMIZATIONS.md` e questo file.
2. Eseguire il gate dei contratti SDL3 e i controlli statici disponibili localmente.
3. Pubblicare sul solo branch `sdl3-windowing`.
4. Lasciare che il gate rapido preceda la matrice costosa; correggere ogni fallimento prima del
   checkpoint successivo.
5. Non dichiarare completata una voce hardware soltanto perché compila in CI.
