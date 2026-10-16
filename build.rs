const LOGO_SOURCES: &[&str] = &["slate"];
const DEFAULT_LOGO: &str = "slate";
const LOGO_DIRS: &[&str] = &["", "assets", "assets/icons"];
const LOGO_EXTS: &[&str] = &["png", "ico"];

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

    let requested = std::env::var("SLATE_ICON").unwrap_or_default();
    let selected = if !requested.is_empty() && resolve(&requested).is_some() {
        resolve(&requested).expect("checked")
    } else {
        resolve(DEFAULT_LOGO)
            .unwrap_or_else(|| panic!("default logo `{DEFAULT_LOGO}` not found in {LOGO_DIRS:?}"))
    };

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR set by cargo");
    let icons_dir = std::path::Path::new("assets").join("icons");

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

    if let Err(err) = embed_resource::compile(&rc_path, embed_resource::NONE).manifest_optional() {
        panic!("embed icon/version resource: {err}");
    }

    let abs = std::fs::canonicalize(&selected).unwrap_or(selected);
    println!("cargo:rustc-env=SLATE_LOGO={}", abs.display());
}

fn version_quad(version: &str) -> (u16, u16, u16, u16) {
    let mut parts = version.split('.').map(|p| p.parse::<u16>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

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

    let header_len = 6 + 16 * images.len();
    let mut out = Vec::with_capacity(header_len + images.iter().map(Vec::len).sum::<usize>());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(images.len() as u16).to_le_bytes());

    let mut offset = header_len as u32;
    for (&size, png) in sizes.iter().zip(&images) {
        let dim = if size >= 256 { 0u8 } else { size as u8 };
        out.push(dim);
        out.push(dim);
        out.push(0);
        out.push(0);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for png in &images {
        out.extend_from_slice(png);
    }
    Ok(out)
}

fn make_ico(png_path: &std::path::Path, ico_path: &std::path::Path) {
    if let Some(dir) = ico_path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let bytes = ico_bytes(png_path).unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(ico_path, &bytes)
        .unwrap_or_else(|e| panic!("write {}: {e}", ico_path.display()));
}
