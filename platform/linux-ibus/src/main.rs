//! `ibus-engine-pkey`.
//!
//! * `--ibus`: started by `ibus-daemon` from the component file in `/usr/share/ibus/component`
//!   (installed by the package); the engine takes the component's D-Bus name.
//! * no argument or `--register`: started by hand or by the user's autostart; the engine
//!   announces itself to the running daemon with `RegisterComponent`, so no root is needed.
use std::time::Duration;

use ime_runtime::Runtime;
use pkey_ibus::dbus::{Factory, Shared, COMPONENT_NAME, FACTORY_PATH};
use pkey_ibus::{address, wire};
use zbus::connection::Builder;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version") {
        println!("ibus-engine-pkey {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "ibus-engine-pkey [--ibus | --register]\n\
             \n  --ibus      started by ibus-daemon (component file installed by the package)\
             \n  --register  announce the engine to the running ibus-daemon (default)"
        );
        return;
    }
    let by_daemon = args.iter().any(|a| a == "--ibus");
    let Some(addr) = address::find() else {
        eprintln!("ibus-engine-pkey: cannot find the IBus bus (is ibus-daemon running?)");
        std::process::exit(1);
    };
    let shared = Shared::new(Runtime::from_default_paths());
    let result = zbus::block_on(async {
        let mut builder = Builder::address(addr.as_str())?;
        if by_daemon {
            builder = builder.name(COMPONENT_NAME)?;
        }
        let conn = builder
            .serve_at(FACTORY_PATH, Factory::new(shared))?
            .build()
            .await?;
        if !by_daemon {
            let exe = std::env::current_exe()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "ibus-engine-pkey".into());
            let component =
                wire::component(&format!("{exe} --ibus"), env!("CARGO_PKG_VERSION"), "pkey");
            conn.call_method(
                Some("org.freedesktop.IBus"),
                "/org/freedesktop/IBus",
                Some("org.freedesktop.IBus"),
                "RegisterComponent",
                &(component,),
            )
            .await?;
        }
        // Leave when the daemon goes away, instead of lingering as an orphan.
        while !conn.is_closed() {
            async_io::Timer::after(Duration::from_secs(2)).await;
        }
        Ok::<(), zbus::Error>(())
    });
    if let Err(e) = result {
        eprintln!("ibus-engine-pkey: {e}");
        std::process::exit(1);
    }
}
