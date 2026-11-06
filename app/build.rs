use std::fmt::Write as _;

fn main() {
    slint_build::compile("ui/main.slint").expect("Slint UI compile failed");

    // Material Design Icons name -> codepoint table (from ui/fonts/mdi-codepoints.txt).
    let table = std::fs::read_to_string("ui/fonts/mdi-codepoints.txt").expect("mdi-codepoints.txt");
    let mut out = String::from("pub static MDI: &[(&str, u32)] = &[\n");
    let mut entries: Vec<(String, u32)> = table
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| {
            let (name, hex) = l.split_once('\t')?;
            Some((name.to_string(), u32::from_str_radix(hex.trim(), 16).ok()?))
        })
        .collect();
    entries.sort();
    for (name, cp) in entries {
        writeln!(out, "    ({name:?}, 0x{cp:X}),").unwrap();
    }
    out.push_str("];\n");
    let dest = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("mdi_table.rs");
    std::fs::write(dest, out).unwrap();
    println!("cargo:rerun-if-changed=ui/fonts/mdi-codepoints.txt");
    println!("cargo:rerun-if-changed=ui");

    // Build time, reported to Home Assistant as the "compilation time".
    let built = std::process::Command::new("date")
        .arg("+%b %e %Y, %H:%M:%S")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    println!("cargo:rustc-env=NSPANEL_BUILD_TIME={}", built.trim());

    // Wake word engine (C++, TensorFlow Lite + micro frontend), only for the
    // panel (the TFLite library comes from the Buildroot sysroot).
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("aarch64") {
        let fe = "native/microfrontend/tensorflow/lite/experimental/microfrontend/lib";
        let c_files = [
            "filterbank.c", "filterbank_util.c", "frontend.c", "frontend_util.c", "log_lut.c", "log_scale.c",
            "log_scale_util.c", "noise_reduction.c", "noise_reduction_util.c", "pcan_gain_control.c",
            "pcan_gain_control_util.c", "window.c", "window_util.c",
        ];
        let mut c = cc::Build::new();
        c.include("native/microfrontend").include("native/kissfft").include("native/kissfft/tools")
            .define("FIXED_POINT", "16").flag("-mcpu=cortex-a35").warnings(false);
        for f in c_files {
            c.file(format!("{fe}/{f}"));
        }
        c.file("native/kissfft/kiss_fft.c").file("native/kissfft/tools/kiss_fftr.c");
        // C objects go into the same static library as the C++ part (they
        // reference each other, so two libraries would depend on link order).
        let c_objects = c.compile_intermediates();
        cc::Build::new()
            .cpp(true)
            .objects(c_objects)
            .std("c++17")
            .include("native/microfrontend")
            .include("native/kissfft")
            .include("native/kissfft/tools")
            .define("FIXED_POINT", "16")
            .flag("-mcpu=cortex-a35")
            .warnings(false)
            .file(format!("{fe}/fft.cc"))
            .file(format!("{fe}/fft_util.cc"))
            .file("native/mww.cc")
            .compile("mww");
        println!("cargo:rustc-link-lib=tensorflow-lite");

        // Echo cancellation (WebRTC AudioProcessing from the Buildroot sysroot;
        // include flags from its pkg-config file through Buildroot's wrapper).
        let pkg_config = std::env::var("PKG_CONFIG").unwrap_or_else(|_| "pkg-config".into());
        let cflags = std::process::Command::new(&pkg_config)
            .args(["--cflags", "webrtc-audio-processing-1"])
            .output()
            .expect("pkg-config webrtc-audio-processing-1");
        assert!(cflags.status.success(), "pkg-config: webrtc-audio-processing-1 not found");
        let mut aec = cc::Build::new();
        aec.cpp(true).std("c++17").flag("-mcpu=cortex-a35").warnings(false).file("native/aec.cc");
        for flag in String::from_utf8_lossy(&cflags.stdout).split_whitespace() {
            if let Some(dir) = flag.strip_prefix("-I") {
                aec.include(dir);
            } else if let Some(def) = flag.strip_prefix("-D") {
                let (k, v) = def.split_once('=').map(|(k, v)| (k, Some(v))).unwrap_or((def, None));
                aec.define(k, v);
            }
        }
        aec.compile("aec");
        println!("cargo:rustc-link-lib=webrtc-audio-processing-1");
        println!("cargo:rerun-if-changed=native");
    }

    // ESPHome native API messages (proto/api.proto) -> Rust types.
    let fds = protox::compile(["proto/api.proto"], ["proto"]).expect("api.proto");
    prost_build::Config::new().compile_fds(fds).expect("prost codegen");
    println!("cargo:rerun-if-changed=proto");
}
