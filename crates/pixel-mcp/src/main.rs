//! Server MCP su stdio: un solo servizio, tutti i tool di `tools.rs`.
//!
//! L'agente AI (Claude Desktop, Cursor, CLI MCP client, ...) avvia questo
//! binario come processo figlio e parla JSON-RPC 2.0 su stdin/stdout.

use anyhow::Result;
use pixel_mcp::PixelService;
use rmcp::{ServiceExt, transport::stdio};

#[tokio::main]
async fn main() -> Result<()> {
    let service = PixelService.serve(stdio()).await?;
    // L'EOF su stdin chiude il ciclo di servizio.
    service.waiting().await?;
    Ok(())
}
