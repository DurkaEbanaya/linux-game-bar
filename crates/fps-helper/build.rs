fn main() {
    println!("cargo:rerun-if-changed=src/present.bpf.c");
    let output =
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("present.bpf.o");
    let status =
        std::process::Command::new(std::env::var("CLANG").unwrap_or_else(|_| "clang".into()))
            .args([
                "-target",
                "bpf",
                "-O2",
                "-g",
                "-Wall",
                "-Werror",
                "-c",
                "src/present.bpf.c",
                "-o",
            ])
            .arg(output)
            .status()
            .expect("clang is required to compile FPS eBPF probes");
    assert!(status.success(), "eBPF compilation failed");
}
