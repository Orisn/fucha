// Packs the translation catalogs and the language table for `mdm-core`: moved here from
// `crates/app/build.rs` when `i18n` left the egui app.
fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../i18n");
    catalogs(&root);
    languages(&root);
}

/// Embed every translation catalog, `i18n/<locale>.tsv` at the repository root (craft-i18n's
/// format; D-019), packed for `i18n::catalogs` to unpack once at start (#25):
/// `OUT_DIR/catalogs.br`, all of them in one brotli stream (quality 11: 12.4 MB of text in
/// about 0.8 MB, about 11 ms to unpack), and `OUT_DIR/catalogs.rs`, each locale's code and its
/// range in the unpacked text. The comment lines (`#`: translators' notes, `#, machine`) are
/// more than half of the text and craft-i18n skips them unread, so they are blanked, each kept as
/// an empty line so a problem's "line N" still points into the catalog file.
fn catalogs(root: &std::path::Path) {
    println!("cargo:rerun-if-changed={}", root.display());
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let mut text_all = String::new();
    let mut rows = Vec::new();
    if let Ok(dir) = std::fs::read_dir(root) {
        let mut files: Vec<_> = dir.flatten().map(|e| e.path()).collect();
        files.sort();
        for path in files {
            let (Some(stem), Some("tsv")) = (
                path.file_stem().and_then(|s| s.to_str()).map(str::to_owned),
                path.extension().and_then(|s| s.to_str()),
            ) else {
                continue;
            };
            // `en.tsv` lists the source texts (for translators and checks); English needs no
            // catalog at run time. `languages.tsv` is the list of languages, not a catalog.
            if stem == "en" || stem == "languages" {
                continue;
            }
            println!("cargo:rerun-if-changed={}", path.display());
            let text = std::fs::read_to_string(&path).expect("a translation catalog");
            let rows_only: Vec<&str> = text
                .split('\n')
                .map(|line| if line.starts_with('#') { "" } else { line })
                .collect();
            let start = text_all.len();
            text_all.push_str(&rows_only.join("\n"));
            rows.push(format!("    ({stem:?}, {start}, {}),", text_all.len()));
        }
    }
    let params = brotli::enc::BrotliEncoderParams {
        quality: 11,
        lgwin: 24,
        size_hint: text_all.len(),
        ..Default::default()
    };
    let mut packed = Vec::new();
    brotli::BrotliCompress(&mut text_all.as_bytes(), &mut packed, &params)
        .expect("packing the catalogs");
    std::fs::write(out_dir.join("catalogs.br"), packed).unwrap();
    std::fs::write(
        out_dir.join("catalogs.rs"),
        format!("&[\n{}\n]\n", rows.join("\n")),
    )
    .unwrap();
}

/// `i18n/languages.tsv` (D-019) as `(code, English name, own name, flag SVG)` rows, the flags
/// from `crates/app/assets/flags/` embedded: `OUT_DIR/languages.rs`, for the language picker.
fn languages(root: &std::path::Path) {
    let table = root.join("languages.tsv");
    println!("cargo:rerun-if-changed={}", table.display());
    let flags = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../app/assets/flags");
    println!("cargo:rerun-if-changed={}", flags.display());
    let text = std::fs::read_to_string(&table).expect("i18n/languages.tsv");
    let mut header: Vec<&str> = Vec::new();
    let mut rows = Vec::new();
    for line in text.lines() {
        if let Some(h) = line.strip_prefix("# code\t") {
            header = std::iter::once("code").chain(h.split('\t')).collect();
            continue;
        }
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        assert_eq!(cols.len(), header.len(), "i18n/languages.tsv: {line}");
        let col = |name: &str| cols[header.iter().position(|h| *h == name).expect(name)];
        let flag = flags.join(format!("{}.svg", col("flag")));
        assert!(flag.exists(), "missing flag {}", flag.display());
        let flag = flag.canonicalize().unwrap_or(flag);
        rows.push(format!(
            "    ({:?}, {:?}, {:?}, include_bytes!({:?})),",
            col("code"),
            col("english"),
            col("native"),
            flag.display().to_string()
        ));
    }
    let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("languages.rs");
    std::fs::write(out, format!("&[\n{}\n]\n", rows.join("\n"))).unwrap();
}
