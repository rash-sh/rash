/// ANCHOR: module
/// # ldap_entry
///
/// Manage LDAP directory entries.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: full
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Examples
///
/// ```yaml
/// - name: Create user entry
///   ldap_entry:
///     dn: "cn=john,ou=users,dc=example,dc=com"
///     state: present
///     attributes:
///       objectClass:
///         - inetOrgPerson
///         - organizationalPerson
///         - person
///       cn: john
///       sn: Doe
///       uid: john
///       mail: john@example.com
///     server_uri: ldap://ldap.example.com
///     bind_dn: "cn=admin,dc=example,dc=com"
///     bind_pw: "{{ ldap_admin_pass }}"
///
/// - name: Create group entry
///   ldap_entry:
///     dn: "cn=developers,ou=groups,dc=example,dc=com"
///     state: present
///     attributes:
///       objectClass:
///         - groupOfNames
///       cn: developers
///       member:
///         - "cn=john,ou=users,dc=example,dc=com"
///         - "cn=jane,ou=users,dc=example,dc=com"
///     server_uri: ldap://ldap.example.com
///     bind_dn: "cn=admin,dc=example,dc=com"
///     bind_pw: "{{ ldap_admin_pass }}"
///
/// - name: Delete an entry
///   ldap_entry:
///     dn: "cn=olduser,ou=users,dc=example,dc=com"
///     state: absent
///     server_uri: ldap://ldap.example.com
///     bind_dn: "cn=admin,dc=example,dc=com"
///     bind_pw: "{{ ldap_admin_pass }}"
///
/// - name: Create entry with StartTLS
///   ldap_entry:
///     dn: "cn=admin,ou=users,dc=example,dc=com"
///     state: present
///     attributes:
///       objectClass:
///         - inetOrgPerson
///       cn: admin
///       sn: Admin
///     server_uri: ldap://ldap.example.com
///     bind_dn: "cn=admin,dc=example,dc=com"
///     bind_pw: "{{ ldap_admin_pass }}"
///     start_tls: true
///
/// - name: Create entry without cert validation
///   ldap_entry:
///     dn: "cn=test,ou=users,dc=example,dc=com"
///     state: present
///     attributes:
///       objectClass:
///         - inetOrgPerson
///       cn: test
///       sn: Test
///     server_uri: ldaps://ldap.example.com
///     bind_dn: "cn=admin,dc=example,dc=com"
///     bind_pw: "{{ ldap_admin_pass }}"
///     validate_certs: false
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use serde_norway::value;

use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The distinguished name of the LDAP entry.
    pub dn: String,
    /// The desired state of the entry.
    #[serde(default)]
    pub state: State,
    /// Dictionary of attributes for the entry (used with state=present).
    pub attributes: Option<BTreeMap<String, AttributeValue>>,
    /// LDAP server URI.
    #[serde(default = "default_server_uri")]
    pub server_uri: String,
    /// Bind DN for authentication.
    pub bind_dn: Option<String>,
    /// Bind password for authentication.
    pub bind_pw: Option<String>,
    /// Use StartTLS for the connection.
    #[serde(default)]
    pub start_tls: bool,
    /// Validate SSL certificates.
    #[serde(default = "default_validate_certs")]
    pub validate_certs: bool,
    /// Attributes that identify entry uniqueness.
    pub dn_attrs: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(untagged)]
pub enum AttributeValue {
    Single(String),
    Multiple(Vec<String>),
}

fn default_server_uri() -> String {
    "ldap://localhost".to_string()
}

fn default_validate_certs() -> bool {
    true
}

fn connect_ldap(params: &Params) -> Result<ldap3::LdapConn> {
    let mut settings = ldap3::LdapConnSettings::new();
    if params.start_tls {
        settings = settings.set_starttls(true);
    }
    if !params.validate_certs {
        settings = settings.set_no_tls_verify(true);
    }

    let mut conn = ldap3::LdapConn::with_settings(settings, &params.server_uri).map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to connect to LDAP server: {e}"),
        )
    })?;

    if let (Some(bind_dn), Some(bind_pw)) = (&params.bind_dn, &params.bind_pw) {
        conn.simple_bind(bind_dn, bind_pw)
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, format!("LDAP bind failed: {e}")))?
            .success()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, format!("LDAP bind failed: {e}")))?;
    }

    Ok(conn)
}

fn search_entry(
    conn: &mut ldap3::LdapConn,
    dn: &str,
    dn_attrs: &Option<Vec<String>>,
) -> Result<Option<HashMap<String, Vec<String>>>> {
    let attrs: Vec<&str> = match dn_attrs {
        Some(a) => a.iter().map(|s| s.as_str()).collect(),
        None => vec!["*"],
    };

    let (entries, _result) = conn
        .search(dn, ldap3::Scope::Base, "(objectClass=*)", attrs)
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("LDAP search failed: {e}"),
            )
        })?
        .success()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("LDAP search failed: {e}"),
            )
        })?;

    if entries.is_empty() {
        return Ok(None);
    }

    let search_entry = ldap3::SearchEntry::construct(entries[0].clone());
    Ok(Some(search_entry.attrs))
}

fn attrs_to_hashset(attrs: &BTreeMap<String, AttributeValue>) -> Vec<(String, HashSet<String>)> {
    let mut pairs = Vec::new();
    for (key, value) in attrs {
        let values: HashSet<String> = match value {
            AttributeValue::Single(v) => {
                let mut set = HashSet::new();
                set.insert(v.clone());
                set
            }
            AttributeValue::Multiple(v) => v.iter().cloned().collect(),
        };
        pairs.push((key.clone(), values));
    }
    pairs
}

fn compare_attrs(
    current: &HashMap<String, Vec<String>>,
    desired: &BTreeMap<String, AttributeValue>,
) -> (bool, Vec<ldap3::Mod<String>>) {
    let mut changed = false;
    let mut mods = Vec::new();

    for (key, desired_values) in attrs_to_hashset(desired) {
        let current_values: HashSet<String> = match current.get(&key) {
            Some(vals) => vals.iter().cloned().collect(),
            None => HashSet::new(),
        };

        if current_values != desired_values {
            changed = true;
            mods.push(ldap3::Mod::Replace(key, desired_values));
        }
    }

    (changed, mods)
}

fn exec_present(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let attributes = params.attributes.as_ref();
    let mut conn = connect_ldap(params)?;

    let existing = search_entry(&mut conn, &params.dn, &params.dn_attrs)?;

    match existing {
        None => {
            if check_mode {
                return Ok(ModuleResult::new(true, None, None));
            }

            let attrs = attributes.ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "attributes are required when state=present",
                )
            })?;

            let add_attrs: Vec<(String, HashSet<String>)> = attrs_to_hashset(attrs);

            conn.add(&params.dn, add_attrs)
                .map_err(|e| {
                    Error::new(ErrorKind::SubprocessFail, format!("LDAP add failed: {e}"))
                })?
                .success()
                .map_err(|e| {
                    Error::new(ErrorKind::SubprocessFail, format!("LDAP add failed: {e}"))
                })?;

            Ok(ModuleResult::new(
                true,
                Some(value::to_value(json!({
                    "dn": params.dn,
                    "action": "added"
                }))?),
                Some(format!("Entry {} created", params.dn)),
            ))
        }
        Some(current_attrs) => {
            let attrs = match attributes {
                Some(a) => a,
                None => {
                    return Ok(ModuleResult::new(
                        false,
                        Some(value::to_value(json!({
                            "dn": params.dn,
                            "action": "exists"
                        }))?),
                        Some(format!("Entry {} already exists", params.dn)),
                    ));
                }
            };

            let (changed, mods) = compare_attrs(&current_attrs, attrs);

            if !changed {
                return Ok(ModuleResult::new(
                    false,
                    Some(value::to_value(json!({
                        "dn": params.dn,
                        "action": "unchanged"
                    }))?),
                    Some(format!("Entry {} unchanged", params.dn)),
                ));
            }

            if check_mode {
                return Ok(ModuleResult::new(true, None, None));
            }

            conn.modify(&params.dn, mods)
                .map_err(|e| {
                    Error::new(
                        ErrorKind::SubprocessFail,
                        format!("LDAP modify failed: {e}"),
                    )
                })?
                .success()
                .map_err(|e| {
                    Error::new(
                        ErrorKind::SubprocessFail,
                        format!("LDAP modify failed: {e}"),
                    )
                })?;

            Ok(ModuleResult::new(
                true,
                Some(value::to_value(json!({
                    "dn": params.dn,
                    "action": "modified"
                }))?),
                Some(format!("Entry {} modified", params.dn)),
            ))
        }
    }
}

fn exec_absent(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let mut conn = connect_ldap(params)?;

    let existing = search_entry(&mut conn, &params.dn, &params.dn_attrs)?;

    match existing {
        None => Ok(ModuleResult::new(
            false,
            Some(value::to_value(json!({
                "dn": params.dn,
                "action": "not_found"
            }))?),
            Some(format!("Entry {} not found", params.dn)),
        )),
        Some(_) => {
            if check_mode {
                return Ok(ModuleResult::new(true, None, None));
            }

            conn.delete(&params.dn)
                .map_err(|e| {
                    Error::new(
                        ErrorKind::SubprocessFail,
                        format!("LDAP delete failed: {e}"),
                    )
                })?
                .success()
                .map_err(|e| {
                    Error::new(
                        ErrorKind::SubprocessFail,
                        format!("LDAP delete failed: {e}"),
                    )
                })?;

            Ok(ModuleResult::new(
                true,
                Some(value::to_value(json!({
                    "dn": params.dn,
                    "action": "deleted"
                }))?),
                Some(format!("Entry {} deleted", params.dn)),
            ))
        }
    }
}

pub fn ldap_entry(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    match params.state {
        State::Present => exec_present(&params, check_mode),
        State::Absent => exec_absent(&params, check_mode),
    }
}

#[derive(Debug)]
pub struct LdapEntry;

impl Module for LdapEntry {
    fn get_name(&self) -> &str {
        "ldap_entry"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            ldap_entry(parse_params(optional_params)?, check_mode)?,
            None,
        ))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_present() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            dn: "cn=john,ou=users,dc=example,dc=com"
            state: present
            attributes:
              objectClass:
                - inetOrgPerson
                - organizationalPerson
                - person
              cn: john
              sn: Doe
              uid: john
              mail: john@example.com
            server_uri: ldap://ldap.example.com
            bind_dn: "cn=admin,dc=example,dc=com"
            bind_pw: secret
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.dn, "cn=john,ou=users,dc=example,dc=com");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.server_uri, "ldap://ldap.example.com");
        assert_eq!(
            params.bind_dn,
            Some("cn=admin,dc=example,dc=com".to_string())
        );
        assert_eq!(params.bind_pw, Some("secret".to_string()));

        let attrs = params.attributes.unwrap();
        assert_eq!(
            attrs.get("cn").unwrap(),
            &AttributeValue::Single("john".to_string())
        );
        assert_eq!(
            attrs.get("sn").unwrap(),
            &AttributeValue::Single("Doe".to_string())
        );
        assert!(matches!(
            attrs.get("objectClass").unwrap(),
            AttributeValue::Multiple(v) if v.len() == 3
        ));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            dn: "cn=olduser,ou=users,dc=example,dc=com"
            state: absent
            server_uri: ldap://ldap.example.com
            bind_dn: "cn=admin,dc=example,dc=com"
            bind_pw: secret
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.dn, "cn=olduser,ou=users,dc=example,dc=com");
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_defaults() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            dn: "cn=test,dc=example,dc=com"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Present);
        assert_eq!(params.server_uri, "ldap://localhost");
        assert!(params.validate_certs);
        assert!(!params.start_tls);
        assert_eq!(params.bind_dn, None);
        assert_eq!(params.bind_pw, None);
        assert_eq!(params.attributes, None);
        assert_eq!(params.dn_attrs, None);
    }

    #[test]
    fn test_parse_params_start_tls() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            dn: "cn=test,dc=example,dc=com"
            state: present
            server_uri: ldap://ldap.example.com
            bind_dn: "cn=admin,dc=example,dc=com"
            bind_pw: secret
            start_tls: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.start_tls);
    }

    #[test]
    fn test_parse_params_no_validate_certs() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            dn: "cn=test,dc=example,dc=com"
            state: present
            server_uri: ldaps://ldap.example.com
            bind_dn: "cn=admin,dc=example,dc=com"
            bind_pw: secret
            validate_certs: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(!params.validate_certs);
    }

    #[test]
    fn test_parse_params_with_dn_attrs() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            dn: "cn=john,ou=users,dc=example,dc=com"
            state: present
            attributes:
              cn: john
              sn: Doe
            dn_attrs:
              - cn
              - uid
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.dn_attrs,
            Some(vec!["cn".to_string(), "uid".to_string()])
        );
    }

    #[test]
    fn test_parse_params_group_with_members() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            dn: "cn=developers,ou=groups,dc=example,dc=com"
            state: present
            attributes:
              objectClass:
                - groupOfNames
              cn: developers
              member:
                - "cn=john,ou=users,dc=example,dc=com"
                - "cn=jane,ou=users,dc=example,dc=com"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let attrs = params.attributes.unwrap();
        match attrs.get("member").unwrap() {
            AttributeValue::Multiple(v) => assert_eq!(v.len(), 2),
            _ => panic!("Expected Multiple values for member attribute"),
        }
    }

    #[test]
    fn test_compare_attrs_no_change() {
        let mut current: HashMap<String, Vec<String>> = HashMap::new();
        current.insert("cn".to_string(), vec!["john".to_string()]);
        current.insert("sn".to_string(), vec!["Doe".to_string()]);

        let mut desired: BTreeMap<String, AttributeValue> = BTreeMap::new();
        desired.insert("cn".to_string(), AttributeValue::Single("john".to_string()));
        desired.insert("sn".to_string(), AttributeValue::Single("Doe".to_string()));

        let (changed, mods) = compare_attrs(&current, &desired);
        assert!(!changed);
        assert!(mods.is_empty());
    }

    #[test]
    fn test_compare_attrs_with_change() {
        let mut current: HashMap<String, Vec<String>> = HashMap::new();
        current.insert("cn".to_string(), vec!["john".to_string()]);
        current.insert("sn".to_string(), vec!["OldName".to_string()]);

        let mut desired: BTreeMap<String, AttributeValue> = BTreeMap::new();
        desired.insert("cn".to_string(), AttributeValue::Single("john".to_string()));
        desired.insert(
            "sn".to_string(),
            AttributeValue::Single("NewName".to_string()),
        );

        let (changed, mods) = compare_attrs(&current, &desired);
        assert!(changed);
        assert_eq!(mods.len(), 1);
    }

    #[test]
    fn test_compare_attrs_new_attribute() {
        let mut current: HashMap<String, Vec<String>> = HashMap::new();
        current.insert("cn".to_string(), vec!["john".to_string()]);

        let mut desired: BTreeMap<String, AttributeValue> = BTreeMap::new();
        desired.insert("cn".to_string(), AttributeValue::Single("john".to_string()));
        desired.insert("sn".to_string(), AttributeValue::Single("Doe".to_string()));

        let (changed, mods) = compare_attrs(&current, &desired);
        assert!(changed);
        assert_eq!(mods.len(), 1);
    }

    #[test]
    fn test_compare_attrs_multi_value_change() {
        let mut current: HashMap<String, Vec<String>> = HashMap::new();
        current.insert("objectClass".to_string(), vec!["inetOrgPerson".to_string()]);

        let mut desired: BTreeMap<String, AttributeValue> = BTreeMap::new();
        desired.insert(
            "objectClass".to_string(),
            AttributeValue::Multiple(vec!["inetOrgPerson".to_string(), "top".to_string()]),
        );

        let (changed, mods) = compare_attrs(&current, &desired);
        assert!(changed);
        assert_eq!(mods.len(), 1);
    }
}
