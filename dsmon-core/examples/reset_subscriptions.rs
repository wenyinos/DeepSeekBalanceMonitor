//! Removes the demo keys and the fabricated subscription history.
//!
//! Run with: `cargo run -p dsmon-core --example reset_subscriptions`

const DEMO_KEY: &str = "seed-demo";

fn main() {
    // Drop the placeholder keys only while they still hold the seeded value, so
    // a real key entered since is left untouched.
    for key in [
        dsmon_core::storage::KEY_OPENCODE_GO,
        dsmon_core::storage::KEY_COMMAND_CODE,
    ] {
        match dsmon_core::storage::read_secret(key) {
            Ok(Some(value)) if value == DEMO_KEY => {
                match dsmon_core::storage::delete_secret(key) {
                    Ok(()) => println!("removed the demo key for {key}"),
                    Err(error) => println!("could not remove {key}: {error}"),
                }
            }
            Ok(Some(_)) => println!("kept {key}: it no longer holds the demo value"),
            Ok(None) => println!("{key} was already empty"),
            Err(error) => println!("could not read {key}: {error}"),
        }
    }

    // Every subscription reading so far came from the seeder.
    match dsmon_core::storage::prune_subscription_history(0) {
        Ok(()) => println!("cleared the subscription history"),
        Err(error) => println!("could not clear the history: {error}"),
    }
}
