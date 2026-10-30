fn main() {
    let output = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let logo = image::open("screenios.png").expect("Missing project logo");
    let icon = output.join("screenios.ico");
    let logo = logo.resize_exact(256, 256, image::imageops::FilterType::Lanczos3);
    logo.save_with_format(output.join("screenios.png"), image::ImageFormat::Png)
        .expect("Build window icon");
    logo.save_with_format(&icon, image::ImageFormat::Ico)
        .expect("Build Windows icon");
    winresource::WindowsResource::new()
        .set_icon(icon.to_str().unwrap())
        .set("ProductName", "ScreenIOS")
        .set("FileDescription", "ScreenIOS USB iPhone viewer")
        .compile()
        .expect("Embed Windows icon");
    let root = std::env::var("FFMPEG_ROOT")
        .unwrap_or_else(|_| ".local/ffmpeg/ffmpeg-N-128142-g28fda48e0b-win64-lgpl-shared".into());
    cc::Build::new()
        .cpp(true)
        .file("src/decoder.cpp")
        .include(format!("{root}/include"))
        .flag_if_supported("/std:c++17")
        .flag_if_supported("/O2")
        .compile("screenios_video");
    println!("cargo:rerun-if-changed=src/decoder.cpp");
    println!("cargo:rerun-if-changed=screenios.png");
    println!("cargo:rustc-link-lib=comctl32");
    println!("cargo:rerun-if-env-changed=FFMPEG_ROOT");
}
