//! Memory and CPU of each service's process tree.
//!
//! One `ps` call lists every process (pid, ppid, rss, %cpu); each service's
//! usage is its spawned pid plus all descendants (a `sh -c` → turbo → vite
//! chain). On macOS memory is the physical footprint from `proc_pid_rusage`,
//! which counts compressed pages the way Activity Monitor and `top` do (RSS
//! alone under-reports a leaking dev server by gigabytes); elsewhere it is RSS.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Usage {
    /// Bytes.
    pub mem: u64,
    /// Percent of one core (can exceed 100).
    pub cpu: f32,
}

struct Proc {
    ppid: u32,
    rss_kb: u64,
    cpu: f32,
}

/// Usage of each `(name, root pid)`; services whose pid is gone are omitted.
pub async fn sample(roots: &[(String, u32)]) -> HashMap<String, Usage> {
    if roots.is_empty() {
        return HashMap::new();
    }
    let Ok(out) = tokio::process::Command::new("ps")
        .args(["-axo", "pid=,ppid=,rss=,%cpu="])
        .output()
        .await
    else {
        return HashMap::new();
    };
    let procs = parse_ps(&String::from_utf8_lossy(&out.stdout));
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for (&pid, p) in &procs {
        children.entry(p.ppid).or_default().push(pid);
    }
    roots
        .iter()
        .filter(|(_, pid)| procs.contains_key(pid))
        .map(|(name, root)| {
            let mut u = Usage::default();
            let mut stack = vec![*root];
            while let Some(pid) = stack.pop() {
                let Some(p) = procs.get(&pid) else { continue };
                u.mem += footprint(pid).unwrap_or(p.rss_kb * 1024);
                u.cpu += p.cpu;
                if let Some(kids) = children.get(&pid) {
                    stack.extend(kids);
                }
            }
            (name.clone(), u)
        })
        .collect()
}

fn parse_ps(text: &str) -> HashMap<u32, Proc> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            let pid = f.next()?.parse().ok()?;
            let ppid = f.next()?.parse().ok()?;
            let rss_kb = f.next()?.parse().ok()?;
            let cpu = f.next()?.replace(',', ".").parse().ok()?;
            Some((pid, Proc { ppid, rss_kb, cpu }))
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn footprint(pid: u32) -> Option<u64> {
    let mut info = std::mem::MaybeUninit::<libc::rusage_info_v2>::zeroed();
    // SAFETY: the buffer is a rusage_info_v2, matching the RUSAGE_INFO_V2 flavor.
    let rc = unsafe {
        libc::proc_pid_rusage(
            pid as libc::c_int,
            libc::RUSAGE_INFO_V2,
            info.as_mut_ptr().cast(),
        )
    };
    // SAFETY: rc == 0 means the kernel filled the struct.
    (rc == 0).then(|| unsafe { info.assume_init() }.ri_phys_footprint)
}

#[cfg(not(target_os = "macos"))]
fn footprint(_pid: u32) -> Option<u64> {
    None
}

/// `512M`, `3G`, `1.5GB`, `800mb`, or plain bytes.
pub fn parse_size(s: &str) -> Option<u64> {
    let t = s.trim().to_ascii_uppercase();
    let t = t.strip_suffix('B').unwrap_or(&t);
    let (num, mul) = match t.chars().last()? {
        'K' => (&t[..t.len() - 1], 1u64 << 10),
        'M' => (&t[..t.len() - 1], 1 << 20),
        'G' => (&t[..t.len() - 1], 1 << 30),
        _ => (t, 1),
    };
    let n: f64 = num.trim().parse().ok()?;
    (n > 0.0).then_some((n * mul as f64) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(parse_size("512M"), Some(512 << 20));
        assert_eq!(parse_size("3G"), Some(3 << 30));
        assert_eq!(parse_size("1.5gb"), Some(3 << 29));
        assert_eq!(parse_size("2048"), Some(2048));
        assert_eq!(parse_size("lots"), None);
        assert_eq!(parse_size("0M"), None);
    }

    #[tokio::test]
    async fn sums_the_process_tree() {
        let mut child = tokio::process::Command::new("sh")
            .args(["-c", "sleep 30 & sleep 30 & wait"])
            .spawn()
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let pid = child.id().unwrap();
        let only_sh = {
            let out = sample(&[("x".into(), pid)]).await;
            out["x"].mem
        };
        let ps = parse_ps(&String::from_utf8_lossy(
            &std::process::Command::new("ps")
                .args(["-axo", "pid=,ppid=,rss=,%cpu="])
                .output()
                .unwrap()
                .stdout,
        ));
        let kids = ps.values().filter(|p| p.ppid == pid).count();
        assert_eq!(kids, 2, "two sleeps under sh");
        let own = footprint(pid).unwrap_or(ps[&pid].rss_kb * 1024);
        assert!(
            only_sh > own,
            "tree total {only_sh} must exceed the root alone {own}"
        );
        child.kill().await.unwrap();
    }
}
