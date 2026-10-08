//! Runs a real, no-output libobs startup and scene composition smoke.

fn main() {
    match partytime_obs::smoke() {
        Ok(report) => println!(
            "libobs {} initialized audio/video; {} source attached to scene; output created: {}",
            report.libobs_version, report.scene_sources, report.created_output
        ),
        Err(error) => {
            eprintln!("libobs smoke failed: {error}");
            std::process::exit(1);
        }
    }
}
