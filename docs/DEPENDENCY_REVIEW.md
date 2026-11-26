# Dipendenze: decisioni attuali e lavoro aperto

Revisione del 26 novembre 2026, dopo il passaggio al player CEF. Lo storico è in
[CUSTOMIZATIONS.md](../CUSTOMIZATIONS.md); il backlog operativo è in [TODO.md](../TODO.md).

## Player desktop (Windows, macOS, Linux)

| Dipendenza | Versione | Ruolo e motivo |
| --- | --- | --- |
| `cef` (cef-rs) | `=154.5.0` (CEF 154, Chromium 154) | Esegue il gioco: Chromium e il suo V8. Versione imposta da welding. |
| `welding` / `grafting` (wgpu-weld) | fork in `support/vendor/wgpu-weld` | Callback OSR di CEF, import della texture condivisa su wgpu, bootstrap della sandbox Windows; evita di scrivere a mano l'import D3D11/D3D12, IOSurface e dma-buf. Modifiche e criteri di rimozione in `ROVES_PATCHES.md`. |
| `wgpu` | `=29.0.4` | Presentazione: DX12 (Windows), Metal (macOS), Vulkan (Linux). Riga `wgpu-29` di welding. |
| `sdl3` | `0.20` | Finestra, eventi, input, icona. |
| `tokio` | `1.47` | Server loopback del gioco e trasporto del bridge MCP. |
| `steamworks` | `=0.13.1` (feature `steam`) | Variante Steam; mai fatale senza Steam. |
| `serde_json`, `url`, `getrandom`, `log` | — | Host API, regole dei link, token MCP, inoltro dei log di welding. |
| Server MCP (Node) | `@modelcontextprotocol/server` 2.4.0, `zod` 4.4.3 | Solo nei pacchetti MCP di sviluppo. |

## Decisioni

- Il V8 e le API Web sono quelli di Chromium: non si aggiungono runtime JS o polyfill nativi.
- Preferire le API di CEF, wgpu-weld, wgpu e SDL3 al codice scritto a mano; un fork resta
  temporaneo e documentato in `ROVES_PATCHES.md` con issue/PR upstream.
- `image-size` scartato (26 novembre 2026) per il server MCP: richiede Node 22, il server
  supporta Node 20, e il bridge riporta già regione e scala degli screenshot.

## Lavoro aperto

- welding richiede CEF 154: aggiornare CEF solo insieme a una release di welding compatibile.
- welding attiva la feature `accelerated_osr` di `cef`, che porta un wgpu 30 inutilizzato nel
  grafo (compilato, mai istanziato): da proporre upstream.
- Rimuovere il fork di wgpu-weld quando le modifiche elencate in `ROVES_PATCHES.md` sono in
  una release ufficiale.
