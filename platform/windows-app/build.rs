//! Ask Windows for the modern look (comctl32 v6) and embed the application icon.
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=app.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        println!(
            "cargo:rustc-link-arg-bins=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );

        let out_dir = std::env::var("OUT_DIR").unwrap();
        let res_path = Path::new(&out_dir).join("app.res");

        let rc_candidates = [
            "rc.exe",
            r"C:\Program Files (x86)\Windows Kits\10\bin\10.0.22621.0\x64\rc.exe",
            r"C:\Program Files (x86)\Windows Kits\10\bin\10.0.22000.0\x64\rc.exe",
            r"C:\Program Files (x86)\Windows Kits\10\bin\10.0.19041.0\x64\rc.exe",
        ];

        let mut compiled = false;
        for rc in &rc_candidates {
            if let Ok(status) = std::process::Command::new(rc)
                .arg("/fo")
                .arg(&res_path)
                .arg("app.rc")
                .status()
            {
                if status.success() {
                    compiled = true;
                    break;
                }
            }
        }

        if compiled {
            println!("cargo:rustc-link-arg-bins={}", res_path.display());
        } else {
            // The linker runs from the workspace root, so the checked-in resource needs an
            // absolute path (a bare "app.res" is "cannot open input file" on CI).
            let checked_in =
                Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("app.res");
            if checked_in.exists() {
                println!("cargo:rustc-link-arg-bins={}", checked_in.display());
            }
        }
    }
}
