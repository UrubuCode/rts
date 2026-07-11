//! Linux `os.cpus()` — `/proc/cpuinfo` (model/MHz) + `/proc/stat` (per-core
//! tick counters, converted to milliseconds via `sysconf(_SC_CLK_TCK)`).

use super::{CpuInfo, CpuTimes};

pub fn collect() -> Vec<CpuInfo> {
    let (models, speeds) = parse_cpuinfo();
    let times = parse_stat();
    let n = times.len().max(models.len());
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        out.push(CpuInfo {
            model: models.get(i).cloned().unwrap_or_default(),
            speed: speeds.get(i).copied().unwrap_or(0),
            times: times.get(i).copied().unwrap_or_default(),
        });
    }
    out
}

/// Per-core `(model name, cpu MHz)` from `/proc/cpuinfo`.
fn parse_cpuinfo() -> (Vec<String>, Vec<i64>) {
    let mut models = Vec::new();
    let mut speeds = Vec::new();
    let content = match std::fs::read_to_string("/proc/cpuinfo") {
        Ok(c) => c,
        Err(_) => return (models, speeds),
    };
    let (mut cur_model, mut cur_speed) = (String::new(), 0i64);
    let mut have = false;
    for line in content.lines() {
        if line.is_empty() {
            if have {
                models.push(std::mem::take(&mut cur_model));
                speeds.push(cur_speed);
                cur_speed = 0;
                have = false;
            }
            continue;
        }
        if let Some((key, val)) = line.split_once(':') {
            let key = key.trim();
            let val = val.trim();
            match key {
                "model name" | "Processor" | "cpu model" => {
                    cur_model = val.to_string();
                    have = true;
                }
                "cpu MHz" | "clock" => {
                    cur_speed = val
                        .trim_end_matches("MHz")
                        .trim()
                        .parse::<f64>()
                        .map(|f| f as i64)
                        .unwrap_or(0);
                    have = true;
                }
                _ => {}
            }
        }
    }
    if have {
        models.push(cur_model);
        speeds.push(cur_speed);
    }
    (models, speeds)
}

/// Per-core cumulative times (ms) from `/proc/stat` `cpuN` lines.
fn parse_stat() -> Vec<CpuTimes> {
    let hz = clk_tck();
    let ms_per_tick = 1000.0 / hz;
    let mut out = Vec::new();
    let content = match std::fs::read_to_string("/proc/stat") {
        Ok(c) => c,
        Err(_) => return out,
    };
    for line in content.lines() {
        // Skip the aggregate "cpu " line; take "cpu0", "cpu1", …
        if !line.starts_with("cpu") || line.starts_with("cpu ") {
            continue;
        }
        let mut it = line.split_whitespace();
        let label = it.next().unwrap_or("");
        if !label[3..].chars().all(|c| c.is_ascii_digit()) || label.len() == 3 {
            continue;
        }
        let nums: Vec<f64> = it.filter_map(|s| s.parse::<f64>().ok()).collect();
        // user nice system idle iowait irq softirq [steal guest guest_nice]
        let get = |i: usize| nums.get(i).copied().unwrap_or(0.0);
        out.push(CpuTimes {
            user: get(0) * ms_per_tick,
            nice: get(1) * ms_per_tick,
            sys: get(2) * ms_per_tick,
            idle: get(3) * ms_per_tick,
            irq: get(5) * ms_per_tick,
        });
    }
    out
}

fn clk_tck() -> f64 {
    let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if hz > 0 {
        hz as f64
    } else {
        100.0
    }
}
