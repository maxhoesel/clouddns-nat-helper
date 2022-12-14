//! Interface with DNS providers and get/set zone records.
//!
//! Providers are DNS server providers such as Cloudflare that can be accessed through an API.
//! This application retrieves records from them, generates a [`crate::plan::Plan`] of required actions,
//! then applies that plan back to the providers.
//!
//! All providers implement the [`Provider`] trait. Currently, the following providers are available:
//! - [`CloudflareProvider`]: Interfaces with the Cloudflare dns and zone API
mod cloudflare;

#[cfg(test)]
use mockall::automock;

// Re-exports for convenience
pub use self::cloudflare::{CloudflareProvider, CloudflareProviderConfig};

// Actual provider uses
use std::{
    fmt::Display,
    net::{Ipv4Addr, Ipv6Addr},
};

use crate::{config::TTL, plan::Plan};

/// A provider is any DNS service provider, such as Cloudflare, PowerDNS, etc...
///
/// They implement a few basic methods to access and modify DNS records.
#[cfg_attr(test, automock)]
pub trait Provider {
    /// Returns whether this provider supports running in dry-run mode, with no changes being made
    fn supports_dry_run(&self) -> bool;
    fn set_dry_run(&mut self, dry_run: bool);

    // Returns the default ttl that will be applied to all new records
    fn ttl(&self) -> Option<TTL>;
    fn set_ttl(&mut self, ttl: TTL);

    /// Get all relevant records currently registered with the provider.
    /// Note that we only care about A and AAAA records, as well as TXT records (for the [`crate::registry::TxtRegistry`]).
    /// Returns a result of [`DnsRecord`]s
    fn records(&self) -> Result<Vec<DnsRecord>, ProviderError>;

    /// Apply a full [`Plan`] of DNS record changes to this provider.
    /// As plans are generated with the help of a registry,
    /// the actions in the plan are guaranteed to only operate on owned records.
    fn apply_plan(&self, plan: Plan) -> Vec<Result<(), ProviderError>>;

    /// Create a single TXT record.
    /// This method is intended for use by registries that need to store additional information in the DNS zone,
    /// such as [`crate::registry::TxtRegistry`].
    /// For regular A record operations, use [`Provider::apply_plan()`] instead.
    fn create_txt_record(&self, domain: String, content: String) -> Result<(), ProviderError>;
    /// Delete a single TXT record.
    /// This method is intended for use by registries that need to store additional information in the DNS zone,
    /// such as the [`crate::registry::TxtRegistry`].
    /// For regular A record operations, use [`Provider::apply_plan()`] instead.
    fn delete_txt_record(&self, domain: String, content: String) -> Result<(), ProviderError>;
}

/// Generic error returned by a provider action.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProviderError {
    msg: String,
}
impl Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg.as_str())
    }
}
impl std::error::Error for ProviderError {}

impl From<String> for ProviderError {
    fn from(s: String) -> Self {
        ProviderError { msg: s }
    }
}

/// Represents a single DNS record that can be used to interface with a [`Provider`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DnsRecord {
    /// The fully-qualified domain name of the record (e.g. `my.example.com`)
    pub domain_name: String,
    /// A variant of [`RecordContent`], representing the data stored in the record
    pub content: RecordContent,
}
impl Display for DnsRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.domain_name, self.content)
    }
}

/// Represents the content of a single DNS record.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RecordContent {
    A(Ipv4Addr),
    Aaaa(Ipv6Addr),
    Txt(String),
}
impl Display for RecordContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                RecordContent::A(a) => format!("A {}", a),
                RecordContent::Aaaa(aaaa) => format!("AAAA {}", aaaa),
                RecordContent::Txt(txt) => format!("TXT {}", txt),
            }
        )
    }
}
