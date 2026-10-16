//! Real SIGKILL atomic-save proof, including stale-temp recovery on next open.
use nib_vault::{Guard, VaultHandle, fsx};
use std::{
    fs,
    io::{BufRead, BufReader},
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime},
};
const SIZE: usize = 1024 * 1024;
#[test]
fn two_hundred_killed_writers_keep_complete_notes() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("crash.md");
    let temporary = d.path().join(".crash.nib-tmp");
    let old = vec![b'A'; SIZE];
    let new = vec![b'B'; SIZE];
    fs::write(&path, &old).unwrap();
    let start = Instant::now();
    let mut seed = 7_u64;
    let mut recovered = 0;
    for _ in 0..200 {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "subprocess_crash_writer",
                "--nocapture",
            ])
            .env("NIB_CRASH_ROOT", d.path())
            .env("NIB_ALLOW_ROOTS", d.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        loop {
            line.clear();
            assert!(
                output.read_line(&mut line).unwrap() > 0,
                "writer exited before saving"
            );
            if line.contains("nib-writer-ready") {
                break;
            }
        }
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        std::thread::sleep(Duration::from_micros(seed % 2500));
        child.kill().unwrap();
        let status = child.wait().unwrap();
        assert!(!status.success());
        let bytes = fs::read(&path).unwrap();
        assert!(
            bytes == old || bytes == new,
            "a kill produced a partial note"
        );
        if temporary.exists() {
            let file = fs::File::options().write(true).open(&temporary).unwrap();
            file.set_modified(SystemTime::now() - Duration::from_secs(120))
                .unwrap();
            let (h, _rx) = VaultHandle::open(
                d.path(),
                &nib_core::settings::Config::default(),
                Guard::confined(vec![d.path().into()]),
            )
            .unwrap();
            let cleanup = Instant::now();
            while temporary.exists() {
                assert!(cleanup.elapsed() < Duration::from_secs(1));
                std::thread::sleep(Duration::from_millis(1));
            }
            drop(h);
            recovered += 1;
        }
    }
    assert!(
        recovered > 0,
        "the probe never interrupted a temporary write"
    );
    eprintln!(
        "crash_kills=200 recovered_temps={recovered} elapsed={:.3}s",
        start.elapsed().as_secs_f64()
    );
    assert!(start.elapsed() < Duration::from_secs(20));
}
#[test]
#[ignore = "invoked by the parent with an isolated root, then killed"]
fn subprocess_crash_writer() {
    let root = std::path::PathBuf::from(std::env::var_os("NIB_CRASH_ROOT").unwrap());
    let path = root.join("crash.md");
    let guard = Guard::confined(vec![root]);
    let a = vec![b'A'; SIZE];
    let b = vec![b'B'; SIZE];
    let mut n = 0_u64;
    loop {
        fsx::save(&guard, &path, if n.is_multiple_of(2) { &b } else { &a }).unwrap();
        if n == 0 {
            println!("nib-writer-ready");
        }
        n += 1;
    }
}
