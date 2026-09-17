use std::collections::BTreeMap;

use crate::Fact;

const FIELDS: [(&str, &str); 3] = [
    ("ProductName", "os.name"),
    ("ProductVersion", "os.version"),
    ("BuildVersion", "os.build"),
];

pub(crate) fn parse(output: &str) -> Result<Vec<Fact>, String> {
    let mut values = BTreeMap::new();

    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let (key, value) = line
            .split_once(':')
            .ok_or_else(|| format!("invalid sw_vers line: {line}"))?;
        let key = key.trim();
        if FIELDS.iter().any(|(required, _)| *required == key)
            && values.insert(key, value.trim()).is_some()
        {
            return Err(format!("duplicate sw_vers field: {key}"));
        }
    }

    FIELDS
        .into_iter()
        .map(|(source, target)| {
            let value = values
                .get(source)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("missing sw_vers field: {source}"))?;
            Ok(Fact {
                name: target.to_owned(),
                value: (*value).to_owned(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_macos_version_facts() {
        let facts =
            parse("ProductName:\t\tmacOS\nProductVersion:\t\t27.0\nBuildVersion:\t\t26A5388g\n")
                .unwrap();

        assert_eq!(facts[0].value, "macOS");
        assert_eq!(facts[1].value, "27.0");
        assert_eq!(facts[2].value, "26A5388g");
    }

    #[test]
    fn rejects_missing_fields() {
        assert_eq!(
            parse("ProductName: macOS\n").unwrap_err(),
            "missing sw_vers field: ProductVersion"
        );
    }

    #[test]
    fn rejects_duplicate_fields() {
        assert_eq!(
            parse(
                "ProductName: macOS\nProductName: macOS\nProductVersion: 27.0\nBuildVersion: 26A\n"
            )
            .unwrap_err(),
            "duplicate sw_vers field: ProductName"
        );
    }
}
