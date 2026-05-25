pub mod alias;
pub mod compression;
pub mod config;
#[cfg(not(target_arch = "wasm32"))]
pub mod extract;
pub mod family;
#[cfg(not(target_arch = "wasm32"))]
pub mod graph;
#[cfg(not(target_arch = "wasm32"))]
pub mod index;
#[cfg(not(target_arch = "wasm32"))]
pub mod lint;
#[cfg(all(feature = "mcp", not(target_arch = "wasm32")))]
pub mod mcp;
#[cfg(not(target_arch = "wasm32"))]
pub mod meta_index;
#[cfg(not(target_arch = "wasm32"))]
pub mod ontology;
pub mod parser;
pub mod prose;
pub mod query;
#[cfg(not(target_arch = "wasm32"))]
pub mod rename;
#[cfg(not(target_arch = "wasm32"))]
pub mod search;
#[cfg(not(target_arch = "wasm32"))]
pub mod treesitter;
#[cfg(all(feature = "watch", not(target_arch = "wasm32")))]
pub mod watch;

#[cfg(target_arch = "wasm32")]
pub mod wasm;
