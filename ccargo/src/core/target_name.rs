use crate::utils::{Error, InternedString};
use serde::{ser, de};
use std::str::FromStr;


#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TargetName {
    package: InternedString,
    target: InternedString,
}

impl TargetName {
    pub fn new(package: impl Into<InternedString>, target: impl Into<InternedString>) -> Self {
        Self{package: package.into(), target: target.into()}
    }
    pub fn package(&self) -> InternedString {
        self.package
    }
    pub fn target(&self) -> InternedString {
        self.target
    }
}

impl std::str::FromStr for TargetName {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let package_len = s.find("::")
            .filter(|v| *v > 0 && *v < s.len() - 2)
            .ok_or(anyhow::anyhow!("Expected target name like `pkg::target`, got `{}`", s))?;
        Ok(Self::new(&s[..package_len], &s[package_len+2..]))
    }
}

impl std::fmt::Debug for TargetName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}::{}", self.package, self.target)
    }   
}

impl std::fmt::Display for TargetName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}::{}", self.package, self.target)
    }   
}

impl ser::Serialize for TargetName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        format!("{}::{}", self.package, self.target).serialize(serializer)
    }
}

impl<'de> de::Deserialize<'de> for TargetName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        struct TargetNameVisitor;
        impl<'de> de::Visitor<'de> for TargetNameVisitor {
            type Value = TargetName;
            
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a target name string like \"mypkg::lib\"")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
                where
                    E: de::Error, {
                TargetName::from_str(v)
                    .map_err(|_| de::Error::custom("invalid package name"))
            }
        }
        deserializer.deserialize_any(TargetNameVisitor)
    }
}

#[cfg(test)]
mod test {
    use super::*;
        
    #[test]
    fn from_str() {
        assert!("pkg::lib".parse::<TargetName>().is_ok());
        assert!("pkg::".parse::<TargetName>().is_err());
        assert!("::lib".parse::<TargetName>().is_err());
        assert!("pkg:lib".parse::<TargetName>().is_err());
        assert!("".parse::<TargetName>().is_err());
        assert_eq!(TargetName::new("pkg", "lib"), "pkg::lib".parse::<TargetName>().unwrap());
    }
}
