//! Build the Slate icon and embed it as the executable's icon resource, so
//! Explorer, the taskbar and Alt+Tab show the logo even before the window sets
//! it.
//!
//! The candidate selected by the `SLATE_ICON` env var (default `slate`) is
//! embedded as the app icon. A candidate is either a `<name>.png` (turned into
//! a multi-size PNG-compressed `.ico`) or a ready-made `<name>.ico`. PNG
//! candidates also get their `.ico` written to `assets/icons/<name>.ico`.
//!
//! The resolved source path of the selected logo is exported as `SLATE_LOGO`
//! via `rustc-env`, so the runtime window icon and the empty-state mark embed
//! the same art (the `image` crate decodes both PNG and ICO).
//!
//! Candidates are looked up by name in the repo root, `assets/`, or
//! `assets/icons/`.
//!
//! Pick one with: `$env:SLATE_ICON = "slate"; cargo build`

/// Candidate logo names (without extension), searched in `LOGO_DIRS`.
const LOGO_SOURCES: &[&str] = &["slate"];
/// Used when `SLATE_ICON` is unset or names an unknown candidate.
const DEFAULT_LOGO: &str = "slate";
/// Directories (repo-relative) searched for `<name>.<ext>`, in order.
const LOGO_DIRS: &[&str] = &["", "assets", "assets/icons"];
/// Extensions searched, in order.
const LOGO_EXTS: &[&str] = &["png", "ico"];

/// Resolve a candidate name to an existing source file (`<name>.png` or
/// `<name>.ico`).
fn resolve(name: &str) -> Option<std::path::PathBuf> {
    for ext in LOGO_EXTS {
        let file = format!("{name}.{ext}");
        if let Some(path) = LOGO_DIRS
            .iter()
            .map(|dir| std::path::Path::new(dir).join(&file))
            .find(|path| path.exists())
        {
            return Some(path);
        }
    }
    None
}

fn main() {
    println!("cargo:rerun-if-env-changed=SLATE_ICON");
    for name in LOGO_SOURCES {
        if let Some(path) = resolve(name) {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }

    // `SLATE_ICON` may name any candidate, or any other PNG/ICO found in
    // `LOGO_DIRS`.
    let requested = std::env::var("SLATE_ICON").unwrap_or_default();
    let selected = if !requested.is_empty() && resolve(&requested).is_some() {
        resolve(&requested).expect("checked")
    } else {
        resolve(DEFAULT_LOGO)
            .unwrap_or_else(|| panic!("default logo `{DEFAULT_LOGO}` not found in {LOGO_DIRS:?}"))
    };

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR set by cargo");
    let icons_dir = std::path::Path::new("assets").join("icons");

    // Build a comparison .ico for every PNG candidate. Best-effort: these side
    // files are only for eyeballing variants, and a locked or memory-mapped
    // destination (Explorer, an editor) must never fail the build.
    for name in LOGO_SOURCES {
        if let Some(path) = resolve(name) {
            if path.extension().and_then(|e| e.to_str()) == Some("png") {
                if let Ok(bytes) = ico_bytes(&path) {
                    let _ = std::fs::create_dir_all(&icons_dir);
                    let _ = std::fs::write(icons_dir.join(format!("{name}.ico")), bytes);
                }
            }
        }
    }

    // Produce the icon embedded as the app resource.
    let ico_path = std::path::Path::new(&out_dir).join("slate.ico");
    if selected.extension().and_then(|e| e.to_str()) == Some("ico") {
        std::fs::copy(&selected, &ico_path)
            .unwrap_or_else(|e| panic!("copy {}: {e}", selected.display()));
    } else {
        make_ico(&selected, &ico_path);
    }

    let rc_path = std::path::Path::new(&out_dir).join("slate.rc");
    let ico_lit = ico_path.display().to_string().replace('\\', "\\\\");
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let (v1, v2, v3, v4) = version_quad(&version);
    let rc = format!(
        r#"1 ICON "{ico_lit}"
1 VERSIONINFO
FILEVERSION {v1},{v2},{v3},{v4}
PRODUCTVERSION {v1},{v2},{v3},{v4}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "FileDescription", "Slate"
      VALUE "FileVersion", "{version}"
      VALUE "ProductName", "Slate"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    );
    std::fs::write(&rc_path, rc).expect("write slate.rc");

    // A failed resource compile (missing/failed rc.exe) must not silently ship an
    // icon-less binary. `manifest_optional` still tolerates a non-Windows build.
    if let Err(err) = embed_resource::compile(&rc_path, embed_resource::NONE).manifest_optional() {
        panic!("embed icon/version resource: {err}");
    }

    // Hand the exact selected art to the crate (window icon + empty state).
    let abs = std::fs::canonicalize(&selected).unwrap_or(selected);
    println!("cargo:rustc-env=SLATE_LOGO={}", abs.display());
}

/// `x.y.z` (or any dotted form) as the four 16-bit VERSIONINFO fields.
fn version_quad(version: &str) -> (u16, u16, u16, u16) {
    let mut parts = version.split('.').map(|p| p.parse::<u16>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

/// Build the bytes of a PNG-compressed multi-size .ico from a PNG.
fn ico_bytes(png_path: &std::path::Path) -> Result<Vec<u8>, String> {
    use image::codecs::png::PngEncoder;
    use image::imageops::FilterType;
    use image::ImageEncoder;

    let source = image::open(png_path)
        .map_err(|e| format!("open {}: {e}", png_path.display()))?
        .to_rgba8();

    let sizes: [u32; 5] = [16, 32, 48, 64, 256];
    let mut images: Vec<Vec<u8>> = Vec::with_capacity(sizes.len());
    for &size in &sizes {
        let resized = image::imageops::resize(&source, size, size, FilterType::Lanczos3);
        let mut png = Vec::new();
        PngEncoder::new(&mut png)
            .write_image(
                resized.as_raw(),
                size,
                size,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|e| format!("encode png: {e}"))?;
        images.push(png);
    }

    // ICONDIR (6 bytes) + ICONDIRENTRY per image (16 bytes each).
    let header_len = 6 + 16 * images.len();
    let mut out = Vec::with_capacity(header_len + images.iter().map(Vec::len).sum::<usize>());
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // type: icon
    out.extend_from_slice(&(images.len() as u16).to_le_bytes());

    let mut offset = header_len as u32;
    for (&size, png) in sizes.iter().zip(&images) {
        let dim = if size >= 256 { 0u8 } else { size as u8 };
        out.push(dim); // width (0 means 256)
        out.push(dim); // height
        out.push(0); // palette count
        out.push(0); // reserved
        out.extend_from_slice(&1u16.to_le_bytes()); // color planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for png in &images {
        out.extend_from_slice(png);
    }
    Ok(out)
}

/// Write a PNG-compressed multi-size .ico from a PNG, creating the destination
/// directory if needed. Used for the embedded app icon, so failures are fatal.
fn make_ico(png_path: &std::path::Path, ico_path: &std::path::Path) {
    if let Some(dir) = ico_path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let bytes = ico_bytes(png_path).unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(ico_path, &bytes)
        .unwrap_or_else(|e| panic!("write {}: {e}", ico_path.display()));
}
