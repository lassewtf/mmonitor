#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParsedOutput {
    pub summary: String,
    pub metrics: Vec<PerformanceDatum>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PerformanceDatum {
    pub label: String,
    pub value: f64,
    pub unit: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

pub(crate) fn parse(output: &str) -> Result<ParsedOutput, String> {
    let (summary, performance_data) = output
        .split_once('|')
        .ok_or_else(|| "missing Nagios performance data separator".to_owned())?;
    let tokens = tokenize(performance_data)?;
    if tokens.is_empty() {
        return Err("missing Nagios performance data".to_owned());
    }

    let metrics = tokens
        .into_iter()
        .map(parse_datum)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ParsedOutput {
        summary: summary.trim().to_owned(),
        metrics,
    })
}

fn tokenize(input: &str) -> Result<Vec<&str>, String> {
    let bytes = input.as_bytes();
    let mut tokens = Vec::new();
    let mut start = 0;

    while start < bytes.len() {
        while start < bytes.len() && bytes[start].is_ascii_whitespace() {
            start += 1;
        }
        if start == bytes.len() {
            break;
        }

        let mut end = start;
        if bytes[start] == b'\'' {
            end += 1;
            while end < bytes.len() && bytes[end] != b'\'' {
                end += 1;
            }
            if end == bytes.len() {
                return Err("unterminated quoted performance label".to_owned());
            }
            end += 1;
        }
        while end < bytes.len() && !bytes[end].is_ascii_whitespace() {
            end += 1;
        }
        tokens.push(&input[start..end]);
        start = end;
    }

    Ok(tokens)
}

fn parse_datum(token: &str) -> Result<PerformanceDatum, String> {
    let (label, fields) = token
        .split_once('=')
        .ok_or_else(|| format!("performance datum has no value: {token}"))?;
    let label = label
        .strip_prefix('\'')
        .and_then(|label| label.strip_suffix('\''))
        .unwrap_or(label);
    if label.is_empty() {
        return Err("performance datum has an empty label".to_owned());
    }

    let fields = fields.split(';').collect::<Vec<_>>();
    let (value, unit) = parse_value(fields[0])?;

    Ok(PerformanceDatum {
        label: label.to_owned(),
        value,
        unit: unit.to_owned(),
        min: optional_number(fields.get(3).copied())?,
        max: optional_number(fields.get(4).copied())?,
    })
}

fn parse_value(input: &str) -> Result<(f64, &str), String> {
    for index in (1..=input.len()).rev() {
        if !input.is_char_boundary(index) {
            continue;
        }
        if let Ok(value) = input[..index].parse::<f64>()
            && value.is_finite()
        {
            return Ok((value, &input[index..]));
        }
    }
    Err(format!("invalid performance value: {input}"))
}

fn optional_number(input: Option<&str>) -> Result<Option<f64>, String> {
    let Some(input) = input.filter(|input| !input.is_empty()) else {
        return Ok(None);
    };
    let value = input
        .parse::<f64>()
        .map_err(|_| format!("invalid performance bound: {input}"))?;
    value
        .is_finite()
        .then_some(Some(value))
        .ok_or_else(|| format!("non-finite performance bound: {input}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multiline_monitoring_plugins_output() {
        let parsed =
            parse("[OK] - Scaled Load (10 CPUs): 1m: 5.2\n|'load1'=5.2;;; 'scaled_load1'=0.52;;;")
                .unwrap();

        assert!(parsed.summary.contains("10 CPUs"));
        assert_eq!(parsed.metrics[0].label, "load1");
        assert_eq!(parsed.metrics[1].value, 0.52);
    }

    #[test]
    fn parses_disk_maximum() {
        let parsed = parse("DISK |'/'=802577416192B;0;0;0;994631127040").unwrap();

        assert_eq!(parsed.metrics[0].unit, "B");
        assert_eq!(parsed.metrics[0].max, Some(994631127040.0));
    }
}
