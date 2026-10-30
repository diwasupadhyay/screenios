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
    println!("cargo:rustc-link-lib=shell32");
    println!("cargo:rustc-link-lib=advapi32");
    println!("cargo:rerun-if-env-changed=FFMPEG_ROOT");
    let compiler = cc::Build::new().cpp(true).get_compiler();
    let mut helper = compiler.to_command();
    helper
        .args([
            "/nologo",
            "/std:c++20",
            "/EHsc",
            "/O2",
            "/MD",
            "/DNOMINMAX",
            "/DWIN32_LEAN_AND_MEAN",
            "/Ivendor/quicktime",
        ])
        .args([
            "src/quicktime_helper.cpp",
            "vendor/quicktime/Protocol/QuickTimePacket.cpp",
            "vendor/quicktime/Protocol/QuickTimeSession.cpp",
            "vendor/quicktime/Media/CoreMedia.cpp",
        ])
        .arg(format!("/Fo{}\\", output.display()))
        .arg(format!(
            "/Fe{}",
            output.join("ScreenIOS.UsbCapture.exe").display()
        ))
        .args(["/link", ".local/libusb/libusb-1.0.lib", "advapi32.lib"]);
    assert!(
        helper.status().expect("Build USB capture helper").success(),
        "USB capture helper compilation failed"
    );
    let profile = output.ancestors().nth(3).unwrap();
    std::fs::copy(
        output.join("ScreenIOS.UsbCapture.exe"),
        profile.join("ScreenIOS.UsbCapture.exe"),
    )
    .unwrap();
    println!("cargo:rerun-if-changed=src/quicktime_helper.cpp");
    println!("cargo:rerun-if-changed=vendor/quicktime");
}
