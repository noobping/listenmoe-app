use std::{env, fs, path::Path};

fn main() {
    let project = env::var("CARGO_PKG_NAME").expect("CARGO_PKG_NAME not set");
    #[cfg(not(feature = "setup"))]
    let version = env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION not set");
    #[cfg(not(feature = "setup"))]
    let comment = env::var("CARGO_PKG_DESCRIPTION").unwrap_or_else(|_| "Anime/Japanese Radio".to_string());
    let app_id = if cfg!(debug_assertions) {
        format!("dev.noobping.{project}.develop")
    } else {
        format!("dev.noobping.{project}")
    };
    let resource_id = format!("/dev/noobping/{project}");

    // Expose APP_ID and RESOURCE_ID to your main crate:
    println!("cargo:rustc-env=APP_ID={app_id}");
    println!("cargo:rustc-env=RESOURCE_ID={resource_id}");

    // Make Cargo rerun if these change
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=data");

    // Ensure data/ exists
    let data_dir = Path::new("data");
    fs::create_dir_all(&data_dir).unwrap();

    // Collect all .svg icon files under data/
    let mut icons: Vec<String> = Vec::new();
    collect_svg_icons(&data_dir, &data_dir, &mut icons);
    icons.sort();

    // Generate data/resources.xml
    let mut xml = String::from("<gresources>\n");
    xml.push_str(&format!("\t<gresource prefix=\"{resource_id}\">\n"));
    for f in &icons {
        xml.push_str(&format!("\t\t<file>{}</file>\n", f));
    }
    xml.push_str("\t</gresource>\n</gresources>\n");
    fs::write(data_dir.join("resources.xml"), xml).unwrap();

    // Compile GResources into $OUT_DIR/compiled.gresource
    glib_build_tools::compile_resources(&["data"], "data/resources.xml", "compiled.gresource");

    // Generate .desktop file for non-setup builds
    #[cfg(not(feature = "setup"))]
    desktop_file(&project, &version, &comment, &app_id);
}

/// Recursively collect all `.svg` files under `dir`,
/// and push their path *relative to `data_dir`* into `icons`.
fn collect_svg_icons(dir: &Path, data_dir: &Path, icons: &mut Vec<String>) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.is_dir() {
            collect_svg_icons(&path, data_dir, icons);
        } else if path.extension().and_then(|e| e.to_str()) == Some("svg") {
            let rel = path.strip_prefix(data_dir).unwrap();
            icons.push(rel.to_string_lossy().into_owned());
        }
    }
}

#[cfg(not(feature = "setup"))]
fn desktop_file(project: &str, version: &str, comment: &str, app_id: &str) {
    let dir = Path::new(".");
    let contents = format!(
        "[Desktop Entry]
Type=Application
Version={version}
Name={project}
Comment={comment}
Exec={project} %u
Icon={app_id}
Terminal=false
Categories=AudioVideo;Player;
"
    );
    fs::write(dir.join(format!("{project}.desktop")), contents)
        .expect("Can not build desktop file");
}
