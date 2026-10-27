use bollard::models::ContainerStatsResponse;
use std::collections::HashMap;

use bollard::models::{
    ContainerCpuStats, ContainerCpuUsage, ContainerMemoryStats, ContainerNetworkStats,
};

use docker_tui::docker::stats::*;

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
