fn main() {
    let root = std::env::var("FFMPEG_ROOT").unwrap_or_else(|_| ".local/ffmpeg/ffmpeg-master-latest-win64-lgpl-shared".into());
    cc::Build::new().cpp(true).file("src/decoder.cpp").include(format!("{root}/include"))
        .flag_if_supported("/std:c++17").flag_if_supported("/O2").compile("screenios_video");
    println!("cargo:rerun-if-changed=src/decoder.cpp");
    println!("cargo:rerun-if-env-changed=FFMPEG_ROOT");
}
