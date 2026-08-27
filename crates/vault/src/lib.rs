mod config;
mod documents;
mod error;
mod ignore;
mod index;
mod model;
mod rank;
mod recall;
mod text;
mod walk;

#[cfg(test)]
mod tests;

pub use error::VaultError;
pub use model::{VaultCandidate, VaultRecallRequest, VaultRecallResult, VaultTaxonomy};
pub use recall::recall;
