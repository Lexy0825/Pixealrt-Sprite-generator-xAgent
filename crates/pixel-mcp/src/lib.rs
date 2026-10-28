//! `pixel-mcp` — server MCP (Model Context Protocol) su stdio.
//!
//! Espone all'agente AI gli strumenti per creare progetti, disegnare sulla
//! griglia, leggere lo stato ed esportare PNG/ZIP. La libreria contiene i
//! tool **senza** il transport, così sono richiamabili direttamente nei test;
//! il binario `main.rs` li serve via stdio con `rmcp`.

pub mod tools;

pub use tools::PixelService;
