use bollard::models::ContainerStatsResponse;

/// One resource usage sample, derived from a Docker stats frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StatsSample {
    pub cpu_percent: f64,
    pub mem_used: u64,
    pub mem_limit: u64,
    pub net_rx: u64,
    pub net_tx: u64,
}

impl StatsSample {
    pub fn mem_percent(&self) -> f64 {
        if self.mem_limit == 0 {
            0.0
        } else {
            self.mem_used as f64 / self.mem_limit as f64 * 100.0
        }
    }
}

/// Compute usage the way `docker stats` does.
pub fn compute(s: &ContainerStatsResponse) -> StatsSample {
    let mut sample = StatsSample::default();

    if let (Some(cpu), Some(pre)) = (&s.cpu_stats, &s.precpu_stats) {
        let total = |c: &bollard::models::ContainerCpuStats| {
            c.cpu_usage.as_ref().and_then(|u| u.total_usage)
        };
        if let (Some(t), Some(pt), Some(sys), Some(psys)) = (
            total(cpu),
            total(pre),
            cpu.system_cpu_usage,
            pre.system_cpu_usage,
        ) {
            let cpu_delta = t.saturating_sub(pt) as f64;
            let sys_delta = sys.saturating_sub(psys) as f64;
            let cpus = cpu
                .online_cpus
                .map(f64::from)
                .or_else(|| {
                    cpu.cpu_usage
                        .as_ref()
                        .and_then(|u| u.percpu_usage.as_ref())
                        .map(|p| p.len() as f64)
                })
                .unwrap_or(1.0);
            if sys_delta > 0.0 {
                sample.cpu_percent = cpu_delta / sys_delta * cpus * 100.0;
            }
        }
    }

    if let Some(mem) = &s.memory_stats {
        // Page cache is reclaimable, so `docker stats` excludes it.
        let cache = mem
            .stats
            .as_ref()
            .and_then(|st| {
                st.get("inactive_file")
                    .or_else(|| st.get("total_inactive_file"))
                    .or_else(|| st.get("cache"))
            })
            .copied()
            .unwrap_or(0);
        sample.mem_used = mem.usage.unwrap_or(0).saturating_sub(cache);
        sample.mem_limit = mem.limit.unwrap_or(0);
    }

    for net in s.networks.iter().flat_map(|n| n.values()) {
        sample.net_rx += net.rx_bytes.unwrap_or(0);
        sample.net_tx += net.tx_bytes.unwrap_or(0);
    }
    sample
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use bollard::models::{
        ContainerCpuStats, ContainerCpuUsage, ContainerMemoryStats, ContainerNetworkStats,
    };

    use super::*;

    fn cpu(total: u64, system: u64) -> ContainerCpuStats {
        ContainerCpuStats {
            cpu_usage: Some(ContainerCpuUsage {
                total_usage: Some(total),
                ..Default::default()
            }),
            system_cpu_usage: Some(system),
            online_cpus: Some(4),
            ..Default::default()
        }
    }

    fn net(rx: u64, tx: u64) -> ContainerNetworkStats {
        ContainerNetworkStats {
            rx_bytes: Some(rx),
            tx_bytes: Some(tx),
            ..Default::default()
        }
    }

    #[test]
    fn computes_cpu_memory_and_network() {
        let s = ContainerStatsResponse {
            cpu_stats: Some(cpu(1_200, 10_000)),
            precpu_stats: Some(cpu(1_000, 8_000)),
            memory_stats: Some(ContainerMemoryStats {
                usage: Some(150),
                limit: Some(1_000),
                stats: Some(HashMap::from([("inactive_file".to_string(), 50)])),
                ..Default::default()
            }),
            networks: Some(HashMap::from([
                ("eth0".to_string(), net(10, 20)),
                ("eth1".to_string(), net(1, 2)),
            ])),
            ..Default::default()
        };
        let sample = compute(&s);
        // 200 / 2000 * 4 cpus * 100
        assert!((sample.cpu_percent - 40.0).abs() < 1e-9);
        assert_eq!(sample.mem_used, 100);
        assert_eq!(sample.mem_limit, 1_000);
        assert!((sample.mem_percent() - 10.0).abs() < 1e-9);
        assert_eq!((sample.net_rx, sample.net_tx), (11, 22));
    }

    #[test]
    fn first_frame_without_precpu_is_zero_cpu() {
        let s = ContainerStatsResponse {
            cpu_stats: Some(cpu(1_200, 10_000)),
            ..Default::default()
        };
        assert_eq!(compute(&s).cpu_percent, 0.0);
    }

    #[test]
    fn empty_frame_is_all_zero() {
        assert_eq!(
            compute(&ContainerStatsResponse::default()),
            StatsSample::default()
        );
    }
}
