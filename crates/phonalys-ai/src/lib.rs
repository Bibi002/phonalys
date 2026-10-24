//! Passerelle IA de Phonalys.
//!
//! Principe non négociable : **seuls des modèles hébergés par le praticien** sont
//! joignables (Ollama, llama.cpp, LM Studio, vLLM… sur sa machine ou son réseau privé).
//! Les IA grand public dans le cloud sont bloquées par construction : par l'adresse IP
//! (seules les plages privées sont autorisées) et par une liste noire de noms d'hôte.

pub mod client;
pub mod policy;
pub mod provider;
pub mod pseudonymize;

pub use client::{AiError, Engine};
pub use policy::{EndpointPolicy, PolicyError, ValidatedEndpoint};
pub use provider::{ChatMessage, ChatRequest, ProviderKind, Role};
pub use pseudonymize::Pseudonymizer;
