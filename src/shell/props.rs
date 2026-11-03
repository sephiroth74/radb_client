//! System properties: `getprop` / `setprop`.

use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::sync::LazyLock;

use regex::Regex;

use crate::error::{Error, Result};
use crate::shell::Shell;

static RE_PROP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[(.*)\]\s*:\s*\[([^\]]*)\]$").unwrap());

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Property {
    pub key: String,
    pub value: String,
}

/// The type of a property, as reported by `getprop -T`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PropType {
    String,
    Bool,
    Int,
    Uint,
    Double,
    Size,
    Enum(Vec<String>),
    Unknown(String),
}

macro_rules! ro_build_property {
    ($name:ident, $key:literal, $typ:ty) => {
        #[doc = concat!("The `", $key, "` property.")]
        pub fn $name(&self) -> Result<$typ> {
            self.getprop_with_type::<$typ>($key)
        }
    };
}

impl Shell<'_> {
    /// The value of a property; empty if the property is not set.
    pub fn getprop(&self, key: &str) -> Result<String> {
        self.exec_string(["getprop", key])
    }

    /// The value of a property, parsed.
    pub fn getprop_with_type<T: std::str::FromStr>(&self, key: &str) -> Result<T> {
        let value = self.getprop(key)?;
        value
            .parse::<T>()
            .map_err(|_| Error::ParseError(format!("property {key}=`{value}`")))
    }

    /// All the properties.
    pub fn getprops(&self) -> Result<Vec<Property>> {
        Ok(parse_props(&self.exec_string(["getprop"])?))
    }

    /// The types of all the properties.
    pub fn getprops_type(&self) -> Result<HashMap<String, PropType>> {
        let output = self.exec_string(["getprop", "-T"])?;
        Ok(parse_props(&output)
            .into_iter()
            .map(|p| (p.key, PropType::from(p.value.as_str())))
            .collect())
    }

    pub fn getprop_type(&self, key: &str) -> Result<PropType> {
        Ok(PropType::from(
            self.exec_string(["getprop", "-T", key])?.as_str(),
        ))
    }

    /// Set a property. An empty value clears it.
    pub fn setprop(&self, key: &str, value: &str) -> Result<()> {
        self.exec(["setprop", key, value]).map(|_| ())
    }

    pub fn clear_prop(&self, key: &str) -> Result<()> {
        self.setprop(key, "")
    }

    ro_build_property!(build_ab_update, "ro.build.ab_update", String);
    ro_build_property!(build_characteristics, "ro.build.characteristics", String);
    ro_build_property!(build_date, "ro.build.date", String);
    ro_build_property!(build_date_utc, "ro.build.date.utc", u64);
    ro_build_property!(build_description, "ro.build.description", String);
    ro_build_property!(build_display_id, "ro.build.display.id", String);
    ro_build_property!(build_fingerprint, "ro.build.fingerprint", String);
    ro_build_property!(build_flavor, "ro.build.flavor", String);
    ro_build_property!(build_host, "ro.build.host", String);
    ro_build_property!(build_id, "ro.build.id", String);
    ro_build_property!(build_product, "ro.build.product", String);
    ro_build_property!(build_tags, "ro.build.tags", String);
    ro_build_property!(build_type, "ro.build.type", String);
    ro_build_property!(build_user, "ro.build.user", String);
    ro_build_property!(build_version_codename, "ro.build.version.codename", String);
    ro_build_property!(
        build_version_incremental,
        "ro.build.version.incremental",
        String
    );
    // a String: releases like "8.1.0" or "16" are not always integers
    ro_build_property!(build_version_release, "ro.build.version.release", String);
    ro_build_property!(build_version_sdk, "ro.build.version.sdk", u16);
}

/// Parse lines like `[key]: [value]` (output of `getprop` and `getprop -T`).
fn parse_props(output: &str) -> Vec<Property> {
    output
        .lines()
        .filter_map(|line| RE_PROP.captures(line.trim_end()))
        .map(|c| Property {
            key: c[1].to_string(),
            value: c[2].to_string(),
        })
        .collect()
}

impl From<&str> for PropType {
    fn from(value: &str) -> Self {
        match value.trim() {
            "string" => PropType::String,
            "bool" => PropType::Bool,
            "int" => PropType::Int,
            "uint" => PropType::Uint,
            "double" => PropType::Double,
            "size" => PropType::Size,
            other => match other.strip_prefix("enum ") {
                Some(values) => {
                    PropType::Enum(values.split_whitespace().map(str::to_string).collect())
                }
                None => PropType::Unknown(other.to_string()),
            },
        }
    }
}

impl Display for PropType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PropType::String => write!(f, "string"),
            PropType::Bool => write!(f, "bool"),
            PropType::Int => write!(f, "int"),
            PropType::Uint => write!(f, "uint"),
            PropType::Double => write!(f, "double"),
            PropType::Size => write!(f, "size"),
            PropType::Enum(values) => write!(f, "enum {}", values.join(" ")),
            PropType::Unknown(value) => write!(f, "{value}"),
        }
    }
}

impl Display for Property {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} = {}", self.key, self.value)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_props() {
        let props = parse_props(include_str!("../../test_files/fixtures/api35/getprop.txt"));
        assert!(props.len() > 500);
        let get = |key: &str| {
            props
                .iter()
                .find(|p| p.key == key)
                .map(|p| p.value.as_str())
        };
        assert_eq!(get("ro.build.version.sdk"), Some("35"));
        assert_eq!(get("ro.build.type"), Some("userdebug"));
        // empty values are kept
        assert!(props.iter().any(|p| p.value.is_empty()));
    }

    #[test]
    fn test_parse_prop_types() {
        let types: HashMap<_, _> = parse_props(include_str!(
            "../../test_files/fixtures/api35/getprop_types.txt"
        ))
        .into_iter()
        .map(|p| (p.key, PropType::from(p.value.as_str())))
        .collect();
        assert_eq!(types.get("ro.build.version.sdk"), Some(&PropType::Int));
        assert_eq!(types.get("ro.build.type"), Some(&PropType::String));
        assert!(types.values().any(|t| matches!(t, PropType::Enum(_))));
    }

    #[test]
    fn test_prop_type() {
        assert_eq!(PropType::from("bool"), PropType::Bool);
        assert_eq!(
            PropType::from("enum running stopped"),
            PropType::Enum(vec!["running".to_string(), "stopped".to_string()])
        );
        assert_eq!(
            PropType::from("weird"),
            PropType::Unknown("weird".to_string())
        );
        assert_eq!(PropType::from("enum a b").to_string(), "enum a b");
    }
}
