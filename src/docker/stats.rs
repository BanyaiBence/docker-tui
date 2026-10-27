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
