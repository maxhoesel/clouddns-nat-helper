mod util;

use std::collections::HashMap;

use itertools::Itertools;
use log::{debug, info, warn};

use self::util::{rec_into_d, txt_record_string, TXT_RECORD_IDENT};
use super::{ARegistry, Domain, DomainName, RegistryError};
use crate::provider::{DnsRecord, Provider, RecordContent};

#[derive(Debug, Clone, PartialEq)]
enum Ownership {
    /// This domains A record belongs to us
    Owned,
    /// This domains A records are managed by someone else
    Taken,
    /// This domain doesn't have A records, we can claim it
    Available,
}

#[derive(Debug, Clone)]
struct RegisteredDomain {
    domain: Domain,
    ownership: Ownership,
}

#[non_exhaustive]
pub struct TxtRegistry<'a> {
    domains: HashMap<DomainName, RegisteredDomain>,
    tenant: String,
    provider: &'a dyn Provider,
}

impl TxtRegistry<'_> {
    /// Determine the current ownership status for a given domain
    fn determine_ownership(domain: &Domain, tenant: &str) -> Ownership {
        let owner_records: Vec<&String> = domain
            .txt
            .iter()
            .filter(|txt| txt.as_str().starts_with(TXT_RECORD_IDENT))
            .unique()
            .collect();

        match owner_records.len() {
            0 => {
                if domain.a.is_empty() {
                    // No A records and no ownership - we can manage this one
                    Ownership::Available
                } else {
                    // A records already present, seems like this domain is externally managed
                    Ownership::Taken
                }
            }
            1 => {
                if owner_records.contains(&&txt_record_string(tenant)) {
                    // We are the proud owner of this domain
                    Ownership::Owned
                } else {
                    // Some other instance of the nat-helper manages this
                    Ownership::Taken
                }
            }
            2.. => {
                warn!("Conflicting ownership of domain {} - extra ownership records were found:{:?}.\n Considering this domain taken", domain.name, owner_records);
                Ownership::Taken
            }
            _ => unreachable!(),
        }
    }

    pub fn create(
        records: Vec<DnsRecord>,
        tenant: String,
        provider: &dyn Provider,
    ) -> Box<dyn ARegistry + '_> {
        let tenant = tenant.replace(TXT_RECORD_IDENT, "");
        let mut domains: HashMap<String, RegisteredDomain> = HashMap::new();

        // Create a map of all domains that we will watch over
        for rec in &records {
            if let Some(reg_d) = domains.get_mut(&rec.domain) {
                // Update an existing domain
                rec_into_d(rec, &mut reg_d.domain)
            } else {
                // Create a new domain and insert the record
                let mut reg_d = RegisteredDomain {
                    domain: Domain {
                        name: rec.domain.to_owned(),
                        a: Vec::new(),
                        aaaa: Vec::new(),
                        txt: Vec::new(),
                    },
                    ownership: Ownership::Taken, // Safe default, overwritten below
                };
                rec_into_d(rec, &mut reg_d.domain);
                domains.insert(rec.domain.to_owned(), reg_d);
            }
        }

        for domain in domains.values_mut() {
            domain.ownership = TxtRegistry::determine_ownership(&domain.domain, &tenant);
        }

        Box::new(TxtRegistry {
            domains,
            tenant,
            provider,
        })
    }
}

impl ARegistry for TxtRegistry<'_> {
    fn owned_domains(&self) -> Vec<super::Domain> {
        self.domains
            .values()
            .filter(|d| d.ownership == Ownership::Owned)
            .map(|d| d.domain.clone())
            .collect_vec()
    }

    fn claim(&mut self, name: &DomainName) -> Result<(), super::RegistryError> {
        if !self.domains.contains_key(name) {
            return Err(RegistryError {
                msg: format!("Domain {} not in registry", name),
            });
        }

        let reg_d = self.domains.get_mut(name).unwrap();
        match reg_d.ownership {
            Ownership::Owned => {
                info!(
                    "Attempted to claim domain {}, but it is already owned by us. Ignoring",
                    name
                );
                Ok(())
            }
            Ownership::Taken => Err(RegistryError {
                msg: format!(
                    "Domain {} already has A records and no ownership record",
                    name
                ),
            }),
            Ownership::Available => {
                self.provider
                    .create_txt_record(
                        reg_d.domain.name.to_owned(),
                        txt_record_string(&self.tenant),
                    )
                    .map_err(|e| RegistryError {
                        msg: format!("Unable to claim domain {}: {}", name, e),
                    })?;
                reg_d.ownership = Ownership::Owned;
                debug!("Successfully claimed domain {}", name);
                Ok(())
            }
        }
    }

    fn release(&mut self, name: &DomainName) -> Result<(), RegistryError> {
        if !self.domains.contains_key(name) {
            return Err(RegistryError {
                msg: format!("Domain {} not in registry", name),
            });
        }

        let reg_d = self.domains.get_mut(name).unwrap();
        match reg_d.ownership {
            Ownership::Owned => {
                self.provider
                    .delete_txt_record(
                        reg_d.domain.name.to_owned(),
                        txt_record_string(&self.tenant),
                    )
                    .map_err(|e| RegistryError {
                        msg: format!("unable to release domain {}: {}", name, e),
                    })?;
                reg_d.ownership = Ownership::Owned;
                debug!("Sucessfully released domain {}", name);
                Ok(())
            }
            Ownership::Taken => Err(RegistryError {
                msg: format!(
                    "Cannot release domain {} as it is owned by someone else",
                    name
                ),
            }),
            Ownership::Available => {
                info!("Attempted to release domain {}, but it is already not owned by anyone. Ignoring", name);
                Ok(())
            }
        }
    }

    fn set_tenant(&mut self, tenant: String) {
        self.tenant = tenant;
    }
}