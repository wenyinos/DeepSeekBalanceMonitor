//! Seeds demo readings so the subscription charts can be inspected.
//!
//! Run with: `cargo run -p dsmon-core --example seed_demo`

/// How many days of history to fabricate.
const DAYS: usize = 30;

/// A day's worth of consumption, cycled through so the curve has some shape.
const OPENCODE_DAILY: [f64; 10] = [1.5, 2.0, 3.2, 1.1, 0.8, 2.4, 1.9, 3.6, 2.2, 1.4];
const COMMAND_CODE_DAILY: [f64; 10] = [0.8, 1.4, 2.2, 0.6, 0.4, 1.8, 1.2, 2.6, 1.5, 0.9];

fn main() {
    // A placeholder key puts each subscription into its "configured" state,
    // which is what makes its chart appear at all.
    let _ = dsmon_core::storage::store_secret(dsmon_core::storage::KEY_OPENCODE_GO, "seed-demo");
    let _ = dsmon_core::storage::store_secret(dsmon_core::storage::KEY_COMMAND_CODE, "seed-demo");

    seed(
        dsmon_core::storage::PROVIDER_OPENCODE_GO,
        100.0,
        &OPENCODE_DAILY,
    );
    seed(
        dsmon_core::storage::PROVIDER_COMMAND_CODE,
        70.0,
        &COMMAND_CODE_DAILY,
    );
}

/// Writes one reading per day, each carrying the running total.
fn seed(provider: &str, cap: f64, daily: &[f64]) {
    let mut used = 0.0;
    for step in 0..DAYS {
        used += daily[step % daily.len()];
        let days_ago = DAYS - 1 - step;
        let timestamp = timestamp_days_ago(days_ago);
        if let Err(error) =
            dsmon_core::storage::save_subscription_usage_at(provider, used, cap, &timestamp)
        {
            println!("seeding {provider} failed: {error}");
            return;
        }
    }
    println!("seeded {provider}: {used:.1} of {cap} over {DAYS} days");
}

fn timestamp_days_ago(days: usize) -> String {
    let output = std::process::Command::new("date")
        .args(["-d", &format!("{days} days ago"), "+%Y-%m-%d %H:%M:%S"])
        .output()
        .expect("the date command runs");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}
