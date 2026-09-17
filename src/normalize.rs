use crate::{Metric, config::CheckKind, nagios::ParsedOutput};

pub(crate) fn normalize(kind: CheckKind, output: &ParsedOutput) -> Result<Vec<Metric>, String> {
    match kind {
        CheckKind::SystemDisk => disk(output),
        CheckKind::CpuLoad => cpu_load(output),
        CheckKind::Memory => memory(output),
    }
}

fn disk(output: &ParsedOutput) -> Result<Vec<Metric>, String> {
    let disk = output
        .metrics
        .iter()
        .find(|metric| metric.label == "/")
        .ok_or_else(|| "missing root filesystem metric".to_owned())?;
    if disk.unit != "B" {
        return Err(format!(
            "root filesystem uses unexpected unit: {}",
            disk.unit
        ));
    }
    let total = disk
        .max
        .ok_or_else(|| "root filesystem metric has no maximum".to_owned())?;
    if disk.value < 0.0 || total <= 0.0 || disk.value > total {
        return Err("root filesystem values are inconsistent".to_owned());
    }

    Ok(vec![
        bytes("filesystem.total", total),
        bytes("filesystem.used", disk.value),
        bytes("filesystem.available", total - disk.value),
    ])
}

fn cpu_load(output: &ParsedOutput) -> Result<Vec<Metric>, String> {
    let logical_cpus = logical_cpu_count(&output.summary)?;
    let labels = [
        ("load1", "cpu.load.1m"),
        ("load5", "cpu.load.5m"),
        ("load15", "cpu.load.15m"),
        ("scaled_load1", "cpu.load_per_logical_cpu.1m"),
        ("scaled_load5", "cpu.load_per_logical_cpu.5m"),
        ("scaled_load15", "cpu.load_per_logical_cpu.15m"),
    ];
    let mut metrics = vec![Metric {
        name: "cpu.logical_cpus".to_owned(),
        value: f64::from(logical_cpus),
        unit: None,
    }];

    for (source, target) in labels {
        let metric = output
            .metrics
            .iter()
            .find(|metric| metric.label == source)
            .ok_or_else(|| format!("missing CPU metric: {source}"))?;
        if !metric.unit.is_empty() || metric.value < 0.0 {
            return Err(format!("invalid CPU metric: {source}"));
        }
        metrics.push(Metric {
            name: target.to_owned(),
            value: metric.value,
            unit: None,
        });
    }

    Ok(metrics)
}

fn logical_cpu_count(summary: &str) -> Result<u32, String> {
    let marker = "Scaled Load (";
    let rest = summary
        .find(marker)
        .map(|index| &summary[index + marker.len()..])
        .ok_or_else(|| "CPU summary has no logical CPU count".to_owned())?;
    let end = rest
        .find(" CPUs)")
        .ok_or_else(|| "CPU summary has an invalid logical CPU count".to_owned())?;
    let count = rest[..end]
        .parse::<u32>()
        .map_err(|_| "CPU summary has an invalid logical CPU count".to_owned())?;
    if count == 0 {
        return Err("CPU summary reports zero logical CPUs".to_owned());
    }
    Ok(count)
}

fn memory(output: &ParsedOutput) -> Result<Vec<Metric>, String> {
    let labels = [
        ("memory.total", "bytes"),
        ("memory.used", "bytes"),
        ("memory.available", "bytes"),
        ("memory.available_non_compressed", "bytes"),
        ("memory.free", "bytes"),
        ("memory.wired", "bytes"),
        ("memory.compressed", "bytes"),
        ("memory.system_free_percent", "percent"),
        ("swap.total", "bytes"),
        ("swap.used", "bytes"),
        ("swap.free", "bytes"),
    ];

    labels
        .into_iter()
        .map(|(name, unit)| {
            let source = output
                .metrics
                .iter()
                .find(|metric| metric.label == name)
                .ok_or_else(|| format!("missing memory metric: {name}"))?;
            let expected = if unit == "bytes" { "B" } else { "%" };
            if source.unit != expected || source.value < 0.0 {
                return Err(format!("invalid memory metric: {name}"));
            }
            Ok(Metric {
                name: name.to_owned(),
                value: source.value,
                unit: Some(unit.to_owned()),
            })
        })
        .collect()
}

fn bytes(name: &str, value: f64) -> Metric {
    Metric {
        name: name.to_owned(),
        value,
        unit: Some("bytes".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use crate::nagios;

    use super::*;

    #[test]
    fn normalizes_disk_usage() {
        let output = nagios::parse("DISK |'/'=80B;0;0;0;100").unwrap();
        let metrics = normalize(CheckKind::SystemDisk, &output).unwrap();

        assert_eq!(metrics[0].value, 100.0);
        assert_eq!(metrics[2].value, 20.0);
    }

    #[test]
    fn normalizes_cpu_load() {
        let output = nagios::parse(
            "Scaled Load (10 CPUs) |'load1'=5;;; 'load5'=4;;; 'load15'=3;;; \
             'scaled_load1'=0.5;;; 'scaled_load5'=0.4;;; 'scaled_load15'=0.3;;;",
        )
        .unwrap();
        let metrics = normalize(CheckKind::CpuLoad, &output).unwrap();

        assert_eq!(metrics[0].value, 10.0);
        assert_eq!(metrics[4].value, 0.5);
    }

    #[test]
    fn normalizes_memory() {
        let output = nagios::parse(
            "MEMORY |'memory.total'=100B;;;; 'memory.used'=80B;;;; \
             'memory.available'=60B;;;; 'memory.available_non_compressed'=30B;;;; \
             'memory.free'=5B;;;; 'memory.wired'=20B;;;; 'memory.compressed'=30B;;;; \
             'memory.system_free_percent'=30%;;;; 'swap.total'=50B;;;; \
             'swap.used'=40B;;;; 'swap.free'=10B;;;;",
        )
        .unwrap();
        let metrics = normalize(CheckKind::Memory, &output).unwrap();

        assert_eq!(metrics.len(), 11);
        assert_eq!(metrics[7].unit.as_deref(), Some("percent"));
    }
}
