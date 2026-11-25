# Prestazioni e fluidità di Roves CEF

Il riferimento è la build Windows Release di CEF + SDL3 + wgpu, con Chrome
sullo stesso hardware e sugli stessi file. La priorità è la fluidità percepita.

## Lavoro aperto

1. **Primo accesso alle impostazioni.** Misurare dal clic al primo frame visibile,
   poi fino alla fine della transizione. Confrontare prima apertura e riaperture.
   Distinguere montaggio React, lettura dei salvataggi, layout e paint. Anticipare
   lavoro solo se il costo si ripete e il menu resta reattivo.
2. **Primo Start.** Misurare dal clic al primo frame della storia. Separare rete
   loopback, cache, moduli, decode, upload GPU e attese. Usare gli hint web
   standard se una risorsa critica è dichiarata. Valutare la preparazione della
   prima scena a cura del gioco se emerge un costo specifico. Misurare anche
   con una fixture JavaScript indipendente da Pixi. Evitare il precaricamento
   indiscriminato.
3. **CPU a riposo e durante il gioco.** Attribuire il carico all'host SDL3, al
   renderer CEF o al processo GPU. Contare risvegli, nuove texture e presentazioni
   nel menu fermo e durante la storia. Ridurre risvegli o presentazioni ridondanti
   se la traccia ne mostra il costo. Verificare input, timer, audio, resize e ripresa.
4. **Scatti intermittenti di presentazione.** Se ricompaiono, correlare gli stalli
   dell'host con acquisizione della superficie, submit, present e nuove texture
   CEF. Usare PresentMon e una traccia ETW per distinguere coda GPU, swap chain e
   DWM. Valutare le attese nel percorso wgpu-weld solo se risultano dominanti.
5. **Baseline pubblicabile.** Raccogliere risultati ripetibili sul gioco reale e
   riportarli nella wiki con commit, driver e condizioni di prova.

## Protocollo

- Tre coppie alternate di prove Roves e Chrome. Stesso dist, viewport, DPR,
  refresh e risoluzione del canvas. Registrare profilo nuovo e riaperture a caldo
  separatamente, tenendo una sola finestra sotto misura.
- Misurare avvio, menu fermo, impostazioni, Start e scrittura di una battuta.
  Raccogliere P95/P99 dei frame mostrati, sequenze di frame mancati, latenza
  input-display e CPU/GPU per processo. A 60 Hz il riferimento è 16,67 ms.
- I timestamp di present del player misurano il lavoro dell'host. Per i frame
  mostrati usare PresentMon. Annotare eventuali limiti ETW e della traccia.
- Consultare il riferimento Chromium fissato prima di ogni intervento. Cambiare
  una variabile alla volta e ripetere la prova a parità di condizioni.
  Accettare miglioramenti ripetibili che preservino input, audio, resa e salvataggi.

Le istruzioni per avviare la build CEF e raccogliere le misure sono nel
[README del player](../support/cef-netrender-probe/README.md#presentmon-comparison-with-chrome).
