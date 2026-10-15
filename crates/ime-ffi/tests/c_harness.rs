//! Builds `tests/ffi/harness.c` with the system C compiler, links it against the cdylib and runs
//! it. Set `PKEY_REQUIRE_C_HARNESS=1` (CI does) to fail instead of skipping when no C compiler
//! is available, and `PKEY_C_SANITIZE=address,undefined` to build the harness with sanitizers.

use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `target/<profile>`: the test executable lives in `target/<profile>/deps`.
fn profile_dir() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    exe.parent().unwrap().parent().unwrap().to_path_buf()
}

fn host_triple() -> String {
    let out = Command::new("rustc").arg("-vV").output().unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("host: ").map(str::to_string))
        .expect("rustc reports its host")
}

/// Deletes the scratch folder when the test ends, however it ends.
struct RemoveOnDrop(PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pkey-c-harness-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn c_harness_passes() {
    let require = std::env::var_os("PKEY_REQUIRE_C_HARNESS").is_some();
    let skip = |why: &str| {
        assert!(!require, "C harness required but unavailable: {why}");
        eprintln!("SKIPPED c_harness: {why}");
    };

    let host = host_triple();
    let tmp = scratch_dir();
    let _cleanup = RemoveOnDrop(tmp.clone());
    let compiler = cc::Build::new()
        .target(&host)
        .host(&host)
        .opt_level(0)
        .debug(true)
        .out_dir(&tmp)
        .cargo_metadata(false)
        .cargo_warnings(false)
        .try_get_compiler();
    let compiler = match compiler {
        Ok(c) => c,
        Err(e) => return skip(&format!("no C compiler found ({e})")),
    };

    let lib_dir = profile_dir();
    // `cargo test` builds the rlib only; make sure the cdylib exists and is current.
    let mut build_lib = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    build_lib
        .args(["build", "-p", "ime-ffi", "--lib"])
        .env("CARGO_TARGET_DIR", lib_dir.parent().unwrap());
    if lib_dir.file_name().is_some_and(|n| n == "release") {
        build_lib.arg("--release");
    }
    let built = build_lib.output().expect("run cargo build");
    assert!(
        built.status.success(),
        "cargo build -p ime-ffi failed:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let import_lib = if compiler.is_like_msvc() {
        lib_dir.join("pkey_ffi.dll.lib")
    } else {
        lib_dir.join("libpkey_ffi.so")
    };
    if !import_lib.exists() && !lib_dir.join("libpkey_ffi.dylib").exists() {
        return skip(&format!("cdylib not found in {}", lib_dir.display()));
    }

    let root = workspace_root();
    let harness = root.join("tests/ffi/harness.c");
    let include = root.join("crates/ime-ffi/include");
    let exe = tmp.join(if cfg!(windows) {
        "harness.exe"
    } else {
        "harness"
    });
    let sanitize = std::env::var("PKEY_C_SANITIZE").ok();

    let mut cmd = compiler.to_command();
    if compiler.is_like_msvc() {
        cmd.arg(&harness)
            .arg(format!("/I{}", include.display()))
            .arg(format!("/Fe{}", exe.display()))
            .arg(format!("/Fo{}\\", tmp.display()))
            .arg(&import_lib);
        if sanitize.is_some() {
            cmd.arg("/fsanitize=address");
        }
    } else {
        cmd.arg(&harness)
            .arg("-I")
            .arg(&include)
            .arg("-L")
            .arg(&lib_dir)
            .arg("-lpkey_ffi")
            .arg(format!("-Wl,-rpath,{}", lib_dir.display()))
            .arg("-o")
            .arg(&exe);
        if let Some(s) = &sanitize {
            cmd.arg(format!("-fsanitize={s}"))
                .arg("-fno-omit-frame-pointer");
        }
    }
    let build = cmd.output().expect("run the C compiler");
    assert!(
        build.status.success(),
        "C compile failed:\n{}\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    // Windows finds the DLL on PATH; Unix uses the rpath / LD_LIBRARY_PATH.
    let sep = if cfg!(windows) { ";" } else { ":" };
    let path = format!(
        "{}{sep}{}",
        lib_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let run = Command::new(&exe)
        .env("PATH", &path)
        .env("LD_LIBRARY_PATH", &lib_dir)
        .env("DYLD_LIBRARY_PATH", &lib_dir)
        .output()
        .expect("run the harness");
    assert!(
        run.status.success(),
        "harness failed ({:?}):\n{}\n{}",
        run.status.code(),
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(String::from_utf8_lossy(&run.stdout).contains("ok"));
}
