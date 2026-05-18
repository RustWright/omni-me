use std::path::Path;

fn main() {
    apply_android_overrides();
    tauri_build::build();
}

/// Copy files from `android-overrides/` over the corresponding paths in
/// `gen/android/`. `gen/android/` is gitignored — Tauri regenerates it on
/// every `tauri android init`, which silently overwrites our customizations
/// (share-target intent-filter, MainActivity intent handler, etc.). Running
/// this on every cargo build guarantees the overrides are in place
/// whenever the Android project exists.
///
/// No-op on machines that haven't run `tauri android init` (desktop dev).
fn apply_android_overrides() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let gen_main = crate_root.join("gen/android/app/src/main");
    if !gen_main.exists() {
        return;
    }

    let overrides = crate_root.join("android-overrides");
    if !overrides.exists() {
        return;
    }

    let pairs: &[(&str, &str)] = &[
        ("AndroidManifest.xml", "AndroidManifest.xml"),
        (
            "java/com/omni_me/app/MainActivity.kt",
            "java/com/omni_me/app/MainActivity.kt",
        ),
    ];

    for (src_rel, dst_rel) in pairs {
        let src = overrides.join(src_rel);
        let dst = gen_main.join(dst_rel);
        if !src.exists() {
            continue;
        }
        // Cargo only re-runs build.rs when listed inputs change.
        println!("cargo:rerun-if-changed={}", src.display());

        if let Some(parent) = dst.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::copy(&src, &dst) {
            println!(
                "cargo:warning=failed to copy {} -> {}: {}",
                src.display(),
                dst.display(),
                e
            );
        }
    }
}
