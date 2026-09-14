//! Desktop notifications over D-Bus.
//!
//! ksni speaks StatusNotifierItem and nothing else, so the call is made
//! directly. A session with no notification service is not an error: the
//! message is dropped, which is what the plan asks for.

use std::collections::HashMap;

use zbus::zvariant::Value;

use super::Message;

const SERVICE: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const INTERFACE: &str = "org.freedesktop.Notifications";

/// Hands the message to the desktop.
pub fn send(message: &Message) -> Result<(), String> {
    let connection = zbus::blocking::Connection::session().map_err(|error| error.to_string())?;

    // `Notify(susssasa{sv}i)`: application, replaces id, icon, summary, body,
    // actions, hints, timeout. A negative timeout leaves the lifetime to the
    // service, which is what a notice about a balance should do.
    let actions: Vec<&str> = Vec::new();
    let hints: HashMap<&str, Value<'_>> = HashMap::new();

    connection
        .call_method(
            Some(SERVICE),
            PATH,
            Some(INTERFACE),
            "Notify",
            &(
                dsmon_core::APP_NAME,
                0u32,
                "",
                message.title.as_str(),
                message.body.as_str(),
                &actions,
                &hints,
                -1i32,
            ),
        )
        .map_err(|error| error.to_string())?;

    Ok(())
}
