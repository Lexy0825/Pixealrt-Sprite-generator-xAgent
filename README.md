# Pixel Studio

Editor pixel-art / tileset multi-frame in Rust, con **server MCP integrato**:
un'AI (Claude Desktop, Cursor, qualunque client MCP) può guidare la creazione
dei disegni mentre l'editor le mostra in tempo reale.

## Online Docs now up:
🌐 https://lexy0825.github.io/Pixealrt-Sprite-generator-xAgent/ 🌐

## Architettura

Cargo workspace con tre crate:

```
crates/
  pixel-core/        lib — logica dati pura, senza GUI né I/O di rete
    color.rs         RGBA 8 bit: alpha 0 = trasparente (nessuno sfondo implicito)
    grid.rs          griglia WxH bounds-checked, set_pixel / fill_rect
    brush.rs         impronta penna dura/morbida e gomma (falloff sui vicini)
    frame.rs         frame = griglia + duration_ms
    project.rs       progetto multi-frame: frame attivo, fps, name, CRUD
    project_json.rs  persistenza JSON versione 1, salvataggio atomico (tmp + rename)
    export.rs        PNG del frame attivo + ZIP (frame_0000.png… + metadata.json)

  pixel-ui/          bin — app desktop (eframe/egui, renderer glow interno)
    app.rs           stato: autosave con debounce (400 ms) + polling mtime del file
    ui/menu_bar.rs   menu: File, Aspetto, Animazione, AI Agent
    ui/canvas.rs     canvas con zoom/pan, scacchiera di trasparenza, reticolo
    ui/preview.rs    anteprima animazione live + striscia dei frame
    ui/colors.rs     pannello Colori: ruota tinta + saturazione, opacità,
                     palette del progetto, recenti, pennello/gomma

  pixel-mcp/         bin — server MCP su stdio (rmcp + tokio)
    tools.rs         i tool esposti all'AI (testabili in unit test senza stdio)
    main.rs          avvio: service.serve(stdio()) + waiting()
```

### Come editor e AI condividono lo stato

Un solo file `*.pixproj` (JSON versione 1) è la *single source of truth*:

- **pixel-ui** salva con debounce dopo ogni modifica e ricarica automaticamente
  quando il file cambia (polling dell'mtime, costo minimo): un avviso discreto
  sul canvas segnala "Progetto aggiornato dall'agente AI".
- **pixel-mcp** legge e riscrive lo stesso file ad ogni tool call.
- Non esiste una logica di conflitto perché non esistono due fonti di verità.

Il canvas disegna la griglia con **alpha reale**: i pixel trasparenti restano
trasparenti (né bianchi né neri), resi leggibili da una scacchiera scura
e neutra che può essere spenta dal menu "Aspetto".

### Pennelli e colori

Il pannello destro "Colori" offre un selettore a ruota (tinta sull'anello,
saturazione nel disco), slider di luminosità e opacità, la palette dei
colori già presenti nel progetto e gli ultimi colori usati. Con il pennello
**morbido** un tocco a opacità piena sfuma i vicini: i 4 lati prendono
alpha 0.7, le diagonali 0.4 (falloff documentato in `pixel-core::brush`),
senza mai cancellare il dettaglio più forte già sotto; la **gomma** cancella
il centro e attenua i bordi (tasto destro sul canvas = gomma rapida).

## Comandi

```bash
# Editor desktop
cargo run -p pixel-ui

# Server MCP su stdio (avviato dal client AI, non a mano)
cargo build --release # crea target/release/pixel-mcp

# Test e verifiche
cargo test --workspace
cargo check --workspace
```

Compatibile Linux, macOS e Windows: tutte le dipendenze sono crate puro-Rust
o Windows-coscienti già pronti (`eframe`, `rmcp`, `image` solo-png, `zip`,
`serde`, `thiserror`, `rfd`). Non c'è codice specifico per piattaforma.

## Installazione su Windows

Doppio clic su `install.cmd` (o `powershell -ExecutionPolicy Bypass -File
install.ps1` da terminale). L'installer:

1. scarica Rust se manca (`rustup-init.exe` con SHA256 verificato, toolchain
   `stable`, profilo minimale) e ricarica il PATH nella sessione;
2. controlla i Build Tools MSVC e, se assenti, stampa il comando `winget` utile;
3. compila con `cargo build --release -p pixel-ui -p pixel-mcp`;
4. installa `pixel-ui.exe` e `pixel-mcp.exe` in
   `%LOCALAPPDATA%\Programs\Pixel Studio`, con scorciatoia nel menu Start, sul
   desktop e la voce in "App e funzionalità".

È un'installazione per-utente: nessun privilegio di amministratore. Opzioni:
`-InstallDir "D:\Apps\Pixel Studio"`, `-NoDesktopShortcut`, `-SkipRust`. Log
completo in `%TEMP%\pixel-studio-install-*.log`.

La rimozione passa da "App e funzionalità" oppure dall'`uninstall.ps1` generato
accanto ai binari, che chiede se cancellare anche i recenti in
`%APPDATA%\PixelStudio`.

## Guida utente su GitHub Pages

La guida (`on_doc/`) resta fuori dal repository (vedi `.gitignore`): si pubblica
con `deploy-docs.cmd`, che crea un commit orfano con il solo contenuto della
guida (`index.html`, `style.css`, `img/`, `.nojekyll`) e lo invia al branch
`gh-pages` senza toccare `main`. Su GitHub, una volta sola:
**Settings → Pages → Deploy from a branch → `gh-pages` / `(root)`**.

**[Guida utente online →](https://lexy0825.github.io/Pixealrt-Sprite-generator-xAgent/)**

## Configurazione del client MCP

Esempio per Claude Desktop / Cursor / Windows ecc. (`claude_desktop_config.json`
o equivalente):

```json
{
  "mcpServers": {
    "pixel-studio": {
      "command": "target/release/pixel-mcp",
      "args": []
    }
  }
}
```

Lo stesso path assoluto va bene su Linux/macOS/Windows: il binario
`pixel-mcp` viene avviato dal client come processo figlio e parla
JSON-RPC 2.0 / MCP su stdin/stdout.

Flusso tipico con l'editor aperto:

1. Chiedi all'AI di creare o modificare il progetto (tool MCP sotto).
2. L'AI salva su disco; l'editor ricarica da solo e mostra l'avviso.
3. Dall'editor premi `Salva come…` la prima volta per fissare il percorso
   del file condiviso, poi tutto resta sincronizzato per accordo.

## Tool MCP esposti

| tool               | cosa fa                                                              |
|--------------------|----------------------------------------------------------------------|
| `create_project`   | nuovo progetto (nome, dimensioni, fps) su un file `.pixproj`          |
| `get_project_info` | statistiche: dimensioni, frame attivo, fps, percorso file             |
| `add_frame`        | aggiunge/duplica un frame della sequenza                              |
| `set_active_frame` | sceglie il frame su cui disegnare (0-based)                           |
| `set_fps`          | imposta gli fps dell'anteprima (10..60)                               |
| `set_pixel`        | colore RGBA di un pixel (alpha 0 = trasparente)                       |
| `fill_rect`        | riempie un rettangolo incluso con un colore                           |
| `erase_rect`       | azzera l'alpha di un rettangolo (trasparenza totale)                  |
| `get_canvas`       | legge un frame come PNG in base64 (per "vedere" il disegno)            |
| `export_png`       | esporta un frame in PNG senza sfondo                                  |
| `export_zip`       | esporta tutti i frame in ZIP con `metadata.json`                      |

I nomi e le descrizioni precise arrivano al client via MCP: sono necessarie
per la selezione automatica dei tool da parte dell'AI.

## Licenza

Progetto di esempio: MIT/Apache-2.0 come standard delle crate Rust.
